//! Bulk data ingestion endpoint

use crate::NexusServer;
use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};

/// Ingestion request (NDJSON format)
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// Nodes to ingest
    #[serde(default)]
    pub nodes: Vec<NodeIngest>,
    /// Relationships to ingest
    #[serde(default)]
    pub relationships: Vec<RelIngest>,
    /// Batch size for transaction batching (default: 1000)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Whether to use transaction batching (default: true)
    #[serde(default = "default_use_batching")]
    pub use_batching: bool,
}

fn default_batch_size() -> usize {
    1000
}

fn default_use_batching() -> bool {
    true
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
    /// Number of batches processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batches_processed: Option<usize>,
    /// Progress percentage (0-100)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_percent: Option<f64>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Ingest bulk data
pub async fn ingest_data(
    State(server): State<std::sync::Arc<NexusServer>>,
    Json(request): Json<IngestRequest>,
) -> Json<IngestResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!(
        "Ingesting {} nodes and {} relationships (batch_size: {}, use_batching: {})",
        request.nodes.len(),
        request.relationships.len(),
        request.batch_size,
        request.use_batching
    );

    let total_items = request.nodes.len() + request.relationships.len();
    let mut nodes_ingested = 0;
    let mut relationships_ingested = 0;
    let mut errors = Vec::new();
    let mut batches_processed = 0;

    if request.use_batching && total_items > request.batch_size {
        // Use transaction batching for large imports
        batches_processed = process_with_batching(
            &server,
            &request,
            &mut nodes_ingested,
            &mut relationships_ingested,
            &mut errors,
        )
        .await;
    } else {
        // Process without batching (small imports or batching disabled)
        process_without_batching(
            &server,
            &request,
            &mut nodes_ingested,
            &mut relationships_ingested,
            &mut errors,
        )
        .await;
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    let progress_percent = if total_items > 0 {
        Some((nodes_ingested + relationships_ingested) as f64 / total_items as f64 * 100.0)
    } else {
        Some(100.0)
    };

    tracing::info!(
        "Ingestion completed in {}ms: {} nodes, {} relationships, {} batches",
        execution_time,
        nodes_ingested,
        relationships_ingested,
        batches_processed
    );

    Json(IngestResponse {
        nodes_ingested,
        relationships_ingested,
        ingestion_time_ms: execution_time,
        batches_processed: if batches_processed > 0 {
            Some(batches_processed)
        } else {
            None
        },
        progress_percent,
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        },
    })
}

/// Process ingestion with transaction batching
async fn process_with_batching(
    server: &std::sync::Arc<NexusServer>,
    request: &IngestRequest,
    nodes_ingested: &mut usize,
    relationships_ingested: &mut usize,
    errors: &mut Vec<String>,
) -> usize {
    let mut batches_processed = 0;
    let batch_size = request.batch_size;

    // Process nodes in batches
    for batch in request.nodes.chunks(batch_size) {
        batches_processed += 1;
        let mut batch_nodes = 0;
        let mut batch_errors = Vec::new();

        // Start transaction for this batch
        let mut engine = server.engine.write().await;

        // Begin transaction
        let begin_query = "BEGIN TRANSACTION".to_string();
        if let Err(e) = engine.execute_cypher(&begin_query) {
            errors.push(format!(
                "Failed to begin transaction for batch {}: {}",
                batches_processed, e
            ));
            continue;
        }
        drop(engine);

        // Process nodes in this batch
        for node in batch {
            match create_node_in_batch(server, node).await {
                Ok(_) => batch_nodes += 1,
                Err(e) => batch_errors.push(format!("Node creation failed: {}", e)),
            }
        }

        // Commit transaction
        let mut engine = server.engine.write().await;
        let commit_query = "COMMIT TRANSACTION".to_string();
        if let Err(e) = engine.execute_cypher(&commit_query) {
            batch_errors.push(format!("Transaction commit failed: {}", e));
        }
        drop(engine);

        *nodes_ingested += batch_nodes;
        errors.extend(batch_errors);
    }

    // Process relationships in batches
    for batch in request.relationships.chunks(batch_size) {
        batches_processed += 1;
        let mut batch_rels = 0;
        let mut batch_errors = Vec::new();

        // Start transaction for this batch
        let mut engine = server.engine.write().await;

        // Begin transaction
        let begin_query = "BEGIN TRANSACTION".to_string();
        if let Err(e) = engine.execute_cypher(&begin_query) {
            errors.push(format!(
                "Failed to begin transaction for batch {}: {}",
                batches_processed, e
            ));
            continue;
        }
        drop(engine);

        // Process relationships in this batch
        for rel in batch {
            match create_relationship_in_batch(server, rel).await {
                Ok(_) => batch_rels += 1,
                Err(e) => batch_errors.push(format!("Relationship creation failed: {}", e)),
            }
        }

        // Commit transaction
        let mut engine = server.engine.write().await;
        let commit_query = "COMMIT TRANSACTION".to_string();
        if let Err(e) = engine.execute_cypher(&commit_query) {
            batch_errors.push(format!("Transaction commit failed: {}", e));
        }
        drop(engine);

        *relationships_ingested += batch_rels;
        errors.extend(batch_errors);
    }

    batches_processed
}

