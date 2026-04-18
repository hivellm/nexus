//! `QueryExecutor` — runs correlation queries (semantic / metadata /
//! hybrid) against the vectorizer MCP, caching results via the advanced
//! `VectorizerCache`.

use super::*;

/// Query executor for running queries via MCP
#[derive(Debug)]
pub struct QueryExecutor {
    /// MCP client for vectorizer communication
    mcp_client: Option<serde_json::Value>,
    /// Advanced cache for query results
    vectorizer_cache: VectorizerCache,
}

impl QueryExecutor {
    /// Create a new query executor
    pub fn new() -> Self {
        Self {
            mcp_client: None,
            vectorizer_cache: VectorizerCache::new(),
        }
    }

    /// Create a new query executor with custom cache configuration
    pub fn with_cache_config(config: crate::vectorizer_cache::CacheConfig) -> Self {
        Self {
            mcp_client: None,
            vectorizer_cache: VectorizerCache::with_config(config),
        }
    }

    /// Set the MCP client
    pub fn set_mcp_client(&mut self, client: serde_json::Value) {
        self.mcp_client = Some(client);
    }

    /// Get cache statistics
    pub async fn get_cache_statistics(&self) -> crate::vectorizer_cache::CacheStatistics {
        self.vectorizer_cache.get_statistics().await
    }

    /// Get cache metrics
    pub async fn get_cache_metrics(&self) -> crate::performance::cache::CacheMetrics {
        self.vectorizer_cache.get_metrics().await
    }

    /// Clear the cache
    pub async fn clear_cache(&self) -> Result<()> {
        self.vectorizer_cache.clear().await
    }

    /// Invalidate cache entries matching a pattern
    pub async fn invalidate_cache_pattern(&self, pattern: &str) -> Result<usize> {
        self.vectorizer_cache.invalidate_pattern(pattern).await
    }

    /// Execute a semantic query
    pub async fn execute_semantic_query(&self, query: &SemanticQuery) -> Result<QueryResult> {
        let cache_key = format!("semantic:{}:{}", query.collection, query.query);

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute the query via MCP
        let results = self
            .perform_mcp_semantic_search(
                &query.collection,
                &query.query,
                query.limit,
                query.threshold,
            )
            .await?;

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(results.clone(), results.len(), execution_time_ms)
            .with_metadata(
                "query_type".to_string(),
                serde_json::Value::String("semantic".to_string()),
            )
            .with_metadata(
                "collection".to_string(),
                serde_json::Value::String(query.collection.clone()),
            );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: query.collection.clone(),
            query_string: query.query.clone(),
            threshold: query.threshold,
            limit: query.limit,
            filters: None,
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache
            .put(cache_key, result_json, query_metadata, None)
            .await?;

        Ok(result)
    }

    /// Execute a metadata query
    pub async fn execute_metadata_query(&self, query: &MetadataQuery) -> Result<QueryResult> {
        let cache_key = format!(
            "metadata:{}:{}",
            query.collection,
            serde_json::to_string(&query.filters).unwrap_or_default()
        );

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute the query via MCP
        let results = self
            .perform_mcp_metadata_search(
                &query.collection,
                &query.filters,
                query.limit,
                &query.sort_by,
                &query.sort_order,
            )
            .await?;

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(results.clone(), results.len(), execution_time_ms)
            .with_metadata(
                "query_type".to_string(),
                serde_json::Value::String("metadata".to_string()),
            )
            .with_metadata(
                "collection".to_string(),
                serde_json::Value::String(query.collection.clone()),
            );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "metadata".to_string(),
            collection: query.collection.clone(),
            query_string: serde_json::to_string(&query.filters).unwrap_or_default(),
            threshold: None,
            limit: query.limit,
            filters: Some(serde_json::to_value(&query.filters)?),
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache
            .put(cache_key, result_json, query_metadata, None)
            .await?;

        Ok(result)
    }

