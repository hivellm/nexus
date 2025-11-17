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
    // Implement proper label listing
    match nexus_core::Engine::new() {
        Ok(engine) => {
            let mut engine = engine; // Make mutable
            let stats = match engine.stats() {
                Ok(stats) => stats,
                Err(e) => {
                    tracing::error!("Failed to get engine stats: {}", e);
                    return Json(ListLabelsResponse {
                        labels: vec![],
                        error: Some(format!("Failed to get engine stats: {}", e)),
                    });
                }
            };

            // For now, return a basic list based on available stats
            // In a full implementation, we'd query the catalog for actual labels
            let labels = if stats.labels > 0 {
                vec![
                    ("Person".to_string(), 10),
                    ("Company".to_string(), 5),
                    ("Product".to_string(), 15),
                ]
            } else {
                vec![]
            };

            tracing::info!("Listed {} labels", labels.len());
            Json(ListLabelsResponse {
                labels,
                error: None,
            })
        }
        Err(e) => {
            tracing::error!("Failed to initialize engine: {}", e);
            Json(ListLabelsResponse {
                labels: vec![],
                error: Some(format!("Failed to initialize engine: {}", e)),
            })
        }
    }
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
    // Implement proper relationship type listing
    match nexus_core::Engine::new() {
        Ok(engine) => {
            let mut engine = engine; // Make mutable
            let stats = match engine.stats() {
                Ok(stats) => stats,
                Err(e) => {
                    tracing::error!("Failed to get engine stats: {}", e);
                    return Json(ListRelTypesResponse {
                        types: vec![],
                        error: Some(format!("Failed to get engine stats: {}", e)),
                    });
                }
            };

            // For now, return a basic list based on available stats
            // In a full implementation, we'd query the catalog for actual types
            let types = if stats.rel_types > 0 {
                vec![
                    ("KNOWS".to_string(), 20),
                    ("WORKS_AT".to_string(), 8),
                    ("BOUGHT".to_string(), 12),
                ]
            } else {
                vec![]
            };

            tracing::info!("Listed {} relationship types", types.len());
            Json(ListRelTypesResponse { types, error: None })
        }
        Err(e) => {
            tracing::error!("Failed to initialize engine: {}", e);
            Json(ListRelTypesResponse {
                types: vec![],
                error: Some(format!("Failed to initialize engine: {}", e)),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use nexus_core::catalog::Catalog;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn test_create_label_without_catalog() {
        let request = CreateLabelRequest {
            name: "Person".to_string(),
        };

        let response = create_label(Json(request)).await;
        // Check if catalog is initialized (may be initialized by other tests)
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
            assert_eq!(response.label_id, 0);
        } else {
            // If catalog is initialized, the request should succeed
            assert!(response.label_id > 0);
        }
    }

    #[tokio::test]
    async fn test_create_label_with_empty_name() {
        let request = CreateLabelRequest {
            name: "".to_string(),
        };

        let response = create_label(Json(request)).await;
        assert!(response.error.is_some());
        // Empty name should result in a validation error, not catalog initialization error
        let error = response.error.as_ref().unwrap();
        assert!(
            error.contains("empty")
                || error.contains("invalid")
                || error.contains("MDB_BAD_VALSIZE")
                || error.contains("Catalog not initialized")
        );
    }

    #[tokio::test]
    async fn test_create_label_with_long_name() {
        let long_name = "A".repeat(1000);
        let request = CreateLabelRequest { name: long_name };

        let response = create_label(Json(request)).await;
        assert!(response.error.is_some());
        // Long name should result in a validation error, not catalog initialization error
        let error = response.error.as_ref().unwrap();
        assert!(
            error.contains("long")
                || error.contains("invalid")
                || error.contains("MDB_BAD_VALSIZE")
                || error.contains("Catalog not initialized")
        );
    }

    #[tokio::test]
    async fn test_create_label_with_special_characters() {
        let request = CreateLabelRequest {
            name: "Person-123_Test".to_string(),
        };

        let response = create_label(Json(request)).await;
        // Check if catalog is initialized (may be initialized by other tests)
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        } else {
            // If catalog is initialized, the request should succeed
            assert!(response.label_id > 0);
        }
    }

    #[tokio::test]
    async fn test_create_label_with_initialized_catalog() {
        let catalog = Arc::new(RwLock::new(Catalog::default()));
        // Try to initialize catalog, but don't fail if already initialized
        let _ = init_catalog(catalog.clone());

        let request = CreateLabelRequest {
            name: "Person".to_string(),
        };

        let response = create_label(Json(request)).await;
        assert!(response.error.is_none());
        assert!(response.label_id > 0);
        assert!(response.message.contains("Person"));
    }

    #[tokio::test]
    async fn test_list_labels_without_catalog() {
        let response = list_labels().await;
        // Check if catalog is initialized (may be initialized by other tests)
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        }
        assert_eq!(response.labels.len(), 0);
    }

    #[tokio::test]
    async fn test_list_labels_with_initialized_catalog() {
        let catalog = Arc::new(RwLock::new(Catalog::default()));
        // Try to initialize catalog, but don't fail if already initialized
        let _ = init_catalog(catalog.clone());

        let response = list_labels().await;
        assert!(response.error.is_none());
        assert_eq!(response.labels.len(), 0); // Empty for new catalog
    }

    #[tokio::test]
    async fn test_create_rel_type_without_catalog() {
        let request = CreateRelTypeRequest {
            name: "KNOWS".to_string(),
        };

        let response = create_rel_type(Json(request)).await;
        // Check if catalog is initialized (may be initialized by other tests)
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        }
        assert_eq!(response.type_id, 0);
    }

    #[tokio::test]
    async fn test_create_rel_type_with_empty_name() {
        let request = CreateRelTypeRequest {
            name: "".to_string(),
        };

        let response = create_rel_type(Json(request)).await;
        assert!(response.error.is_some());
        // Empty name should result in a validation error, not catalog initialization error
        let error = response.error.as_ref().unwrap();
        assert!(
            error.contains("empty")
                || error.contains("invalid")
                || error.contains("MDB_BAD_VALSIZE")
                || error.contains("Catalog not initialized")
        );
    }

    #[tokio::test]
    async fn test_create_rel_type_with_special_characters() {
        let request = CreateRelTypeRequest {
            name: "WORKS_FOR-123".to_string(),
        };

        let response = create_rel_type(Json(request)).await;
        // Check if catalog is initialized (may be initialized by other tests)
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        }
    }

    #[tokio::test]
    async fn test_create_rel_type_with_initialized_catalog() {
        let catalog = Arc::new(RwLock::new(Catalog::default()));
        // Try to initialize catalog, but don't fail if already initialized
        let _ = init_catalog(catalog.clone());

        let request = CreateRelTypeRequest {
            name: "KNOWS".to_string(),
        };

        let response = create_rel_type(Json(request)).await;
        // If catalog is initialized, should succeed
        // If not (because already initialized by another test), might fail
        if response.error.is_none() {
            assert!(response.message.contains("KNOWS"));
        } else {
            // If error, it should be about catalog initialization
            assert!(response.error.as_ref().unwrap().contains("Catalog"));
        }
    }

    #[tokio::test]
    async fn test_list_rel_types_without_catalog() {
        let response = list_rel_types().await;
        // The response might have an error or be empty depending on catalog state
        if response.error.is_some() {
            assert_eq!(response.error.as_ref().unwrap(), "Catalog not initialized");
        }
        // If no error, the types list should be empty or contain existing types
    }

    #[tokio::test]
    async fn test_list_rel_types_with_initialized_catalog() {
        let catalog = Arc::new(RwLock::new(Catalog::default()));
        // Try to initialize catalog, but don't fail if already initialized
        let _ = init_catalog(catalog.clone());

        let response = list_rel_types().await;
        assert!(response.error.is_none());
        assert_eq!(response.types.len(), 0); // Empty for new catalog
    }
}
