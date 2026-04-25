//! Schema management endpoints

use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::NexusServer;

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

/// One entry in the response of `GET /schema/labels`.
///
/// Wire shape (issue #2): `{"name": "Person", "id": 0}`. The previous
/// shape was a JSON tuple `["Person", 0]` which the Rust SDK read as
/// `Vec<(String, u32)>`. Callers in the wild were splitting that
/// tuple as `(name, count)` because the second member is unnamed,
/// when it is actually the catalog id allocated by the engine.
/// Naming the fields makes the contract explicit and leaves room
/// for additive fields (e.g. `count`) without another breaking
/// rename.
#[derive(Debug, Clone, Serialize)]
pub struct LabelInfo {
    /// Label name as registered in the engine catalog.
    pub name: String,
    /// Catalog id allocated to this label.
    pub id: u32,
}

/// List labels response
#[derive(Debug, Serialize)]
pub struct ListLabelsResponse {
    /// Labels registered in the catalog with their allocated ids.
    pub labels: Vec<LabelInfo>,
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

/// One entry in the response of `GET /schema/rel_types`.
///
/// Mirrors `LabelInfo` — same rationale, see issue #2.
#[derive(Debug, Clone, Serialize)]
pub struct RelTypeInfo {
    /// Relationship type name as registered in the catalog.
    pub name: String,
    /// Catalog id allocated to this relationship type.
    pub id: u32,
}

/// List relationship types response
#[derive(Debug, Serialize)]
pub struct ListRelTypesResponse {
    /// Relationship types registered in the catalog with their ids.
    pub types: Vec<RelTypeInfo>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Create a new label. Registers the name in the shared engine's
/// catalog and returns the allocated `LabelId`.
pub async fn create_label(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CreateLabelRequest>,
) -> Json<CreateLabelResponse> {
    tracing::info!("Creating label: {}", request.name);

    let engine = server.engine.read().await;
    match engine.catalog.get_or_create_label(&request.name) {
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
                message: String::new(),
                error: Some(e.to_string()),
            })
        }
    }
}

/// List every label registered in the engine's catalog.
pub async fn list_labels(State(server): State<Arc<NexusServer>>) -> Json<ListLabelsResponse> {
    tracing::info!("Listing all labels");

    let engine = server.engine.read().await;
    let labels: Vec<LabelInfo> = engine
        .catalog
        .list_all_labels()
        .into_iter()
        .map(|(id, name)| LabelInfo { name, id })
        .collect();

    tracing::info!("Listed {} labels", labels.len());
    Json(ListLabelsResponse {
        labels,
        error: None,
    })
}

/// Create a new relationship type.
pub async fn create_rel_type(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CreateRelTypeRequest>,
) -> Json<CreateRelTypeResponse> {
    tracing::info!("Creating relationship type: {}", request.name);

    let engine = server.engine.read().await;
    match engine.catalog.get_or_create_type(&request.name) {
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
                message: String::new(),
                error: Some(e.to_string()),
            })
        }
    }
}

/// List every relationship type registered in the catalog.
pub async fn list_rel_types(State(server): State<Arc<NexusServer>>) -> Json<ListRelTypesResponse> {
    tracing::info!("Listing all relationship types");

    let engine = server.engine.read().await;
    let types: Vec<RelTypeInfo> = engine
        .catalog
        .list_all_types()
        .into_iter()
        .map(|(id, name)| RelTypeInfo { name, id })
        .collect();

    tracing::info!("Listed {} relationship types", types.len());
    Json(ListRelTypesResponse { types, error: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_server() -> Arc<NexusServer> {
        use parking_lot::RwLock as PlRwLock;
        use tokio::sync::RwLock as TokioRwLock;

        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_isolated_catalog(ctx.path()).expect("engine init");
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

    #[tokio::test]
    async fn test_create_and_list_labels_round_trip() {
        let server = build_test_server();

        let out = create_label(
            State(Arc::clone(&server)),
            Json(CreateLabelRequest {
                name: "Person".to_string(),
            }),
        )
        .await
        .0;
        assert!(out.error.is_none(), "create failed: {:?}", out.error);

        let listed = list_labels(State(server)).await.0;
        assert!(listed.error.is_none());
        assert!(
            listed.labels.iter().any(|l| l.name == "Person"),
            "expected 'Person' in listed labels: {:?}",
            listed.labels
        );
    }

    #[tokio::test]
    async fn test_create_and_list_rel_types_round_trip() {
        let server = build_test_server();

        let out = create_rel_type(
            State(Arc::clone(&server)),
            Json(CreateRelTypeRequest {
                name: "KNOWS".to_string(),
            }),
        )
        .await
        .0;
        assert!(out.error.is_none(), "create failed: {:?}", out.error);

        let listed = list_rel_types(State(server)).await.0;
        assert!(listed.error.is_none());
        assert!(
            listed.types.iter().any(|t| t.name == "KNOWS"),
            "expected 'KNOWS' in listed types: {:?}",
            listed.types
        );
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_catalog_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        let _ = create_label(
            State(Arc::clone(&server_a)),
            Json(CreateLabelRequest {
                name: "OnlyOnA".to_string(),
            }),
        )
        .await;

        let listed_b = list_labels(State(server_b)).await.0;
        assert!(
            !listed_b.labels.iter().any(|l| l.name == "OnlyOnA"),
            "server B must not see 'OnlyOnA' registered on server A: {:?}",
            listed_b.labels
        );
    }
}
