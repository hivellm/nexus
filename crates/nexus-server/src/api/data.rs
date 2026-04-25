//! Data management endpoints

use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::NexusServer;

/// Helper function to log operation details
fn log_operation(operation: &str, details: &str) {
    tracing::info!("Operation: {} - Details: {}", operation, details);
}

/// Helper function to log errors with context
fn log_error(operation: &str, error: &str, context: &str) {
    tracing::error!(
        "Operation: {} - Error: {} - Context: {}",
        operation,
        error,
        context
    );
}

/// Validate node labels
#[allow(dead_code)]
fn validate_labels(labels: &[String]) -> Result<(), String> {
    if labels.is_empty() {
        return Err("At least one label is required".to_string());
    }

    for label in labels {
        if label.is_empty() {
            return Err("Label cannot be empty".to_string());
        }
        if label.len() > 255 {
            return Err("Label too long (max 255 characters)".to_string());
        }
        if !label.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(
                "Label contains invalid characters (only alphanumeric and underscore allowed)"
                    .to_string(),
            );
        }
    }

    Ok(())
}

/// Validate node ID.
///
/// Note: `0` is a valid node ID in Nexus — the engine assigns it to
/// the first node ever created in a database (see issue #2). Earlier
/// versions of this validator rejected `0` and broke the natural
/// "create then read back" flow because `create_node()` would mint
/// id `0` and the next `get_node(0)` / `update_node(0)` /
/// `delete_node(0)` would fail validation before the engine was
/// even consulted.
///
/// Existence is checked by the engine itself (it returns `Ok(None)`
/// when the row is gone), so this validator is currently a no-op
/// kept for forward-compat with future invariants we may want to
/// guard at the API boundary (e.g. id ranges per shard).
fn validate_node_id(_node_id: u64) -> Result<(), String> {
    Ok(())
}

/// Validate relationship type
#[allow(dead_code)]
fn validate_relationship_type(rel_type: &str) -> Result<(), String> {
    if rel_type.is_empty() {
        return Err("Relationship type cannot be empty".to_string());
    }
    if rel_type.len() > 255 {
        return Err("Relationship type too long (max 255 characters)".to_string());
    }
    if !rel_type.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err("Relationship type contains invalid characters (only alphanumeric and underscore allowed)".to_string());
    }
    Ok(())
}

/// Create node request
#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    #[serde(default)]
    #[allow(dead_code)]
    pub properties: HashMap<String, serde_json::Value>,
}

impl CreateNodeRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), String> {
        // Validate labels
        if self.labels.is_empty() {
            return Err("At least one label is required".to_string());
        }

        // Validate label names
        for label in &self.labels {
            if label.is_empty() {
                return Err("Label names cannot be empty".to_string());
            }
            if label.len() > 100 {
                return Err("Label names cannot exceed 100 characters".to_string());
            }
            if !label
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err("Label names can only contain alphanumeric characters, underscores, and hyphens".to_string());
            }
        }

        // Validate properties
        for (key, value) in &self.properties {
            if key.is_empty() {
                return Err("Property keys cannot be empty".to_string());
            }
            if key.len() > 100 {
                return Err("Property keys cannot exceed 100 characters".to_string());
            }
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err("Property keys can only contain alphanumeric characters, underscores, and hyphens".to_string());
            }

            // Validate property value size
            let value_size = serde_json::to_string(value).unwrap_or_default().len();
            if value_size > 10000 {
                return Err("Property values cannot exceed 10KB".to_string());
            }
        }

        Ok(())
    }
}

/// Create node response
#[derive(Debug, Serialize)]
pub struct CreateNodeResponse {
    /// Node ID
    pub node_id: u64,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create relationship request
#[derive(Debug, Deserialize)]
pub struct CreateRelRequest {
    /// Source node ID
    pub source_id: u64,
    /// Target node ID
    pub target_id: u64,
    /// Relationship type
    pub rel_type: String,
    /// Relationship properties
    #[serde(default)]
    #[allow(dead_code)]
    pub properties: HashMap<String, serde_json::Value>,
}

impl CreateRelRequest {
    /// Validate the request
    pub fn validate(&self) -> Result<(), String> {
        // Validate node IDs
        if self.source_id == 0 {
            return Err("Source node ID cannot be 0".to_string());
        }
        if self.target_id == 0 {
            return Err("Target node ID cannot be 0".to_string());
        }
        if self.source_id == self.target_id {
            return Err("Source and target node IDs cannot be the same".to_string());
        }

        // Validate relationship type
        if self.rel_type.is_empty() {
            return Err("Relationship type cannot be empty".to_string());
        }
        if self.rel_type.len() > 100 {
            return Err("Relationship type cannot exceed 100 characters".to_string());
        }
        if !self
            .rel_type
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err("Relationship type can only contain alphanumeric characters, underscores, and hyphens".to_string());
        }