/// Process ingestion without batching
async fn process_without_batching(
    server: &std::sync::Arc<NexusServer>,
    request: &IngestRequest,
    nodes_ingested: &mut usize,
    relationships_ingested: &mut usize,
    errors: &mut Vec<String>,
) {
    // Process nodes
    for node in &request.nodes {
        match create_node_in_batch(server, node).await {
            Ok(_) => *nodes_ingested += 1,
            Err(e) => errors.push(format!("Node ingestion failed: {}", e)),
        }
    }

    // Process relationships
    for rel in &request.relationships {
        match create_relationship_in_batch(server, rel).await {
            Ok(_) => *relationships_ingested += 1,
            Err(e) => errors.push(format!("Relationship ingestion failed: {}", e)),
        }
    }
}

/// Create a node in batch
async fn create_node_in_batch(
    server: &std::sync::Arc<NexusServer>,
    node: &NodeIngest,
) -> Result<(), String> {
    let labels_str = if node.labels.is_empty() {
        "".to_string()
    } else {
        format!(":{}", node.labels.join(":"))
    };

    // Build properties string
    let props_str = if node.properties.is_object() {
        let props_map = node.properties.as_object().unwrap();
        if props_map.is_empty() {
            String::new()
        } else {
            let props: Vec<String> = props_map
                .iter()
                .map(|(k, v)| {
                    let v_str = serde_json::to_string(v).unwrap_or_else(|_| "null".to_string());
                    format!("{}: {}", k, v_str)
                })
                .collect();
            format!(" {{{}}}", props.join(", "))
        }
    } else {
        String::new()
    };

    let cypher_query = format!("CREATE (n{}{}) RETURN n", labels_str, props_str);

    let mut engine = server.engine.write().await;
    engine
        .execute_cypher(&cypher_query)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a relationship in batch
async fn create_relationship_in_batch(
    server: &std::sync::Arc<NexusServer>,
    rel: &RelIngest,
) -> Result<(), String> {
    // Build properties string
    let props_str = if rel.properties.is_object() {
        let props_map = rel.properties.as_object().unwrap();
        if props_map.is_empty() {
            String::new()
        } else {
            let props: Vec<String> = props_map
                .iter()
                .map(|(k, v)| {
                    let v_str = serde_json::to_string(v).unwrap_or_else(|_| "null".to_string());
                    format!("{}: {}", k, v_str)
                })
                .collect();
            format!(" {{{}}}", props.join(", "))
        }
    } else {
        String::new()
    };

    let cypher_query = format!(
        "MATCH (a), (b) WHERE id(a) = {} AND id(b) = {} CREATE (a)-[r:{}{}]->(b) RETURN r",
        rel.src, rel.dst, rel.r#type, props_str
    );

    let mut engine = server.engine.write().await;
    engine
        .execute_cypher(&cypher_query)
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RootUserConfig;
    use axum::extract::{Json, State};
    use nexus_core::{
        Engine,
        auth::{
            AuditConfig, AuditLogger, AuthConfig, AuthManager, JwtConfig, JwtManager,
            RoleBasedAccessControl,
        },
        database::DatabaseManager,
        executor::Executor,
    };
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    /// Helper function to create a test server
    /// Returns (TempDir, Arc<NexusServer>) - TempDir must be kept alive for the duration of the test
    async fn create_test_server() -> (TempDir, Arc<NexusServer>) {
        let temp_dir = TempDir::new().unwrap();
        let engine = Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager = DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));

        let rbac = RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(auth_config));

        let jwt_config = JwtConfig::default();
        let jwt_manager = Arc::new(JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            AuditLogger::new(AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        let server = Arc::new(NexusServer::new(
            executor_arc,
            engine_arc,
            database_manager_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            RootUserConfig::default(),
        ));

        (temp_dir, server)
    }

    #[tokio::test]
    #[ignore] // TODO: Fix LMDB BadRslot error - likely due to concurrent access issues
    async fn test_ingest_nodes_only() {
        let (_temp_dir, server) = create_test_server().await;
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
            batch_size: 1000,
            use_batching: false,
        };

        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_ingest_relationships_only() {
        let (_temp_dir, server) = create_test_server().await;
        let request = IngestRequest {
            nodes: vec![],
            relationships: vec![RelIngest {
                id: None,
                src: 1,
                dst: 2,
                r#type: "KNOWS".to_string(),
                properties: json!({"since": 2020}),
            }],
            batch_size: 1000,
            use_batching: false,
        };

        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_ingest_mixed_data() {
        let (_temp_dir, server) = create_test_server().await;
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
            batch_size: 1000,
            use_batching: false,
        };

        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_ingest_empty_request() {
        let (_temp_dir, server) = create_test_server().await;
        let request = IngestRequest {
            nodes: vec![],
            relationships: vec![],
            batch_size: 1000,
            use_batching: false,
        };

        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs - empty request should be handled gracefully
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: true,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    #[ignore] // Parser issue with special characters - needs fix in parser
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
            batch_size: 1000,
            use_batching: false,
        };

        let (_temp_dir, server) = create_test_server().await;
        let _response = ingest_data(State(server), Json(request)).await;
        // Test passes if no panic occurs
    }
}
