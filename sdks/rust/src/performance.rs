//! Performance monitoring operations

use crate::client::NexusClient;
use crate::error::{NexusError, Result};
use serde::Deserialize;

/// Query statistics summary
#[derive(Debug, Clone, Deserialize)]
pub struct QueryStatisticsSummary {
    /// Total queries executed
    pub total_queries: u64,
    /// Successful queries
    pub successful_queries: u64,
    /// Failed queries
    pub failed_queries: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Number of slow queries logged
    pub slow_query_count: usize,
}

/// Query pattern statistics
#[derive(Debug, Clone, Deserialize)]
pub struct QueryPatternStats {
    /// Query pattern
    pub pattern: String,
    /// Number of times this pattern was executed
    pub count: u64,
    /// Average execution time in milliseconds
    pub avg_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_time_ms: u64,
    /// Number of successful executions
    pub success_count: u64,
    /// Number of failed executions
    pub failure_count: u64,
}

/// Query statistics response
#[derive(Debug, Clone, Deserialize)]
pub struct QueryStatisticsResponse {
    /// Overall statistics
    pub statistics: QueryStatisticsSummary,
    /// Pattern statistics
    pub patterns: Vec<QueryPatternStats>,
}

/// Slow query record
#[derive(Debug, Clone, Deserialize)]
pub struct SlowQueryRecord {
    /// Query string
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp
    pub timestamp: String,
    /// Whether the query was successful
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of rows returned
    pub rows_returned: u64,
}

/// Slow queries response
#[derive(Debug, Clone, Deserialize)]
pub struct SlowQueriesResponse {
    /// Number of slow queries
    pub count: usize,
    /// Slow query records
    pub queries: Vec<SlowQueryRecord>,
}

/// Plan cache statistics
#[derive(Debug, Clone, Deserialize)]
pub struct PlanCacheStatistics {
    /// Number of cached plans
    pub cached_plans: usize,
    /// Maximum cache size
    pub max_size: usize,
    /// Current memory usage in bytes
    pub current_memory_bytes: u64,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Plan cache statistics response
#[derive(Debug, Clone, Deserialize)]
pub struct PlanCacheStatisticsResponse {
    /// Number of cached plans
    pub cached_plans: usize,
    /// Maximum cache size
    pub max_size: usize,
    /// Current memory usage in bytes
    pub current_memory_bytes: u64,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: u64,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

impl NexusClient {
    /// Get query statistics
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let stats = client.get_query_statistics().await?;
    /// tracing::info!("Total queries: {}", stats.statistics.total_queries);
    /// tracing::info!("Average execution time: {}ms", stats.statistics.average_execution_time_ms);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_query_statistics(&self) -> Result<QueryStatisticsResponse> {
        let url = self.get_base_url().join("/performance/statistics")?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: QueryStatisticsResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Get slow queries
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let slow_queries = client.get_slow_queries().await?;
    /// tracing::info!("Found {} slow queries", slow_queries.count);
    /// for query in slow_queries.queries {
    ///     tracing::info!("Query: {} ({}ms)", query.query, query.execution_time_ms);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_slow_queries(&self) -> Result<SlowQueriesResponse> {
        let url = self.get_base_url().join("/performance/slow-queries")?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: SlowQueriesResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Get plan cache statistics
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let cache_stats = client.get_plan_cache_statistics().await?;
    /// tracing::info!("Cached plans: {}", cache_stats.cached_plans);
    /// tracing::info!("Hit rate: {:.2}%", cache_stats.hit_rate * 100.0);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_plan_cache_statistics(&self) -> Result<PlanCacheStatisticsResponse> {
        let url = self.get_base_url().join("/performance/plan-cache")?;
        let mut request_builder = self.get_client().get(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: PlanCacheStatisticsResponse = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }

    /// Clear plan cache
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use nexus_sdk::NexusClient;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), nexus_sdk::NexusError> {
    /// # let client = NexusClient::new("http://localhost:15474")?;
    /// let response = client.clear_plan_cache().await?;
    /// tracing::info!("Plan cache cleared: {:?}", response);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn clear_plan_cache(&self) -> Result<serde_json::Value> {
        let url = self.get_base_url().join("/performance/plan-cache/clear")?;
        let mut request_builder = self.get_client().post(url);

        request_builder = self.add_auth_headers(request_builder)?;

        let response = self.execute_with_retry(request_builder).await?;
        let status = response.status();

        if status.is_success() {
            let result: serde_json::Value = response.json().await?;
            Ok(result)
        } else {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            Err(NexusError::Api {
                message: error_text,
                status: status.as_u16(),
            })
        }
    }
}
