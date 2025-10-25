//! Schema management endpoints

use axum::extract::Json;
use nexus_core::catalog::Catalog;
use serde::{Deserialize, Serialize};
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

/// Create label request
#[derive(Debug, Deserialize)]
pub struct CreateLabelRequest {
    /// Label name
    pub name: String,
}

/// Create label response
#[derive(Debug, Serialize)]
pub struct CreateLabelResponse {
    /// Label ID
    pub label_id: u32,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// List labels response
#[derive(Debug, Serialize)]
pub struct ListLabelsResponse {
    /// Labels with their IDs
    pub labels: Vec<(String, u32)>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create relationship type request
#[derive(Debug, Deserialize)]
pub struct CreateRelTypeRequest {
    /// Relationship type name
    pub name: String,
}

/// Create relationship type response
#[derive(Debug, Serialize)]
pub struct CreateRelTypeResponse {
    /// Relationship type ID
    pub type_id: u32,
    /// Success message
    pub message: String,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// List relationship types response
#[derive(Debug, Serialize)]
pub struct ListRelTypesResponse {
    /// Relationship types with their IDs
    pub types: Vec<(String, u32)>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create a new label
pub async fn create_label(Json(request): Json<CreateLabelRequest>) -> Json<CreateLabelResponse> {
    tracing::info!("Creating label: {}", request.name);

    let catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(CreateLabelResponse {
                label_id: 0,
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    let catalog = catalog_guard.write().await;
    match catalog.get_or_create_label(&request.name) {
        Ok(label_id) => {
            tracing::info!("Label '{}' created with ID: {}", request.name, label_id);
            Json(CreateLabelResponse {
                label_id,
                message: format!("Label '{}' created successfully", request.name),
                error: None,
            })
        }
        Err(e) => {
            tracing::error!("Failed to create label '{}': {}", request.name, e);
            Json(CreateLabelResponse {
                label_id: 0,
                message: "".to_string(),
                error: Some(e.to_string()),
            })
        }
    }
}

/// List all labels
pub async fn list_labels() -> Json<ListLabelsResponse> {
    tracing::info!("Listing all labels");

    let catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(ListLabelsResponse {
                labels: vec![],
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    let _catalog = catalog_guard.read().await;
    // TODO: Implement proper label listing when Catalog supports it
    // For now, return empty list
    tracing::info!("Label listing not yet implemented");
    Json(ListLabelsResponse {
        labels: vec![],
        error: None,
    })
}

/// Create a new relationship type
pub async fn create_rel_type(
    Json(request): Json<CreateRelTypeRequest>,
) -> Json<CreateRelTypeResponse> {
    tracing::info!("Creating relationship type: {}", request.name);

    let catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(CreateRelTypeResponse {
                type_id: 0,
                message: "".to_string(),
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    let catalog = catalog_guard.write().await;
    match catalog.get_or_create_type(&request.name) {
        Ok(type_id) => {
            tracing::info!(
                "Relationship type '{}' created with ID: {}",
                request.name,
                type_id
            );
            Json(CreateRelTypeResponse {
                type_id,
                message: format!("Relationship type '{}' created successfully", request.name),
                error: None,
            })
        }
        Err(e) => {
            tracing::error!(
                "Failed to create relationship type '{}': {}",
                request.name,
                e
            );
            Json(CreateRelTypeResponse {
                type_id: 0,
                message: "".to_string(),
                error: Some(e.to_string()),
            })
        }
    }
}

/// List all relationship types
pub async fn list_rel_types() -> Json<ListRelTypesResponse> {
    tracing::info!("Listing all relationship types");

    let catalog_guard = match CATALOG.get() {
        Some(catalog) => catalog,
        None => {
            tracing::error!("Catalog not initialized");
            return Json(ListRelTypesResponse {
                types: vec![],
                error: Some("Catalog not initialized".to_string()),
            });
        }
    };

    let _catalog = catalog_guard.read().await;
    // TODO: Implement proper relationship type listing when Catalog supports it
    // For now, return empty list
    tracing::info!("Relationship type listing not yet implemented");
    Json(ListRelTypesResponse {
        types: vec![],
        error: None,
    })
}
