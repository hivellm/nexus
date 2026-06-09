//! Collection query types: `CollectionQuery` trait, query structs
//! (`SemanticQuery`, `MetadataQuery`, `HybridQuery`), `QueryResult`,
//! `QueryBuilder`, and the `CollectionQueryEnum` dispatch wrapper.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::query_executor::QueryExecutor;

/// Trait for different types of collection queries
pub trait CollectionQuery {
    /// Execute the query and return results
    #[allow(async_fn_in_trait)]
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult>;

    /// Get the collection name for this query
    fn collection(&self) -> &str;

    /// Get query parameters as JSON
    fn parameters(&self) -> serde_json::Value;
}

/// Enum wrapper for different collection query types
pub enum CollectionQueryEnum {
    Semantic(SemanticQuery),
    Metadata(MetadataQuery),
    Hybrid(HybridQuery),
}

impl CollectionQuery for CollectionQueryEnum {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        match self {
            CollectionQueryEnum::Semantic(query) => query.execute(executor).await,
            CollectionQueryEnum::Metadata(query) => query.execute(executor).await,
            CollectionQueryEnum::Hybrid(query) => query.execute(executor).await,
        }
    }

    fn collection(&self) -> &str {
        match self {
            CollectionQueryEnum::Semantic(query) => query.collection(),
            CollectionQueryEnum::Metadata(query) => query.collection(),
            CollectionQueryEnum::Hybrid(query) => query.collection(),
        }
    }

    fn parameters(&self) -> serde_json::Value {
        match self {
            CollectionQueryEnum::Semantic(query) => query.parameters(),
            CollectionQueryEnum::Metadata(query) => query.parameters(),
            CollectionQueryEnum::Hybrid(query) => query.parameters(),
        }
    }
}

/// Semantic search query for finding similar content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticQuery {
    /// Collection to search
    pub collection: String,
    /// Search query text
    pub query: String,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Similarity threshold (0.0 to 1.0)
    pub threshold: Option<f32>,
    /// Additional filters
    pub filters: Option<HashMap<String, serde_json::Value>>,
}

impl SemanticQuery {
    /// Create a new semantic query
    pub fn new(collection: String, query: String) -> Self {
        Self {
            collection,
            query,
            limit: None,
            threshold: None,
            filters: None,
        }
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Add a filter to the query
    pub fn with_filter(mut self, key: String, value: serde_json::Value) -> Self {
        if self.filters.is_none() {
            self.filters = Some(HashMap::new());
        }
        if let Some(ref mut filters) = self.filters {
            filters.insert(key, value);
        }
        self
    }
}

impl CollectionQuery for SemanticQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_semantic_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "query": self.query,
            "type": "semantic"
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(threshold) = self.threshold {
            params["threshold"] =
                serde_json::Value::Number(serde_json::Number::from_f64(threshold as f64).unwrap());
        }

        if let Some(ref filters) = self.filters {
            params["filters"] = serde_json::Value::Object(
                filters
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            );
        }

        params
    }
}

/// Sort order for queries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    /// Ascending order
    Asc,
    /// Descending order
    Desc,
}

/// Metadata-based query for filtering by specific fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataQuery {
    /// Collection to search
    pub collection: String,
    /// Field filters
    pub filters: HashMap<String, serde_json::Value>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Sort by field
    pub sort_by: Option<String>,
    /// Sort order (asc/desc)
    pub sort_order: Option<SortOrder>,
}

impl MetadataQuery {
    /// Create a new metadata query
    pub fn new(collection: String) -> Self {
        Self {
            collection,
            filters: HashMap::new(),
            limit: None,
            sort_by: None,
            sort_order: None,
        }
    }

    /// Add a field filter
    pub fn with_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set sorting
    pub fn with_sort(mut self, field: String, order: SortOrder) -> Self {
        self.sort_by = Some(field);
        self.sort_order = Some(order);
        self
    }
}

impl CollectionQuery for MetadataQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_metadata_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "type": "metadata",
            "filters": self.filters
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(ref sort_by) = self.sort_by {
            params["sort_by"] = serde_json::Value::String(sort_by.clone());
        }

        if let Some(sort_order) = self.sort_order {
            params["sort_order"] = serde_json::Value::String(
                match sort_order {
                    SortOrder::Asc => "asc",
                    SortOrder::Desc => "desc",
                }
                .to_string(),
            );
        }

        params
    }
}

/// Hybrid query combining semantic and metadata search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridQuery {
    /// Collection to search
    pub collection: String,
    /// Semantic search query
    pub semantic_query: String,
    /// Metadata filters
    pub metadata_filters: HashMap<String, serde_json::Value>,
    /// Maximum number of results
    pub limit: Option<usize>,
    /// Similarity threshold for semantic search
    pub threshold: Option<f32>,
    /// Weight for semantic vs metadata results (0.0 to 1.0)
    pub semantic_weight: f32,
}

impl HybridQuery {
    /// Create a new hybrid query
    pub fn new(collection: String, semantic_query: String) -> Self {
        Self {
            collection,
            semantic_query,
            metadata_filters: HashMap::new(),
            limit: None,
            threshold: None,
            semantic_weight: 0.7, // Default 70% semantic, 30% metadata
        }
    }

    /// Add a metadata filter
    pub fn with_metadata_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.metadata_filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn with_threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Set the semantic weight
    pub fn with_semantic_weight(mut self, weight: f32) -> Self {
        self.semantic_weight = weight.clamp(0.0, 1.0);
        self
    }
}

