//! Cypher query execution endpoint

use axum::extract::Json;
use nexus_core::executor::{Executor, Query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global executor instance
static EXECUTOR: std::sync::OnceLock<Arc<RwLock<Executor>>> = std::sync::OnceLock::new();

/// Initialize the executor
pub fn init_executor() -> anyhow::Result<Arc<RwLock<Executor>>> {
    let executor = Executor::default();
    let executor_arc = Arc::new(RwLock::new(executor));
    EXECUTOR
        .set(executor_arc.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(executor_arc)
}

/// Cypher query request
#[derive(Debug, Deserialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
}

/// Cypher query response
#[derive(Debug, Serialize)]
pub struct CypherResponse {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<serde_json::Value>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execute Cypher query
pub async fn execute_cypher(Json(request): Json<CypherRequest>) -> Json<CypherResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!("Executing Cypher query: {}", request.query);

    // Get executor instance
    let executor_guard = match EXECUTOR.get() {
        Some(executor) => executor,
        None => {
            tracing::error!("Executor not initialized");
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Executor not initialized".to_string()),
            });
        }
    };

    // Create query
    let query = Query {
        cypher: request.query.clone(),
        params: request.params,
    };

    // Execute query
    let mut executor = executor_guard.write().await;
    match executor.execute(&query) {
        Ok(result_set) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::info!(
                "Query executed successfully in {}ms, {} rows returned",
                execution_time,
                result_set.rows.len()
            );

            Json(CypherResponse {
                columns: result_set.columns,
                rows: result_set
                    .rows
                    .into_iter()
                    .map(|row| serde_json::Value::Array(row.values))
                    .collect(),
                execution_time_ms: execution_time,
                error: None,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::error!("Query execution failed: {}", e);

            Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: execution_time,
                error: Some(e.to_string()),
            })
        }
    }
}
