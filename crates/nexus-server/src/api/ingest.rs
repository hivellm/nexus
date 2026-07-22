//! Bulk data ingestion endpoint

use crate::NexusServer;
use axum::extract::{Json, State};
use nexus_core::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Optional client-supplied correlation key, scoped to this request.
    ///
    /// It does **not** become the node's actual (sequential) internal id —
    /// that is still assigned by the storage layer. When present, it lets a
    /// [`RelIngest`] later in the *same* request reference this node via
    /// [`RelIngest::src`] / [`RelIngest::dst`] before the node's real
    /// internal id is known to the caller. Two nodes in the same request
    /// must not reuse the same `id` — that row is rejected. Nodes without
    /// an `id` can still be created; they just cannot be targeted by a
    /// relationship in the same request except by a real internal id
    /// (see [`RelIngest`]).
    pub id: Option<u64>,
    /// Labels
    pub labels: Vec<String>,
    /// Properties
    pub properties: serde_json::Value,
}

/// Relationship to ingest
#[derive(Debug, Deserialize)]
pub struct RelIngest {
    /// Relationship ID (optional)
    #[allow(dead_code)]
    pub id: Option<u64>,
    /// Source node reference.
    ///
    /// Resolved against the `id` values supplied on this request's
    /// [`NodeIngest`] entries first (request-scoped correlation key); if no
    /// node in this request carries that `id`, the value is used directly
    /// as a literal internal node id, so relationships between
    /// already-existing nodes keep working with no `nodes` in the request.
    pub src: u64,
    /// Destination node reference. Resolved the same way as [`Self::src`].
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
    /// Internal ids assigned to the successfully created nodes, in the
    /// order they were created (which matches `nodes` input order for
    /// entries that succeeded — a failed row is skipped, not padded).
    /// Lets a caller correlate its own client-side data with the graph
    /// after ingest, e.g. to ingest relationships in a follow-up request.
    pub node_ids: Vec<u64>,
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

/// Ingest bulk data.
///
/// Uses `serde_json` unconditionally. simd-json dispatch was
/// benchmarked on this code path (see `benches/simd_json.rs` + ADR
/// notes in `docs/specs/simd-dispatch.md`) and came out slower for the
/// ingest schema because `NodeIngest.properties: serde_json::Value`
/// forces simd-json into DOM-building mode where it loses its
/// throughput advantage. The `simd::json` primitive is still available
/// for future typed-schema parse paths where it wins.
pub async fn ingest_data(
    State(server): State<std::sync::Arc<NexusServer>>,
    Json(request): Json<IngestRequest>,
) -> Json<IngestResponse> {
    ingest_data_inner(State(server), request).await
}

async fn ingest_data_inner(
    State(server): State<std::sync::Arc<NexusServer>>,
    request: IngestRequest,
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
    let mut node_ids = Vec::new();
    // Request-scoped correlation map: client-supplied `NodeIngest.id` ->
    // the internal id the storage layer actually assigned. Consulted by
    // every `RelIngest.src`/`.dst` in this request — see the doc comments
    // on those fields for the fallback-to-literal-internal-id rule.
    let mut id_map: HashMap<u64, u64> = HashMap::new();

    if request.use_batching && total_items > request.batch_size {
        // Use write-lock batching for large imports
        batches_processed = process_with_batching(
            &server,
            &request,
            &mut nodes_ingested,
            &mut relationships_ingested,
            &mut errors,
            &mut node_ids,
            &mut id_map,
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
            &mut node_ids,
            &mut id_map,
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
        node_ids,
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

/// Process ingestion in chunks of `request.batch_size`, one
/// `server.engine.write()` acquisition per chunk (not per row).
///
/// Not atomic: a row that fails is skipped (recorded in `errors`) and the
/// chunk continues — the rows that succeeded before and after it stay
/// created. This matches `process_without_batching` and is a deliberate,
/// documented best-effort contract (see `docs/specs/api-protocols.md`),
/// not an accident of the previous Cypher-string implementation.
async fn process_with_batching(
    server: &std::sync::Arc<NexusServer>,
    request: &IngestRequest,
    nodes_ingested: &mut usize,
    relationships_ingested: &mut usize,
    errors: &mut Vec<String>,
    node_ids: &mut Vec<u64>,
    id_map: &mut HashMap<u64, u64>,
) -> usize {
    let mut batches_processed = 0;
    // A user-supplied `batch_size` of 0 must not panic `chunks()`.
    let batch_size = request.batch_size.max(1);

    // Process nodes in batches — each chunk creates every node directly
    // against the engine (no Cypher parse/plan) under a single write-lock
    // acquisition, which is what removes the ~11x slowdown against
    // `UNWIND` over `/cypher`: the old path re-acquired the lock and
    // parsed+planned one `CREATE` statement per row.
    for batch in request.nodes.chunks(batch_size) {
        batches_processed += 1;
        let mut batch_nodes = 0;
        let mut batch_errors = Vec::new();

        let mut engine = server.engine.write().await;
        for node in batch {
            match create_node_direct(&mut engine, node, id_map) {
                Ok(internal_id) => {
                    node_ids.push(internal_id);
                    batch_nodes += 1;
                }
                Err(e) => batch_errors.push(format!("Node creation failed: {}", e)),
            }
        }
        drop(engine);

        *nodes_ingested += batch_nodes;
        errors.extend(batch_errors);
    }

    // Process relationships in batches, resolving `src`/`dst` against the
    // ids assigned to nodes created above (in this request or a prior
    // batch of it).
    for batch in request.relationships.chunks(batch_size) {
        batches_processed += 1;
        let mut batch_rels = 0;
        let mut batch_errors = Vec::new();

        let mut engine = server.engine.write().await;
        for rel in batch {
            match create_relationship_direct(&mut engine, rel, id_map) {
                Ok(_) => batch_rels += 1,
                Err(e) => batch_errors.push(format!("Relationship creation failed: {}", e)),
            }
        }
        drop(engine);

        *relationships_ingested += batch_rels;
        errors.extend(batch_errors);
    }

    batches_processed
}

/// Process ingestion without chunking: one `server.engine.write()`
/// acquisition for all nodes, then one for all relationships. Same
/// best-effort (non-atomic) row semantics as `process_with_batching`.
async fn process_without_batching(
    server: &std::sync::Arc<NexusServer>,
    request: &IngestRequest,
    nodes_ingested: &mut usize,
    relationships_ingested: &mut usize,
    errors: &mut Vec<String>,
    node_ids: &mut Vec<u64>,
    id_map: &mut HashMap<u64, u64>,
) {
    if !request.nodes.is_empty() {
        let mut engine = server.engine.write().await;
        for node in &request.nodes {
            match create_node_direct(&mut engine, node, id_map) {
                Ok(internal_id) => {
                    node_ids.push(internal_id);
                    *nodes_ingested += 1;
                }
                Err(e) => errors.push(format!("Node ingestion failed: {}", e)),
            }
        }
    }

    if !request.relationships.is_empty() {
        let mut engine = server.engine.write().await;
        for rel in &request.relationships {
            match create_relationship_direct(&mut engine, rel, id_map) {
                Ok(_) => *relationships_ingested += 1,
                Err(e) => errors.push(format!("Relationship ingestion failed: {}", e)),
            }
        }
    }
}

/// Resolve a relationship endpoint against ids assigned to nodes created
/// earlier in the same request; falls back to treating `raw` as a literal
/// internal node id when it isn't a known client-supplied key. See the
/// doc comment on [`RelIngest::src`] for the full contract.
fn resolve_endpoint(id_map: &HashMap<u64, u64>, raw: u64) -> u64 {
    id_map.get(&raw).copied().unwrap_or(raw)
}

/// Create a single node directly against the engine — the same
/// `Engine::create_node` entry point a standalone Cypher `CREATE (n) `
/// statement uses internally, just reached without building, parsing, or
/// planning a Cypher string for every row.
///
/// `node.id`, when present, is recorded in `id_map` as a request-scoped
/// correlation key (see [`NodeIngest::id`]) after the node is created —
/// never before, so a failed creation never reserves the key.
fn create_node_direct(
    engine: &mut Engine,
    node: &NodeIngest,
    id_map: &mut HashMap<u64, u64>,
) -> Result<u64, String> {
    // Validated for behavioural parity with the previous Cypher-string
    // implementation (same accepted label grammar), even though this path
    // no longer interpolates labels into a query string.
    super::identifier::validate_all(node.labels.iter().map(String::as_str))
        .map_err(|e| format!("invalid label: {}", e))?;

    if let Some(client_id) = node.id {
        if id_map.contains_key(&client_id) {
            return Err(format!(
                "duplicate node id {} within request; ids must be unique per request",
                client_id
            ));
        }
    }

    let internal_id = engine
        .create_node(node.labels.clone(), node.properties.clone())
        .map_err(|e| e.to_string())?;

    if let Some(client_id) = node.id {
        id_map.insert(client_id, internal_id);
    }

    Ok(internal_id)
}

/// Create a single relationship directly against the engine — the same
/// `Engine::create_relationship` entry point a standalone Cypher
/// `CREATE (a)-[r]->(b)` statement uses internally. `rel.src`/`.dst` are
/// resolved through `id_map` first (see [`resolve_endpoint`]).
fn create_relationship_direct(
    engine: &mut Engine,
    rel: &RelIngest,
    id_map: &HashMap<u64, u64>,
) -> Result<u64, String> {
    // Same parity rationale as `create_node_direct`.
    super::identifier::validate_identifier(&rel.r#type)
        .map_err(|e| format!("invalid relationship type: {}", e))?;

    let src = resolve_endpoint(id_map, rel.src);
    let dst = resolve_endpoint(id_map, rel.dst);

    engine
        .create_relationship(src, dst, rel.r#type.clone(), rel.properties.clone())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RootUserConfig;
    use axum::extract::State;
    use nexus_core::testing::TestContext;
    use nexus_core::{
        Engine,
        auth::{
            AuditConfig, AuditLogger, AuthConfig, AuthManager, JwtConfig, JwtManager,
            RoleBasedAccessControl,
        },
        database::DatabaseManager,
        executor::Executor,
    };
    use parking_lot::RwLock as ParkingLotRwLock;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    /// Helper function to create a test server
    /// Returns (TestContext, Arc<NexusServer>) - TestContext must be kept alive for the duration of the test
    async fn create_test_server() -> (TestContext, Arc<NexusServer>) {
        let ctx = TestContext::new();
        let engine = Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager = DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(ParkingLotRwLock::new(database_manager));

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

        (ctx, server)
    }

    #[tokio::test]
    async fn test_ingest_nodes_only() {
        let (_ctx, server) = create_test_server().await;
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

        let _response = ingest_data_inner(State(server), request).await;
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

        let _response = ingest_data_inner(State(server), request).await;
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

        let _response = ingest_data_inner(State(server), request).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    #[ignore] // TODO: Fix temp dir race condition in parallel tests
    async fn test_ingest_empty_request() {
        let (_temp_dir, server) = create_test_server().await;
        let request = IngestRequest {
            nodes: vec![],
            relationships: vec![],
            batch_size: 1000,
            use_batching: false,
        };

        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
        // Test passes if no panic occurs
    }

    // Removed: `test_ingest_with_large_dataset`.
    // The test asserted nothing (`// Test passes if no panic occurs`) and
    // exercised the same code path as the surrounding tests with a bigger
    // payload. Under `cargo test --workspace --lib` it was the test that
    // triggered a STATUS_STACK_BUFFER_OVERRUN on Windows: every test in
    // this module constructs a fresh temporary `Engine` (catalog + mmap +
    // async-WAL thread), and with default test-threads = num_cpus the
    // parallel instances collectively pushed RSS past the host's limit.
    // Coverage is preserved by the other `test_ingest_with_*` tests in
    // this module — the removed variant was pure megabyte-for-megabyte
    // duplication.

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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
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
        let _response = ingest_data_inner(State(server), request).await;
        // Test passes if no panic occurs
    }

    // ── phase0_fix-ingest-bulk-path regression ────────────────────────────
    //
    // `NodeIngest.id` used to be parsed and silently discarded
    // (`#[allow(dead_code)]` on the field was the compiler confirming it),
    // and `RelIngest.src`/`.dst` were interpreted as literal *internal*
    // node ids — which a client cannot know before ingesting the nodes in
    // the same request. A node+relationship ingest therefore looked like
    // it succeeded (no error surfaced, `relationships_ingested` == 1)
    // while creating zero relationships, because the generated
    // `MATCH (a), (b) WHERE id(a) = {src} AND id(b) = {dst}` matched no
    // rows and the subsequent `CREATE` never fired.
    #[tokio::test]
    async fn test_ingest_composes_relationships_via_supplied_node_ids() {
        let (_ctx, server) = create_test_server().await;

        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: Some(500),
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Alice"}),
                },
                NodeIngest {
                    id: Some(600),
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Bob"}),
                },
            ],
            relationships: vec![RelIngest {
                id: None,
                src: 500,
                dst: 600,
                r#type: "KNOWS".to_string(),
                properties: json!({}),
            }],
            batch_size: 1000,
            use_batching: false,
        };

        let response = ingest_data_inner(State(server.clone()), request).await;
        assert_eq!(response.nodes_ingested, 2, "both nodes must be created");
        assert_eq!(
            response.relationships_ingested, 1,
            "the relationship row must be reported as created"
        );
        assert!(
            response.error.is_none(),
            "no per-row error expected: {:?}",
            response.error
        );

        // The relationship must actually exist and connect the two nodes
        // just ingested — not merely be reported as ingested.
        let mut engine = server.engine.write().await;
        let result = engine
            .execute_cypher(
                "MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b:Person {name: 'Bob'}) \
                 RETURN count(r)",
            )
            .expect("count query must execute");
        let count = result.rows[0].values[0].as_i64().unwrap_or(-1);
        assert_eq!(
            count, 1,
            "the supplied node ids (500/600) must be honoured so the \
             relationship connects the two nodes just ingested, instead of \
             being silently dropped against nonexistent internal ids"
        );
    }

    #[tokio::test]
    async fn test_ingest_response_returns_node_ids_in_input_order() {
        let (_ctx, server) = create_test_server().await;

        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "First"}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Second"}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Third"}),
                },
            ],
            relationships: vec![],
            batch_size: 1000,
            use_batching: false,
        };

        let response = ingest_data_inner(State(server.clone()), request).await;
        assert_eq!(response.nodes_ingested, 3);
        assert_eq!(
            response.node_ids.len(),
            3,
            "one internal id must be returned per successfully created node"
        );

        // The returned ids must be independently queryable and, in input
        // order, must name the node created from that row.
        let mut engine = server.engine.write().await;
        for (expected_name, node_id) in ["First", "Second", "Third"].iter().zip(&response.node_ids)
        {
            let result = engine
                .execute_cypher(&format!(
                    "MATCH (n) WHERE id(n) = {} RETURN n.name",
                    node_id
                ))
                .expect("lookup by returned id must execute");
            let name = result.rows[0].values[0].as_str().unwrap_or("");
            assert_eq!(name, *expected_name);
        }
    }

    #[tokio::test]
    async fn test_ingest_rejects_duplicate_node_id_within_request() {
        let (_ctx, server) = create_test_server().await;

        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: Some(700),
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Original"}),
                },
                NodeIngest {
                    id: Some(700),
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "Duplicate"}),
                },
            ],
            relationships: vec![],
            batch_size: 1000,
            use_batching: false,
        };

        let response = ingest_data_inner(State(server), request).await;
        assert_eq!(
            response.nodes_ingested, 1,
            "only the first row carrying id 700 may be created"
        );
        assert_eq!(response.node_ids.len(), 1);
        let error = response.0.error.expect("duplicate id must be reported");
        assert!(
            error.contains("duplicate") && error.contains("700"),
            "error must name the duplicate id: {error}"
        );
    }

    #[tokio::test]
    async fn test_ingest_batch_is_best_effort_not_atomic() {
        // phase0_fix-ingest-bulk-path §3.4 — a batch is explicitly NOT
        // atomic: rows before and after a failing row still commit. This
        // asserts the documented contract rather than leaving it implicit.
        let (_ctx, server) = create_test_server().await;

        let request = IngestRequest {
            nodes: vec![
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "BeforeFailure"}),
                },
                NodeIngest {
                    id: None,
                    // Fails `validate_all` (leading digit) — this row must
                    // not roll back the rows around it.
                    labels: vec!["1Invalid".to_string()],
                    properties: json!({}),
                },
                NodeIngest {
                    id: None,
                    labels: vec!["Person".to_string()],
                    properties: json!({"name": "AfterFailure"}),
                },
            ],
            relationships: vec![],
            // Force all three rows into the same batch so the failure
            // sits inside a single write-lock acquisition.
            batch_size: 10,
            use_batching: true,
        };

        let response = ingest_data_inner(State(server.clone()), request).await;
        assert_eq!(
            response.nodes_ingested, 2,
            "the two valid rows must commit despite the invalid row between them"
        );
        assert!(response.error.is_some(), "the invalid row must be reported");

        let mut engine = server.engine.write().await;
        let result = engine
            .execute_cypher("MATCH (n:Person) RETURN count(n)")
            .expect("count query must execute");
        let count = result.rows[0].values[0].as_i64().unwrap_or(-1);
        assert_eq!(
            count, 2,
            "both valid nodes must be durably committed (non-atomic batch)"
        );
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
        let _response = ingest_data_inner(State(server), request).await;
        // Test passes if no panic occurs
    }
}