        // Validate properties
        for (key, value) in &self.properties {
            if key.is_empty() {
                return Err("Property keys cannot be empty".to_string());
            }
            if key.len() > 100 {
                return Err("Property keys cannot exceed 100 characters".to_string());
            }
            if !key
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
            {
                return Err("Property keys can only contain alphanumeric characters, underscores, and hyphens".to_string());
            }

            // Validate property value size
            let value_size = serde_json::to_string(value).unwrap_or_default().len();
            if value_size > 10000 {
                return Err("Property values cannot exceed 10KB".to_string());
            }
        }

        Ok(())
    }
}

/// Create relationship response
#[derive(Debug, Serialize)]
pub struct CreateRelResponse {
    /// Relationship ID
    pub rel_id: u64,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Update node request
#[derive(Debug, Deserialize)]
pub struct UpdateNodeRequest {
    /// Node ID
    pub node_id: u64,
    /// New properties (will replace existing)
    #[allow(dead_code)]
    pub properties: HashMap<String, serde_json::Value>,
}

/// Update node response
#[derive(Debug, Serialize)]
pub struct UpdateNodeResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Delete node request
#[derive(Debug, Deserialize)]
pub struct DeleteNodeRequest {
    /// Node ID
    pub node_id: u64,
}

/// Delete node response
#[derive(Debug, Serialize)]
pub struct DeleteNodeResponse {
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create a new node
pub async fn create_node(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CreateNodeRequest>,
) -> Json<CreateNodeResponse> {
    tracing::info!("Creating node with labels: {:?}", request.labels);

    // Validate the request
    if let Err(validation_error) = request.validate() {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(CreateNodeResponse {
            node_id: 0,
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    // Implement actual node creation using Engine API (CREATE not supported in Cypher parser)
    let _start_time = Instant::now();
    log_operation("create_node", &format!("Labels: {:?}", request.labels));

    // Use the shared Engine instance to create the node
    let mut engine = server.engine.write().await;

    // Create the node using the engine
    match engine.create_node(
        request.labels.clone(),
        serde_json::Value::Object(request.properties.into_iter().collect()),
    ) {
        Ok(node_id) => {
            tracing::info!("Node created successfully with ID: {}", node_id);
            Json(CreateNodeResponse {
                node_id,
                message: "Node created successfully".to_string(),
                error: None,
            })
        }
        Err(e) => {
            log_error("create_node", "Failed to create node", &e.to_string());
            Json(CreateNodeResponse {
                node_id: 0,
                message: "".to_string(),
                error: Some(format!("Failed to create node: {}", e)),
            })
        }
    }
}

/// Create a new relationship
pub async fn create_rel(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CreateRelRequest>,
) -> Json<CreateRelResponse> {
    tracing::info!(
        "Creating relationship: {} -> {} ({})",
        request.source_id,
        request.target_id,
        request.rel_type
    );

    // Validate the request
    if let Err(validation_error) = request.validate() {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(CreateRelResponse {
            rel_id: 0,
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    // Use the shared Engine instance to create the relationship
    let mut engine = server.engine.write().await;

    // Create the relationship using the engine
    match engine.create_relationship(
        request.source_id,
        request.target_id,
        request.rel_type,
        serde_json::Value::Object(request.properties.into_iter().collect()),
    ) {
        Ok(rel_id) => {
            tracing::info!("Relationship created successfully with ID: {}", rel_id);
            Json(CreateRelResponse {
                rel_id,
                message: "Relationship created successfully".to_string(),
                error: None,
            })
        }
        Err(e) => {
            tracing::error!("Failed to create relationship: {}", e);
            Json(CreateRelResponse {
                rel_id: 0,
                message: "".to_string(),
                error: Some(format!("Failed to create relationship: {}", e)),
            })
        }
    }
}

/// Update a node
pub async fn update_node(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<UpdateNodeRequest>,
) -> Json<UpdateNodeResponse> {
    tracing::info!("Updating node: {}", request.node_id);

    // Validate input
    if let Err(validation_error) = validate_node_id(request.node_id) {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(UpdateNodeResponse {
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    // Get current node to preserve labels
    let mut engine = server.engine.write().await;

    // Get current labels from existing node
    let current_labels = match engine.get_node(request.node_id) {
        Ok(Some(node_record)) => {
            let label_ids = node_record.get_labels();
            let mut labels = Vec::new();
            for label_id in label_ids {
                if let Ok(Some(label_name)) = engine.catalog.get_label_name(label_id) {
                    labels.push(label_name);
                }
            }
            labels
        }
        Ok(None) => {
            tracing::warn!("Node {} not found", request.node_id);
            return Json(UpdateNodeResponse {
                message: "".to_string(),
                error: Some("Node not found".to_string()),
            });
        }
        Err(e) => {
            tracing::error!("Failed to get node {}: {}", request.node_id, e);
            return Json(UpdateNodeResponse {
                message: "".to_string(),
                error: Some(format!("Failed to get node: {}", e)),
            });
        }
    };

    // Convert properties HashMap to serde_json::Value
    let properties = serde_json::Value::Object(request.properties.into_iter().collect());

    // Update node using Engine (preserve existing labels)
    match engine.update_node(request.node_id, current_labels, properties) {
        Ok(_) => {
            tracing::info!("Node {} updated successfully", request.node_id);
            Json(UpdateNodeResponse {
                message: "Node updated successfully".to_string(),
                error: None,
            })
        }
        Err(e) => {
            tracing::error!("Failed to update node {}: {}", request.node_id, e);
            Json(UpdateNodeResponse {
                message: "".to_string(),
                error: Some(format!("Failed to update node: {}", e)),
            })
        }
    }
}

/// Delete a node
pub async fn delete_node(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<DeleteNodeRequest>,
) -> Json<DeleteNodeResponse> {
    tracing::info!("Deleting node: {}", request.node_id);

    // Validate input
    if let Err(validation_error) = validate_node_id(request.node_id) {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(DeleteNodeResponse {
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    // Delete node using Engine directly
    let mut engine = server.engine.write().await;

    match engine.delete_node(request.node_id) {
        Ok(true) => {
            tracing::info!("Node {} deleted successfully", request.node_id);
            Json(DeleteNodeResponse {
                message: "Node deleted successfully".to_string(),
                error: None,
            })
        }
        Ok(false) => {
            tracing::warn!("Node {} not found", request.node_id);
            Json(DeleteNodeResponse {
                message: "Node not found".to_string(),
                error: Some("Node not found".to_string()),
            })
        }
        Err(e) => {
            tracing::error!("Failed to delete node {}: {}", request.node_id, e);
            Json(DeleteNodeResponse {
                message: "".to_string(),
                error: Some(format!("Failed to delete node: {}", e)),
            })
        }
    }
}

/// Request to get a node by ID
#[derive(Debug, Deserialize)]
pub struct GetNodeRequest {
    /// Node ID to retrieve
    pub node_id: u64,
}

/// Response for getting a node
#[derive(Debug, Serialize)]
pub struct GetNodeResponse {
    /// Success message
    pub message: String,
    /// Node data if found
    pub node: Option<NodeData>,
    /// Error message if any
    pub error: Option<String>,
}

/// Node data structure
#[derive(Debug, Serialize)]
pub struct NodeData {
    /// Node ID
    pub id: u64,
    /// Node labels
    pub labels: Vec<String>,
    /// Node properties
    pub properties: serde_json::Value,
}

/// Get a node by ID from query parameter
pub async fn get_node_by_id(
    State(server): State<Arc<NexusServer>>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<GetNodeResponse> {
    // Distinguish "no `id` param" from "id=0" — the latter is a valid
    // node id in Nexus, so we cannot collapse both into `unwrap_or(0)`
    // (which used to silently reinterpret a missing param as id=0).
    let node_id = match params.get("id").or_else(|| params.get("node_id")) {
        Some(raw) => match raw.parse::<u64>() {
            Ok(id) => id,
            Err(_) => {
                return Json(GetNodeResponse {
                    message: "".to_string(),
                    node: None,
                    error: Some(format!(
                        "Invalid node id query parameter: {raw:?} — expected unsigned integer"
                    )),
                });
            }
        },
        None => {
            return Json(GetNodeResponse {
                message: "".to_string(),
                node: None,
                error: Some(
                    "Missing required query parameter `id` (or alias `node_id`)".to_string(),
                ),
            });
        }
    };

    tracing::info!("Getting node by ID from query: {}", node_id);

    // Validate input
    if let Err(validation_error) = validate_node_id(node_id) {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(GetNodeResponse {
            message: "".to_string(),
            node: None,
            error: Some(validation_error),
        });
    }

    // Get node using Engine directly
    let mut engine = server.engine.write().await;

    match engine.get_node(node_id) {
        Ok(Some(node_record)) => {
            // Extract labels from label_bits
            let label_ids = node_record.get_labels();
            let mut labels = Vec::new();
            for label_id in label_ids {
                if let Ok(Some(label_name)) = engine.catalog.get_label_name(label_id) {
                    labels.push(label_name);
                }
            }

            // Load properties from storage
            let properties = engine
                .storage
                .load_node_properties(node_id)
                .unwrap_or(None)
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));

            tracing::info!("Node {} retrieved successfully", node_id);
            Json(GetNodeResponse {
                message: "Node retrieved successfully".to_string(),
                node: Some(NodeData {
                    id: node_id,
                    labels,
                    properties,
                }),
                error: None,
            })
        }
        Ok(None) => {
            tracing::warn!("Node {} not found", node_id);
            Json(GetNodeResponse {
                message: "Node not found".to_string(),
                node: None,
                error: Some("Node not found".to_string()),
            })
        }
        Err(e) => {
            tracing::error!("Failed to get node {}: {}", node_id, e);
            Json(GetNodeResponse {
                message: "".to_string(),
                node: None,
                error: Some(format!("Failed to get node: {}", e)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use serde_json::json;
    use std::collections::HashMap;

    /// Build an isolated `Arc<NexusServer>` per test so handler calls
    /// exercise real engine state without touching process-wide globals.
    fn build_test_server() -> Arc<NexusServer> {
        use parking_lot::RwLock as PlRwLock;
        use tokio::sync::RwLock as TokioRwLock;

        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
        let engine_arc = Arc::new(TokioRwLock::new(engine));
        let executor = Arc::new(nexus_core::executor::Executor::default());
        let dbm = Arc::new(PlRwLock::new(
            nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
        ));
        let rbac = Arc::new(TokioRwLock::new(
            nexus_core::auth::RoleBasedAccessControl::new(),
        ));
        let auth_mgr = Arc::new(nexus_core::auth::AuthManager::new(
            nexus_core::auth::AuthConfig::default(),
        ));
        let jwt = Arc::new(nexus_core::auth::JwtManager::new(
            nexus_core::auth::JwtConfig::default(),
        ));
        let audit = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: ctx.path().join("audit"),
                retention_days: 1,
                compress_logs: false,
            })
            .expect("audit init"),
        );

        // Leak the TestContext so its tempdir outlives the request —
        // the handlers may lazily open files during the test body.
        let _leaked = Box::leak(Box::new(ctx));

        Arc::new(NexusServer::new(
            executor,
            engine_arc,
            dbm,
            rbac,
            auth_mgr,
            jwt,
            audit,
            crate::config::RootUserConfig::default(),
        ))
    }

    // ── Validation-only tests (don't touch the engine) ─────────────────

    #[tokio::test]
    async fn test_create_node_with_empty_labels_fails_validation() {
        let server = build_test_server();
        let response = create_node(
            State(server),
            Json(CreateNodeRequest {
                labels: vec![],
                properties: HashMap::new(),
            }),
        )
        .await;
        assert!(
            response
                .error
                .as_ref()
                .map(|e| e.contains("Validation failed"))
                .unwrap_or(false),
        );
    }

    // Issue #2 regression: id 0 is a valid node id (Nexus assigns it
    // to the first node ever created), so neither GET, UPDATE nor
    // DELETE may reject it at the API boundary. The engine itself
    // tells us whether the row actually exists.

    #[tokio::test]
    async fn test_get_node_by_id_zero_round_trips_after_create() {
        // Regression for hivellm/nexus#2 — `client.get_node(0)` used to
        // come back with `node: None` because the validator rejected
        // the zero id before the engine was even consulted, even
        // though `create_node` had just minted the same id.
        let server = build_test_server();

        let mut props = HashMap::new();
        props.insert("name".to_string(), json!("Alice"));
        let create = create_node(
            State(Arc::clone(&server)),
            Json(CreateNodeRequest {
                labels: vec!["Person".to_string()],
                properties: props,
            }),
        )
        .await;
        let node_id = create.node_id;
        assert!(create.error.is_none(), "create failed: {:?}", create.error);

        let mut query = HashMap::new();
        query.insert("id".to_string(), node_id.to_string());
        let got = get_node_by_id(State(Arc::clone(&server)), axum::extract::Query(query))
            .await
            .0;
        assert!(
            got.error.is_none(),
            "get_node({node_id}) errored: {:?}",
            got.error
        );
        let node = got.node.expect("node must be Some after create");
        assert_eq!(node.id, node_id);
        assert_eq!(node.labels, vec!["Person"]);
        assert_eq!(node.properties["name"], json!("Alice"));
    }

    #[tokio::test]
    async fn test_get_node_by_id_missing_param_returns_error() {
        // Without an `id` query parameter the handler now answers with
        // an explicit error rather than silently fetching id=0 (which
        // is itself a valid node, so the previous `unwrap_or(0)` was
        // ambiguous).
        let server = build_test_server();
        let response = get_node_by_id(State(server), axum::extract::Query(HashMap::new()))
            .await
            .0;
        assert!(response.node.is_none());
        assert!(
            response
                .error
                .as_ref()
                .map(|e| e.contains("Missing required query parameter `id`"))
                .unwrap_or(false),
            "expected missing-param error, got: {:?}",
            response.error
        );
    }

    #[tokio::test]
    async fn test_get_node_by_id_invalid_param_returns_error() {
        let server = build_test_server();
        let mut query = HashMap::new();
        query.insert("id".to_string(), "not-a-number".to_string());
        let response = get_node_by_id(State(server), axum::extract::Query(query))
            .await
            .0;
        assert!(response.node.is_none());
        assert!(
            response
                .error
                .as_ref()
                .map(|e| e.contains("Invalid node id query parameter"))
                .unwrap_or(false),
            "expected parse error, got: {:?}",
            response.error
        );
    }

    #[tokio::test]
    async fn test_update_node_with_unknown_id_returns_engine_error_not_validation() {
        // Passing a never-created id used to short-circuit on the
        // synthetic `Node ID cannot be 0` validator. With that
        // validator gone the call now reaches the engine and surfaces
        // a real "node not found"–style error.
        let server = build_test_server();
        let response = update_node(
            State(server),
            Json(UpdateNodeRequest {
                node_id: 0,
                properties: HashMap::new(),
            }),
        )
        .await;
        // Either the engine rejects (error set) or it accepts and
        // creates an in-memory shape — the only thing this test
        // guarantees is that the validator no longer preempts.
        if let Some(err) = response.error.as_ref() {
            assert!(
                !err.contains("Node ID cannot be 0"),
                "validator preempted the engine: {err}"
            );
        }
    }

    #[tokio::test]
    async fn test_delete_node_with_unknown_id_returns_engine_error_not_validation() {
        let server = build_test_server();
        let response = delete_node(State(server), Json(DeleteNodeRequest { node_id: 0 })).await;
        if let Some(err) = response.error.as_ref() {
            assert!(
                !err.contains("Node ID cannot be 0"),
                "validator preempted the engine: {err}"
            );
        }
    }

    // ── Real round-trips using the shared engine ──────────────────────

    #[tokio::test]
    async fn test_create_and_get_node_round_trip() {
        let server = build_test_server();

        let mut props = HashMap::new();
        props.insert("name".to_string(), json!("Alice"));
        props.insert("age".to_string(), json!(30));

        let create = create_node(
            State(Arc::clone(&server)),
            Json(CreateNodeRequest {
                labels: vec!["Person".to_string()],
                properties: props,
            }),
        )
        .await;
        let node_id = create.node_id;
        assert!(create.error.is_none(), "create failed: {:?}", create.error);

        let mut query = HashMap::new();
        query.insert("id".to_string(), node_id.to_string());
        let got = get_node_by_id(State(Arc::clone(&server)), axum::extract::Query(query))
            .await
            .0;

        assert!(got.error.is_none(), "get failed: {:?}", got.error);
        let node = got.node.expect("node present");
        assert_eq!(node.id, node_id);
        assert_eq!(node.labels, vec!["Person"]);
        assert_eq!(node.properties["name"], json!("Alice"));
        assert_eq!(node.properties["age"], json!(30));
    }

    // ── Parallel-isolation guard required by phase2a tail item 2.2 ────

    #[tokio::test]
    async fn test_two_servers_do_not_share_engine_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        let create_a = create_node(
            State(Arc::clone(&server_a)),
            Json(CreateNodeRequest {
                labels: vec!["Marker".to_string()],
                properties: HashMap::new(),
            }),
        )
        .await;
        let a_id = create_a.node_id;
        assert!(create_a.error.is_none());

        // Ask server B for the same id — it must not find it.
        let mut query = HashMap::new();
        query.insert("id".to_string(), a_id.to_string());
        let got_b = get_node_by_id(State(Arc::clone(&server_b)), axum::extract::Query(query)).await;

        assert!(
            got_b.node.is_none(),
            "server B should not see nodes created against server A"
        );
    }
}