    /// Execute a hybrid query
    pub async fn execute_hybrid_query(&self, query: &HybridQuery) -> Result<QueryResult> {
        let cache_key = format!(
            "hybrid:{}:{}:{}",
            query.collection,
            query.semantic_query,
            serde_json::to_string(&query.metadata_filters).unwrap_or_default()
        );

        // Check cache first
        if let Some(cached_result) = self.vectorizer_cache.get(&cache_key).await? {
            return Ok(serde_json::from_value(cached_result)?);
        }

        let start_time = std::time::SystemTime::now();

        // Execute both semantic and metadata queries
        let semantic_results = self
            .perform_mcp_semantic_search(
                &query.collection,
                &query.semantic_query,
                query.limit,
                query.threshold,
            )
            .await?;
        let metadata_results = self
            .perform_mcp_metadata_search(
                &query.collection,
                &query.metadata_filters,
                query.limit,
                &None,
                &None,
            )
            .await?;

        // Combine results using RRF (Reciprocal Rank Fusion)
        let combined_results =
            self.combine_results(semantic_results, metadata_results, query.semantic_weight);

        let execution_time = start_time.elapsed().unwrap_or_default();
        let execution_time_ms = execution_time.as_millis() as u64;

        let result = QueryResult::new(
            combined_results.clone(),
            combined_results.len(),
            execution_time_ms,
        )
        .with_metadata(
            "query_type".to_string(),
            serde_json::Value::String("hybrid".to_string()),
        )
        .with_metadata(
            "collection".to_string(),
            serde_json::Value::String(query.collection.clone()),
        )
        .with_metadata(
            "semantic_weight".to_string(),
            serde_json::Value::Number(
                serde_json::Number::from_f64(query.semantic_weight as f64).unwrap(),
            ),
        );

        // Cache the result
        let query_metadata = QueryMetadata {
            query_type: "hybrid".to_string(),
            collection: query.collection.clone(),
            query_string: format!(
                "{} + {}",
                query.semantic_query,
                serde_json::to_string(&query.metadata_filters).unwrap_or_default()
            ),
            threshold: query.threshold,
            limit: query.limit,
            filters: Some(serde_json::to_value(&query.metadata_filters)?),
        };

        let result_json = serde_json::to_value(&result)?;
        self.vectorizer_cache
            .put(cache_key, result_json, query_metadata, None)
            .await?;

        Ok(result)
    }

    /// Perform MCP semantic search (placeholder implementation)
    async fn perform_mcp_semantic_search(
        &self,
        _collection: &str,
        _query: &str,
        _limit: Option<usize>,
        _threshold: Option<f32>,
    ) -> Result<Vec<serde_json::Value>> {
        // This is a placeholder implementation
        // In a real implementation, this would use the MCP client to call vectorizer tools
        // For now, return empty results
        Ok(vec![])
    }

    /// Perform MCP metadata search (placeholder implementation)
    async fn perform_mcp_metadata_search(
        &self,
        _collection: &str,
        _filters: &HashMap<String, serde_json::Value>,
        _limit: Option<usize>,
        _sort_by: &Option<String>,
        _sort_order: &Option<SortOrder>,
    ) -> Result<Vec<serde_json::Value>> {
        // This is a placeholder implementation
        // In a real implementation, this would use the MCP client to call vectorizer tools
        // For now, return empty results
        Ok(vec![])
    }

    /// Combine semantic and metadata results using RRF
    fn combine_results(
        &self,
        semantic_results: Vec<serde_json::Value>,
        metadata_results: Vec<serde_json::Value>,
        _semantic_weight: f32,
    ) -> Vec<serde_json::Value> {
        // Simple RRF implementation
        // In a real implementation, this would use proper RRF scoring
        let mut combined = semantic_results;
        combined.extend(metadata_results);

        // Remove duplicates based on ID field
        let mut seen_ids = std::collections::HashSet::new();
        combined.retain(|item| {
            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                seen_ids.insert(id.to_string())
            } else {
                true
            }
        });

        // Limit results if needed
        if combined.len() > 1000 {
            combined.truncate(1000);
        }

        combined
    }
}

impl Default for QueryExecutor {
    fn default() -> Self {
        Self::new()
    }
}
