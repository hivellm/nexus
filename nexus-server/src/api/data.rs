//! Data management endpoints

use axum::extract::Json;
use nexus_core::catalog::Catalog;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global catalog instance
static CATALOG: std::sync::OnceLock<Arc<RwLock<Catalog>>> = std::sync::OnceLock::new();

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

    // TODO: Implement node creation when Catalog supports it
    // For now, return error
    tracing::info!("Node creation not yet implemented");
    Json(CreateNodeResponse {
        node_id: 0,
        message: "".to_string(),
        error: Some("Node creation not yet implemented in Catalog".to_string()),
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

    // TODO: Implement relationship creation when Catalog supports it
    // For now, return error
    tracing::info!("Relationship creation not yet implemented");
    Json(CreateRelResponse {
        rel_id: 0,
        message: "".to_string(),
        error: Some("Relationship creation not yet implemented in Catalog".to_string()),
    })
}

/// Update a node
pub async fn update_node(Json(request): Json<UpdateNodeRequest>) -> Json<UpdateNodeResponse> {
    tracing::info!("Updating node: {}", request.node_id);

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

    // TODO: Implement node update when Catalog supports it
    // For now, return error
    tracing::info!("Node update not yet implemented");
    Json(UpdateNodeResponse {
        message: "".to_string(),
        error: Some("Node update not yet implemented in Catalog".to_string()),
    })
}

/// Delete a node
pub async fn delete_node(Json(request): Json<DeleteNodeRequest>) -> Json<DeleteNodeResponse> {
    tracing::info!("Deleting node: {}", request.node_id);

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

    // TODO: Implement node deletion when Catalog supports it
    // For now, return error
    tracing::info!("Node deletion not yet implemented");
    Json(DeleteNodeResponse {
        message: "".to_string(),
        error: Some("Node deletion not yet implemented in Catalog".to_string()),
    })
}
