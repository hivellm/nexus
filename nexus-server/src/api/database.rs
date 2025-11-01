//! Database management API endpoints
//!
//! Provides REST API for managing multiple databases:
//! - POST /management/databases - Create database
//! - DELETE /management/databases/:name - Drop database
//! - GET /management/databases - List all databases
//! - GET /management/databases/:name - Get database info

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use nexus_core::database::{DatabaseInfo, DatabaseManager};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Server state with database manager
#[derive(Clone)]
pub struct DatabaseState {
    /// Database manager
    pub manager: Arc<RwLock<DatabaseManager>>,
}

/// Request to create a new database
#[derive(Debug, Deserialize)]
pub struct CreateDatabaseRequest {
    /// Database name
    pub name: String,
}

/// Response for database creation
#[derive(Debug, Serialize)]
pub struct CreateDatabaseResponse {
    /// Success flag
    pub success: bool,
    /// Database name
    pub name: String,
    /// Message
    pub message: String,
}

/// Response for database list
#[derive(Debug, Serialize)]
pub struct ListDatabasesResponse {
    /// List of databases
    pub databases: Vec<DatabaseInfo>,
    /// Default database name
    pub default_database: String,
}

/// Response for database operations
#[derive(Debug, Serialize)]
pub struct DatabaseResponse {
    /// Success flag
    pub success: bool,
    /// Message
    pub message: String,
}

/// Create a new database
pub async fn create_database(
    State(state): State<DatabaseState>,
    Json(req): Json<CreateDatabaseRequest>,
) -> Response {
    let manager = state.manager.read().await;

    match manager.create_database(&req.name) {
        Ok(_) => Json(CreateDatabaseResponse {
            success: true,
            name: req.name.clone(),
            message: format!("Database '{}' created successfully", req.name),
        })
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(DatabaseResponse {
                success: false,
                message: format!("Failed to create database: {}", e),
            }),
        )
            .into_response(),
    }
}

/// Drop a database
pub async fn drop_database(
    State(state): State<DatabaseState>,
    Path(name): Path<String>,
) -> Response {
    let manager = state.manager.read().await;

    match manager.drop_database(&name) {
        Ok(_) => Json(DatabaseResponse {
            success: true,
            message: format!("Database '{}' dropped successfully", name),
        })
        .into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(DatabaseResponse {
                success: false,
                message: format!("Failed to drop database: {}", e),
            }),
        )
            .into_response(),
    }
}

/// List all databases
pub async fn list_databases(State(state): State<DatabaseState>) -> Response {
    let manager = state.manager.read().await;
    let databases = manager.list_databases();
    let default_database = manager.default_database_name().to_string();

    Json(ListDatabasesResponse {
        databases,
        default_database,
    })
    .into_response()
}

/// Get database info
pub async fn get_database(
    State(state): State<DatabaseState>,
    Path(name): Path<String>,
) -> Response {
    let manager = state.manager.read().await;

    match manager.get_database(&name) {
        Ok(engine) => {
            let engine_guard = engine.read();
            let (node_count, relationship_count) = match engine_guard.stats() {
                Ok(stats) => (stats.nodes, stats.relationships),
                Err(_) => (0, 0),
            };

            Json(DatabaseInfo {
                name: name.clone(),
                path: std::path::PathBuf::new(), // Don't expose full path
                created_at: 0,
                node_count,
                relationship_count,
                storage_size: 0,
            })
            .into_response()
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(DatabaseResponse {
                success: false,
                message: format!("Database not found: {}", e),
            }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::database::DatabaseManager;
    use tempfile::TempDir;

    async fn create_test_state() -> DatabaseState {
        let dir = TempDir::new().unwrap();
        let manager = DatabaseManager::new(dir.path().to_path_buf()).unwrap();
        DatabaseState {
            manager: Arc::new(RwLock::new(manager)),
        }
    }

    #[tokio::test]
    async fn test_create_database_endpoint() {
        let state = create_test_state().await;

        let response = create_database(
            State(state),
            Json(CreateDatabaseRequest {
                name: "test_db".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_list_databases_endpoint() {
        let state = create_test_state().await;

        let response = list_databases(State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_drop_database_endpoint() {
        let state = create_test_state().await;

        // Create database first
        let manager = state.manager.read().await;
        manager.create_database("test_db").unwrap();
        drop(manager);

        let response = drop_database(State(state), Path("test_db".to_string())).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_database_endpoint() {
        let state = create_test_state().await;

        let response = get_database(State(state), Path("neo4j".to_string())).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_nonexistent_database() {
        let state = create_test_state().await;

        let response = get_database(State(state), Path("nonexistent".to_string())).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
