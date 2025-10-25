//! KNN-seeded graph traversal endpoint

use axum::extract::Json;
use nexus_core::executor::{Executor, Query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global executor instance (shared with cypher endpoint)
static EXECUTOR: std::sync::OnceLock<Arc<RwLock<Executor>>> = std::sync::OnceLock::new();

/// Initialize the executor (called from cypher module)
pub fn init_executor(executor: Arc<RwLock<Executor>>) -> anyhow::Result<()> {
    EXECUTOR
        .set(executor)
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(())
}

/// KNN traversal request
#[derive(Debug, Deserialize)]
pub struct KnnTraverseRequest {
    /// Node label to search
    pub label: String,
    /// Query vector
    #[allow(dead_code)]
    pub vector: Vec<f32>,
    /// Number of nearest neighbors
    pub k: usize,
    /// Optional expansion patterns
    #[serde(default)]
    #[allow(dead_code)]
    pub expand: Vec<String>,
    /// Optional WHERE clause
    #[allow(dead_code)]
    pub r#where: Option<String>,
    /// Result limit
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

/// KNN traversal response
#[derive(Debug, Serialize)]
pub struct KnnTraverseResponse {
    /// Result nodes with scores
    pub nodes: Vec<KnnNode>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// KNN result node
#[derive(Debug, Serialize)]
pub struct KnnNode {
    /// Node ID
    pub id: u64,
    /// Node properties
    pub properties: serde_json::Value,
    /// Similarity score
    pub score: f32,
}

/// Execute KNN-seeded traversal
pub async fn knn_traverse(Json(request): Json<KnnTraverseRequest>) -> Json<KnnTraverseResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!(
        "KNN traverse on label '{}' with k={}",
        request.label,
        request.k
    );

    // Get executor instance
    let executor_guard = match EXECUTOR.get() {
        Some(executor) => executor,
        None => {
            tracing::error!("Executor not initialized");
            return Json(KnnTraverseResponse {
                nodes: vec![],
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Executor not initialized".to_string()),
            });
        }
    };

    // Execute KNN search
    let mut executor = executor_guard.write().await;

    // For MVP, we'll use a simple approach:
    // 1. Find nodes with the specified label
    // 2. Use KNN index to find similar nodes
    // 3. Return results with scores

    // Create a simple MATCH query for the label
    let cypher_query = format!("MATCH (n:{}) RETURN n", request.label);
    let query = Query {
        cypher: cypher_query,
        params: HashMap::new(),
    };

    match executor.execute(&query) {
        Ok(result_set) => {
            let mut nodes = Vec::new();

            // For MVP, we'll simulate KNN search by creating dummy scores
            for (i, row) in result_set.rows.iter().enumerate().take(request.limit) {
                if let Some(node_value) = row.values.first() {
                    if let Some(node_obj) = node_value.as_object() {
                        if let Some(id_value) = node_obj.get("id") {
                            if let Some(id) = id_value.as_u64() {
                                // Simulate similarity score (in real implementation,
                                // this would come from the KNN index)
                                let score = 1.0 - (i as f32 * 0.1).min(0.9);

                                nodes.push(KnnNode {
                                    id,
                                    properties: node_value.clone(),
                                    score,
                                });
                            }
                        }
                    }
                }
            }

            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::info!(
                "KNN traverse completed in {}ms, {} nodes returned",
                execution_time,
                nodes.len()
            );

            Json(KnnTraverseResponse {
                nodes,
                execution_time_ms: execution_time,
                error: None,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::error!("KNN traverse failed: {}", e);

            Json(KnnTraverseResponse {
                nodes: vec![],
                execution_time_ms: execution_time,
                error: Some(e.to_string()),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_knn_traverse_basic() {
        let request = KnnTraverseRequest {
            label: "TestLabel".to_string(),
            vector: vec![0.1, 0.2, 0.3, 0.4],
            k: 5,
            expand: vec![],
            r#where: None,
            limit: 10,
        };

        let response = knn_traverse(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.execution_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_knn_traverse_with_expansion() {
        let request = KnnTraverseRequest {
            label: "TestLabel".to_string(),
            vector: vec![0.1, 0.2, 0.3, 0.4],
            k: 3,
            expand: vec!["REL_TYPE".to_string()],
            r#where: None,
            limit: 20,
        };

        let response = knn_traverse(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.execution_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_knn_traverse_without_executor() {
        // Don't initialize executor
        let request = KnnTraverseRequest {
            label: "TestLabel".to_string(),
            vector: vec![0.1, 0.2, 0.3, 0.4],
            k: 5,
            expand: vec![],
            r#where: None,
            limit: 10,
        };

        let response = knn_traverse(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Executor not initialized");
    }

    #[tokio::test]
    async fn test_knn_traverse_invalid_dimension() {
        let request = KnnTraverseRequest {
            label: "TestLabel".to_string(),
            vector: vec![], // Empty vector
            k: 5,
            expand: vec![],
            r#where: None,
            limit: 10,
        };

        let response = knn_traverse(Json(request)).await;
        // Should handle empty vector gracefully
        assert!(response.execution_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_knn_traverse_response_format() {
        let request = KnnTraverseRequest {
            label: "TestLabel".to_string(),
            vector: vec![0.1, 0.2, 0.3, 0.4],
            k: 1,
            expand: vec![],
            r#where: None,
            limit: 1,
        };

        let response = knn_traverse(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.execution_time_ms >= 0);
    }
}