impl CollectionQuery for HybridQuery {
    async fn execute(&self, executor: &QueryExecutor) -> Result<QueryResult> {
        executor.execute_hybrid_query(self).await
    }

    fn collection(&self) -> &str {
        &self.collection
    }

    fn parameters(&self) -> serde_json::Value {
        let mut params = serde_json::json!({
            "type": "hybrid",
            "semantic_query": self.semantic_query,
            "metadata_filters": self.metadata_filters,
            "semantic_weight": self.semantic_weight
        });

        if let Some(limit) = self.limit {
            params["limit"] = serde_json::Value::Number(serde_json::Number::from(limit));
        }

        if let Some(threshold) = self.threshold {
            params["threshold"] =
                serde_json::Value::Number(serde_json::Number::from_f64(threshold as f64).unwrap());
        }

        params
    }
}

/// Query result containing search results and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResult {
    /// Search results
    pub results: Vec<serde_json::Value>,
    /// Total number of results found
    pub total: usize,
    /// Query execution time in milliseconds
    pub execution_time_ms: u64,
    /// Query metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

impl QueryResult {
    /// Create a new query result
    pub fn new(results: Vec<serde_json::Value>, total: usize, execution_time_ms: u64) -> Self {
        Self {
            results,
            total,
            execution_time_ms,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to the result
    pub fn with_metadata(mut self, key: String, value: serde_json::Value) -> Self {
        self.metadata.insert(key, value);
        self
    }

    /// Check if the result is empty
    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
    }

    /// Get the number of results
    pub fn len(&self) -> usize {
        self.results.len()
    }
}

/// Types of queries that can be built
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryType {
    /// Semantic search only
    Semantic,
    /// Metadata filtering only
    Metadata,
    /// Hybrid search
    Hybrid,
}

/// Query builder for constructing complex queries
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    collection: String,
    query_type: QueryType,
    semantic_query: Option<String>,
    metadata_filters: HashMap<String, serde_json::Value>,
    limit: Option<usize>,
    threshold: Option<f32>,
    sort_by: Option<String>,
    sort_order: Option<SortOrder>,
    semantic_weight: f32,
}

impl QueryBuilder {
    /// Create a new query builder
    pub fn new(collection: String) -> Self {
        Self {
            collection,
            query_type: QueryType::Semantic,
            semantic_query: None,
            metadata_filters: HashMap::new(),
            limit: None,
            threshold: None,
            sort_by: None,
            sort_order: None,
            semantic_weight: 0.7,
        }
    }

    /// Set the query type
    pub fn query_type(mut self, query_type: QueryType) -> Self {
        self.query_type = query_type;
        self
    }

    /// Set the semantic query
    pub fn semantic_query(mut self, query: String) -> Self {
        self.semantic_query = Some(query);
        self
    }

    /// Add a metadata filter
    pub fn metadata_filter(mut self, field: String, value: serde_json::Value) -> Self {
        self.metadata_filters.insert(field, value);
        self
    }

    /// Set the maximum number of results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the similarity threshold
    pub fn threshold(mut self, threshold: f32) -> Self {
        self.threshold = Some(threshold);
        self
    }

    /// Set sorting
    pub fn sort(mut self, field: String, order: SortOrder) -> Self {
        self.sort_by = Some(field);
        self.sort_order = Some(order);
        self
    }

    /// Set the semantic weight for hybrid queries
    pub fn semantic_weight(mut self, weight: f32) -> Self {
        self.semantic_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Build the query
    pub fn build(self) -> Result<CollectionQueryEnum> {
        match self.query_type {
            QueryType::Semantic => {
                let query = self.semantic_query.ok_or_else(|| {
                    Error::GraphCorrelation(
                        "Semantic query is required for semantic search".to_string(),
                    )
                })?;

                let mut semantic_query = SemanticQuery::new(self.collection, query);
                if let Some(limit) = self.limit {
                    semantic_query = semantic_query.with_limit(limit);
                }
                if let Some(threshold) = self.threshold {
                    semantic_query = semantic_query.with_threshold(threshold);
                }
                for (key, value) in self.metadata_filters {
                    semantic_query = semantic_query.with_filter(key, value);
                }

                Ok(CollectionQueryEnum::Semantic(semantic_query))
            }
            QueryType::Metadata => {
                let mut metadata_query = MetadataQuery::new(self.collection);
                for (key, value) in self.metadata_filters {
                    metadata_query = metadata_query.with_filter(key, value);
                }
                if let Some(limit) = self.limit {
                    metadata_query = metadata_query.with_limit(limit);
                }
                if let (Some(sort_by), Some(sort_order)) = (self.sort_by, self.sort_order) {
                    metadata_query = metadata_query.with_sort(sort_by, sort_order);
                }

                Ok(CollectionQueryEnum::Metadata(metadata_query))
            }
            QueryType::Hybrid => {
                let query = self.semantic_query.ok_or_else(|| {
                    Error::GraphCorrelation(
                        "Semantic query is required for hybrid search".to_string(),
                    )
                })?;

                let mut hybrid_query = HybridQuery::new(self.collection, query);
                for (key, value) in self.metadata_filters {
                    hybrid_query = hybrid_query.with_metadata_filter(key, value);
                }
                if let Some(limit) = self.limit {
                    hybrid_query = hybrid_query.with_limit(limit);
                }
                if let Some(threshold) = self.threshold {
                    hybrid_query = hybrid_query.with_threshold(threshold);
                }
                hybrid_query = hybrid_query.with_semantic_weight(self.semantic_weight);

                Ok(CollectionQueryEnum::Hybrid(hybrid_query))
            }
        }
    }
}
