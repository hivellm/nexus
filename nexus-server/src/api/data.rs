//! Data management endpoints

use axum::extract::Json;
use nexus_core::catalog::Catalog;
use nexus_core::executor::Executor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Global catalog instance
static CATALOG: std::sync::OnceLock<Arc<RwLock<Catalog>>> = std::sync::OnceLock::new();

/// Global executor instance
static EXECUTOR: std::sync::OnceLock<Arc<RwLock<Executor>>> = std::sync::OnceLock::new();

/// Initialize the executor
pub fn init_executor(executor: Arc<RwLock<Executor>>) -> anyhow::Result<()> {
    EXECUTOR
        .set(executor)
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(())
}

/// Get the executor instance
fn get_executor() -> Arc<RwLock<Executor>> {
    EXECUTOR.get().expect("Executor not initialized").clone()
}

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

/// Validate node ID
fn validate_node_id(node_id: u64) -> Result<(), String> {
    if node_id == 0 {
        return Err("Node ID cannot be 0".to_string());
    }
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

/// Initialize the catalog
pub fn init_catalog(catalog: Arc<RwLock<Catalog>>) -> anyhow::Result<()> {
    CATALOG
        .set(catalog)
        .map_err(|_| anyhow::anyhow!("Failed to set catalog"))?;
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
pub async fn create_node(Json(request): Json<CreateNodeRequest>) -> Json<CreateNodeResponse> {
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

    let _catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(CreateNodeResponse {
                node_id: 0,
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    // Implement actual node creation using Engine API (CREATE not supported in Cypher parser)
    let start_time = Instant::now();
    log_operation("create_node", &format!("Labels: {:?}", request.labels));

    // Use the shared Engine from lib.rs
    use nexus_core::Engine;
    let catalog_arc = match CATALOG.get() {
        Some(c) => c.clone(),
        None => {
            return Json(CreateNodeResponse {
                node_id: 0,
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };
    let catalog = catalog_arc.clone();

    // Create a temporary Engine to use its create_node method
    let record_store = match nexus_core::storage::RecordStore::new("./data") {
        Ok(s) => s,
        Err(e) => {
            log_error(
                "create_node",
                "Failed to create record store",
                &e.to_string(),
            );
            return Json(CreateNodeResponse {
                node_id: 0,
                message: "".to_string(),
                error: Some(format!("Failed to create record store: {}", e)),
            });
        }
    };

    // This approach creates a NEW Engine every time which won't persist data
    // We need to use the shared Engine from the server state
    // For now, return an error indicating this needs to be refactored
    log_error("create_node", "Cannot create Engine in handler", "");
    Json(CreateNodeResponse {
        node_id: 0,
        message: "".to_string(),
        error: Some("Node creation requires shared Engine instance. This handler needs refactoring to use the Engine from NexusServer state".to_string()),
    })
}

/// Create a new relationship
pub async fn create_rel(Json(request): Json<CreateRelRequest>) -> Json<CreateRelResponse> {
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

    let _catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(CreateRelResponse {
                rel_id: 0,
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    // Implement actual relationship creation
    // TODO: Refactor to use shared executor like create_node
    Json(CreateRelResponse {
        rel_id: 0,
        message: "Not yet implemented with shared executor".to_string(),
        error: Some("Use Cypher query with executor".to_string()),
    })
    /*
    match create_engine() {
        Ok(mut engine) => {
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
        Err(e) => {
            tracing::error!("Failed to initialize engine: {}", e);
            Json(CreateRelResponse {
                rel_id: 0,
                message: "".to_string(),
                error: Some(format!("Failed to initialize engine: {}", e)),
            })
        }
    }
    */
}

/// Update a node
pub async fn update_node(Json(request): Json<UpdateNodeRequest>) -> Json<UpdateNodeResponse> {
    tracing::info!("Updating node: {}", request.node_id);

    // Validate input
    if let Err(validation_error) = validate_node_id(request.node_id) {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(UpdateNodeResponse {
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    let _catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(UpdateNodeResponse {
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    // Implement actual node update
    // TODO: Refactor to use shared executor
    Json(UpdateNodeResponse {
        message: "Not yet implemented with shared executor".to_string(),
        error: Some("Use Cypher query with executor".to_string()),
    })
    /*
    match create_engine() {
        Ok(mut engine) => {
            match engine.update_node(
                request.node_id,
                vec!["Updated".to_string()], // TODO: Allow updating labels
                serde_json::Value::Object(request.properties.into_iter().collect()),
            ) {
                Ok(_) => {
                    tracing::info!("Node {} updated successfully", request.node_id);
                    Json(UpdateNodeResponse {
                        message: "Node updated successfully".to_string(),
                        error: None,
                    })
                }
                Err(e) => {
                    tracing::error!("Failed to update node: {}", e);
                    Json(UpdateNodeResponse {
                        message: "".to_string(),
                        error: Some(format!("Failed to update node: {}", e)),
                    })
                }
            }
        }
        Err(e) => {
            tracing::error!("Failed to initialize engine: {}", e);
            Json(UpdateNodeResponse {
                message: "".to_string(),
                error: Some(format!("Failed to initialize engine: {}", e)),
            })
        }
    }
    */
}

/// Delete a node
pub async fn delete_node(Json(request): Json<DeleteNodeRequest>) -> Json<DeleteNodeResponse> {
    tracing::info!("Deleting node: {}", request.node_id);

    // Validate input
    if let Err(validation_error) = validate_node_id(request.node_id) {
        tracing::error!("Validation failed: {}", validation_error);
        return Json(DeleteNodeResponse {
            message: "".to_string(),
            error: Some(format!("Validation failed: {}", validation_error)),
        });
    }

    let _catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(DeleteNodeResponse {
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    // Implement actual node deletion
    // TODO: Refactor to use shared executor
    Json(DeleteNodeResponse {
        message: "Not yet implemented with shared executor".to_string(),
        error: Some("Use Cypher query with executor".to_string()),
    })
    /*
    match create_engine() {
        Ok(mut engine) => match engine.delete_node(request.node_id) {
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
                tracing::error!("Failed to delete node: {}", e);
                Json(DeleteNodeResponse {
                    message: "".to_string(),
                    error: Some(format!("Failed to delete node: {}", e)),
                })
            }
        },
        Err(e) => {
            tracing::error!("Failed to initialize engine: {}", e);
            Json(DeleteNodeResponse {
                message: "".to_string(),
                error: Some(format!("Failed to initialize engine: {}", e)),
            })
        }
    }
    */
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
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Json<GetNodeResponse> {
    let node_id = params
        .get("id")
        .or_else(|| params.get("node_id"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

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

    // Check if catalog is initialized
    if CATALOG.get().is_none() {
        tracing::error!("Catalog not initialized");
        return Json(GetNodeResponse {
            message: "".to_string(),
            node: None,
            error: Some("Catalog not initialized".to_string()),
        });
    }

    // Implement actual node retrieval using Cypher
    let executor = get_executor();
    let mut executor_guard = executor.write().await;

    // Use Cypher query to get node by ID
    let query = format!(
        "MATCH (n) WHERE id(n) = {} RETURN n, labels(n) as node_labels, properties(n) as node_props",
        node_id
    );
    let cypher_query = nexus_core::executor::Query {
        cypher: query,
        params: HashMap::new(),
    };

    match executor_guard.execute(&cypher_query) {
        Ok(result_set) => {
            if result_set.rows.is_empty() {
                tracing::warn!("Node {} not found", node_id);
                return Json(GetNodeResponse {
                    message: "Node not found".to_string(),
                    node: None,
                    error: Some("Node not found".to_string()),
                });
            }

            // For now, return a simple response
            // TODO: Parse the result properly when Cypher result parsing is implemented
            tracing::info!("Node {} retrieved successfully", node_id);
            Json(GetNodeResponse {
                message: "Node retrieved successfully".to_string(),
                node: Some(NodeData {
                    id: node_id,
                    labels: vec![],
                    properties: serde_json::Value::Object(serde_json::Map::new()),
                }),
                error: None,
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

    #[tokio::test]
    async fn test_create_node_without_catalog() {
        let request = CreateNodeRequest {
            labels: vec!["Person".to_string()],
            properties: HashMap::new(),
        };

        let response = create_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        assert_eq!(response.node_id, 0);
    }

    #[tokio::test]
    async fn test_create_node_with_empty_labels() {
        let request = CreateNodeRequest {
            labels: vec![],
            properties: HashMap::new(),
        };

        let response = create_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(
            response.error.as_ref().unwrap(),
            "Validation failed: At least one label is required"
        );
    }

    #[tokio::test]
    async fn test_create_node_with_multiple_labels() {
        let request = CreateNodeRequest {
            labels: vec!["Person".to_string(), "Developer".to_string()],
            properties: HashMap::new(),
        };

        let response = create_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_create_node_with_properties() {
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), json!("Alice"));
        properties.insert("age".to_string(), json!(30));

        let request = CreateNodeRequest {
            labels: vec!["Person".to_string()],
            properties,
        };

        let response = create_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_create_rel_without_catalog() {
        let request = CreateRelRequest {
            source_id: 1,
            target_id: 2,
            rel_type: "KNOWS".to_string(),
            properties: HashMap::new(),
        };

        let response = create_rel(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        assert_eq!(response.rel_id, 0);
    }

    #[tokio::test]
    async fn test_create_rel_with_properties() {
        let mut properties = HashMap::new();
        properties.insert("since".to_string(), json!(2020));

        let request = CreateRelRequest {
            source_id: 1,
            target_id: 2,
            rel_type: "KNOWS".to_string(),
            properties,
        };

        let response = create_rel(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_create_rel_with_empty_type() {
        let request = CreateRelRequest {
            source_id: 1,
            target_id: 2,
            rel_type: "".to_string(),
            properties: HashMap::new(),
        };

        let response = create_rel(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(
            response.error.as_ref().unwrap(),
            "Validation failed: Relationship type cannot be empty"
        );
    }

    #[tokio::test]
    async fn test_update_node_without_catalog() {
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), json!("Bob"));

        let request = UpdateNodeRequest {
            node_id: 1,
            properties,
        };

        let response = update_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_update_node_with_empty_properties() {
        let request = UpdateNodeRequest {
            node_id: 1,
            properties: HashMap::new(),
        };

        let response = update_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_update_node_with_zero_id() {
        let mut properties = HashMap::new();
        properties.insert("name".to_string(), json!("Alice"));

        let request = UpdateNodeRequest {
            node_id: 0,
            properties,
        };

        let response = update_node(Json(request)).await;
        assert!(response.error.is_some());
        assert!(
            response
                .error
                .as_ref()
                .unwrap()
                .contains("Validation failed")
        );
        assert!(
            response
                .error
                .as_ref()
                .unwrap()
                .contains("Node ID cannot be 0")
        );
    }

    #[tokio::test]
    async fn test_delete_node_without_catalog() {
        let request = DeleteNodeRequest { node_id: 1 };

        let response = delete_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }

    #[tokio::test]
    async fn test_delete_node_with_zero_id() {
        let request = DeleteNodeRequest { node_id: 0 };

        let response = delete_node(Json(request)).await;
        assert!(response.error.is_some());
        assert!(
            response
                .error
                .as_ref()
                .unwrap()
                .contains("Validation failed")
        );
        assert!(
            response
                .error
                .as_ref()
                .unwrap()
                .contains("Node ID cannot be 0")
        );
    }

    #[tokio::test]
    async fn test_delete_node_with_large_id() {
        let request = DeleteNodeRequest { node_id: u64::MAX };

        let response = delete_node(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
    }
}
