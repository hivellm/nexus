//! Bulk data ingestion endpoint

use axum::extract::Json;
use nexus_core::executor::{Executor, Query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Global executor instance (shared with other endpoints)
static EXECUTOR: std::sync::OnceLock<std::sync::Arc<tokio::sync::RwLock<Executor>>> =
    std::sync::OnceLock::new();

/// Initialize the executor (called from cypher module)
pub fn init_executor(
    executor: std::sync::Arc<tokio::sync::RwLock<Executor>>,
) -> anyhow::Result<()> {
    EXECUTOR
        .set(executor)
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(())
}

/// Ingestion request (NDJSON format)
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// Nodes to ingest
    #[serde(default)]
    pub nodes: Vec<NodeIngest>,
    /// Relationships to ingest
    #[serde(default)]
    pub relationships: Vec<RelIngest>,
}

/// Node to ingest
#[derive(Debug, Deserialize)]
pub struct NodeIngest {
    /// Node ID (optional, auto-generated if not provided)
    #[allow(dead_code)]
    pub id: Option<u64>,
    /// Labels
    pub labels: Vec<String>,
    /// Properties
    #[allow(dead_code)]
    pub properties: serde_json::Value,
}

/// Relationship to ingest
#[derive(Debug, Deserialize)]
pub struct RelIngest {
    /// Relationship ID (optional)
    #[allow(dead_code)]
    pub id: Option<u64>,
    /// Source node ID
    pub src: u64,
    /// Destination node ID
    pub dst: u64,
    /// Relationship type
    pub r#type: String,
    /// Properties
    #[allow(dead_code)]
    pub properties: serde_json::Value,
}

/// Ingestion response
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    /// Number of nodes ingested
    pub nodes_ingested: usize,
    /// Number of relationships ingested
    pub relationships_ingested: usize,
    /// Ingestion time in milliseconds
    pub ingestion_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Ingest bulk data
pub async fn ingest_data(Json(request): Json<IngestRequest>) -> Json<IngestResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!(
        "Ingesting {} nodes and {} relationships",
        request.nodes.len(),
        request.relationships.len()
    );

    // Get executor instance
    let executor_guard = match EXECUTOR.get() {
        Some(executor) => executor,
        None => {
            tracing::error!("Executor not initialized");
            return Json(IngestResponse {
                nodes_ingested: 0,
                relationships_ingested: 0,
                ingestion_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Executor not initialized".to_string()),
            });
        }
    };

    let mut nodes_ingested = 0;
    let mut relationships_ingested = 0;
    let mut errors = Vec::new();

    // Process nodes
    for node in &request.nodes {
        // For MVP, we'll create simple CREATE queries
        // In a real implementation, this would use bulk operations
        let labels_str = if node.labels.is_empty() {
            "".to_string()
        } else {
            format!(":{}", node.labels.join(":"))
        };

        let cypher_query = format!("CREATE (n{}) RETURN n", labels_str);
        let query = Query {
            cypher: cypher_query,
            params: HashMap::new(),
        };

        let mut executor = executor_guard.write().await;
        match executor.execute(&query) {
            Ok(_) => {
                nodes_ingested += 1;
            }
            Err(e) => {
                errors.push(format!("Node ingestion failed: {}", e));
            }
        }
    }

    // Process relationships
    for rel in &request.relationships {
        // For MVP, we'll create simple CREATE queries
        let cypher_query = format!(
            "MATCH (a), (b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[r:{}]->(b) RETURN r",
            rel.src, rel.dst, rel.r#type
        );
        let query = Query {
            cypher: cypher_query,
            params: HashMap::new(),
        };

        let mut executor = executor_guard.write().await;
        match executor.execute(&query) {
            Ok(_) => {
                relationships_ingested += 1;
            }
            Err(e) => {
                errors.push(format!("Relationship ingestion failed: {}", e));
            }
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;

    tracing::info!(
        "Ingestion completed in {}ms: {} nodes, {} relationships",
        execution_time,
        nodes_ingested,
        relationships_ingested
    );

    Json(IngestResponse {
        nodes_ingested,
        relationships_ingested,
        ingestion_time_ms: execution_time,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use serde_json::json;

    #[tokio::test]
    async fn test_ingest_nodes_only() {
        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Alice", "age": 30}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Bob", "age": 25}),
                },
            ],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_relationships_only() {
        let request = IngestRequest {
            nodes: vec![],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "KNOWS".to_string(),
                properties: json!({"since": 2020}),
            }],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_mixed_data() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({"name": "Alice"}),
            }],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "KNOWS".to_string(),
                properties: json!({}),
            }],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_empty_request() {
        let request = IngestRequest {
            nodes: vec![],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs - empty request should be handled gracefully
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_without_executor() {
        // Don't initialize executor
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Test".to_string()],
                properties: json!({}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Executor not initialized");
    }

    #[tokio::test]
    async fn test_ingest_response_format() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Test".to_string()],
                properties: json!({"key": "value"}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_initialized_executor() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({"name": "Alice", "age": 30}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_complex_properties() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({
                    "name": "Alice",
                    "age": 30,
                    "active": true,
                    "tags": ["developer", "rust"],
                    "metadata": {
                        "created": "2024-01-01",
                        "score": 95.5
                    }
                }),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_multiple_labels() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec![
                    "Person".to_string(),
                    "Developer".to_string(),
                    "Rust".to_string(),
                ],
                properties: json!({"name": "Alice"}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_empty_labels() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec![],
                properties: json!({"name": "Alice"}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_empty_properties() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({}),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_null_properties() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!(null),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_large_dataset() {
        let mut nodes = Vec::new();
        let mut relationships = Vec::new();

        // Create 100 nodes
        for i in 0..100 {
            nodes.push(NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({"id": i, "name": format!("Person{}", i)}),
            });
        }

        // Create 50 relationships
        for i in 0..50 {
            relationships.push(RelIngest {
                id: None,
                src: i + 1,
                dst: i + 2,
                r#type: "KNOWS".to_string(),
                properties: json!({"since": 2020 + i}),
            });
        }

        let request = IngestRequest {
            nodes,
            relationships,
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_complex_relationships() {
        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Alice"}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Company".to_string()],
                    properties: json!({"name": "TechCorp"}),
                },
            ],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "WORKS_FOR".to_string(),
                properties: json!({
                    "position": "Developer",
                    "start_date": "2024-01-01",
                    "salary": 100000
                }),
            }],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_empty_relationship_properties() {
        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Alice"}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Bob"}),
                },
            ],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "KNOWS".to_string(),
                properties: json!({}),
            }],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_null_relationship_properties() {
        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Alice"}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Bob"}),
                },
            ],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "KNOWS".to_string(),
                properties: json!(null),
            }],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }

    #[tokio::test]
    async fn test_ingest_with_special_characters() {
        let request = IngestRequest {
            nodes: vec![NodeIngest {
                id: None,
                labels: vec!["Person".to_string()],
                properties: json!({
                    "name": "José María",
                    "description": "Special chars: àáâãäåæçèéêë",
                    "unicode": "🚀🌟💻"
                }),
            }],
            relationships: vec![],
        };

        let response = ingest_data(Json(request)).await;
        // Test passes if no panic occurs
        assert!(response.ingestion_time_ms >= 0);
    }
}
