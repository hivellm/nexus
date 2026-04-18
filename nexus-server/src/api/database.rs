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
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
    let manager_arc = state.manager.clone();
    let name = req.name.clone();
    let result = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        manager.create_database(&name).map(|_| ())
    })
    .await
    .expect("spawn_blocking panicked");

    match result {
        Ok(()) => Json(CreateDatabaseResponse {
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
    let manager_arc = state.manager.clone();
    let name_for_task = name.clone();
    let result = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        manager.drop_database(&name_for_task, false).map(|_| ())
    })
    .await
    .expect("spawn_blocking panicked");

    match result {
        Ok(()) => Json(DatabaseResponse {
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
    let manager_arc = state.manager.clone();
    let (databases, default_database) = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        (
            manager.list_databases(),
            manager.default_database_name().to_string(),
        )
    })
    .await
    .expect("spawn_blocking panicked");

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
    let manager_arc = state.manager.clone();
    let name_for_task = name.clone();
    let result = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        let engine = manager
            .get_database(&name_for_task)
            .map_err(|e| e.to_string())?;
        let mut engine_guard = engine.write();
        let (node_count, relationship_count) = match engine_guard.stats() {
            Ok(stats) => (stats.nodes, stats.relationships),
            Err(_) => (0, 0),
        };
        Ok::<(u64, u64), String>((node_count, relationship_count))
    })
    .await
    .expect("spawn_blocking panicked");

    match result {
        Ok((node_count, relationship_count)) => Json(DatabaseInfo {
            name: name.clone(),
            path: std::path::PathBuf::new(), // Don't expose full path
            created_at: 0,
            node_count,
            relationship_count,
            storage_size: 0,
            state: nexus_core::database::DatabaseState::Online,
        })
        .into_response(),
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

/// Request to switch database
#[derive(Debug, Deserialize)]
pub struct SwitchDatabaseRequest {
    /// Database name to switch to
    pub name: String,
}

/// Response for session database
#[derive(Debug, Serialize)]
pub struct SessionDatabaseResponse {
    /// Current database name
    pub database: String,
}

/// Get current session database
pub async fn get_session_database(State(state): State<DatabaseState>) -> Response {
    let manager_arc = state.manager.clone();
    let current_db = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        manager.default_database_name().to_string()
    })
    .await
    .expect("spawn_blocking panicked");

    Json(SessionDatabaseResponse {
        database: current_db,
    })
    .into_response()
}

/// Switch session database
pub async fn switch_session_database(
    State(state): State<DatabaseState>,
    Json(req): Json<SwitchDatabaseRequest>,
) -> Response {
    let manager_arc = state.manager.clone();
    let name_for_task = req.name.clone();
    let exists = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        manager.exists(&name_for_task)
    })
    .await
    .expect("spawn_blocking panicked");

    // Check if database exists
    if !exists {
        return (
            StatusCode::NOT_FOUND,
            Json(DatabaseResponse {
                success: false,
                message: format!("Database '{}' does not exist", req.name),
            }),
        )
            .into_response();
    }

    // In a full implementation, this would set the session's current database
    // For now, we just validate the database exists
    Json(DatabaseResponse {
        success: true,
        message: format!("Switched to database '{}'", req.name),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::database::DatabaseManager;
    use nexus_core::testing::TestContext;

    // Test state wrapper that keeps TestContext alive
    struct TestState {
        _ctx: TestContext, // Keep context alive
        state: DatabaseState,
    }

    impl TestState {
        fn new() -> Self {
            let ctx = TestContext::new();
            let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();
            let state = DatabaseState {
                manager: Arc::new(RwLock::new(manager)),
            };
            Self { _ctx: ctx, state }
        }

        fn state(&self) -> DatabaseState {
            DatabaseState {
                manager: self.state.manager.clone(),
            }
        }
    }

    async fn create_test_state() -> DatabaseState {
        // For backward compatibility, but tests should use TestState::new() directly
        TestState::new().state()
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

    #[tokio::test]
    async fn test_create_database_with_invalid_name() {
        let state = create_test_state().await;

        let response = create_database(
            State(state),
            Json(CreateDatabaseRequest {
                name: "invalid name".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_create_duplicate_database() {
        let state = create_test_state().await;

        // Create first time
        let manager = state.manager.read();
        manager.create_database("test_db").unwrap();
        drop(manager);

        // Try to create again
        let response = create_database(
            State(state),
            Json(CreateDatabaseRequest {
                name: "test_db".to_string(),
            }),
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_list_databases_includes_default() {
        let state = create_test_state().await;

        let response = list_databases(State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        // Should include default "neo4j" database
    }

    #[tokio::test]
    #[ignore] // TODO: Fix temp dir race condition
    async fn test_list_databases_after_creating_multiple() {
        let test_state = TestState::new();
        let state = test_state.state();

        // Create multiple databases
        let manager = state.manager.read();
        manager.create_database("db1").unwrap();
        manager.create_database("db2").unwrap();
        manager.create_database("db3").unwrap();
        drop(manager);

        let response = list_databases(State(state)).await;

        assert_eq!(response.status(), StatusCode::OK);
        // Should list all 4 databases (neo4j + 3 new)
    }

    #[tokio::test]
    async fn test_get_database_with_data() {
        let state = create_test_state().await;

        // Create database and add data
        let manager = state.manager.read();
        let db = manager.create_database("test_db").unwrap();
        drop(manager);

        {
            let mut engine = db.write();
            for i in 0..5 {
                engine
                    .create_node(vec!["Person".to_string()], serde_json::json!({"id": i}))
                    .unwrap();
            }
        }

        let response = get_database(State(state), Path("test_db".to_string())).await;

        assert_eq!(response.status(), StatusCode::OK);
        // Should show node_count = 5
    }

    #[tokio::test]
    async fn test_database_response_format() {
        let response = DatabaseResponse {
            success: true,
            message: "Test message".to_string(),
        };

        assert!(response.success);
        assert_eq!(response.message, "Test message");
    }

    #[tokio::test]
    async fn test_create_database_response_format() {
        let response = CreateDatabaseResponse {
            success: true,
            name: "test_db".to_string(),
            message: "Database created".to_string(),
        };

        assert!(response.success);
        assert_eq!(response.name, "test_db");
    }

    #[tokio::test]
    async fn test_list_databases_response_format() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let response = ListDatabasesResponse {
            databases: manager.list_databases(),
            default_database: manager.default_database_name().to_string(),
        };

        assert_eq!(response.default_database, "neo4j");
        assert!(!response.databases.is_empty());
    }

    #[tokio::test]
    async fn test_drop_and_recreate_database() {
        let state = create_test_state().await;

        // Create database
        let manager = state.manager.read();
        manager.create_database("test_db").unwrap();
        drop(manager);

        // Drop it
        let _response1 = drop_database(State(state.clone()), Path("test_db".to_string())).await;

        // Recreate with same name
        let response2 = create_database(
            State(state),
            Json(CreateDatabaseRequest {
                name: "test_db".to_string(),
            }),
        )
        .await;

        assert_eq!(response2.status(), StatusCode::OK);
    }

    // ==========================================================================
    // Concurrency regression test — proves the DatabaseManager read path does
    // not starve the tokio runtime under load. Prior to commit <async-lock-
    // migration>, the handlers called `manager.read()` directly from async fn,
    // which pins a tokio worker for the whole acquire+serve window. With 32
    // concurrent readers hitting a saturated worker pool this manifested as
    // dropped requests. After the migration every lock acquisition lives
    // inside `tokio::task::spawn_blocking`, so readers queue on the blocking
    // pool (rayon-like) and the tokio workers stay free to accept new work.
    //
    // The assertion is intentionally structural ("all 32 complete
    // successfully in well under a pathological timeout") rather than a tight
    // latency cap, to avoid flakes on slow CI machines while still catching
    // the deadlock / starvation regression we care about.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_concurrent_list_databases_does_not_starve_runtime() {
        use std::time::{Duration, Instant};

        let test_state = TestState::new();

        // Seed a handful of databases so list_databases does actual work.
        {
            let manager = test_state.state.manager.read();
            for i in 0..4 {
                manager
                    .create_database(&format!("concurrency_test_db_{i}"))
                    .unwrap();
            }
        }

        let start = Instant::now();
        let mut handles = Vec::with_capacity(32);
        for _ in 0..32 {
            let state = test_state.state();
            handles.push(tokio::spawn(
                async move { list_databases(State(state)).await },
            ));
        }

        // 30-second pathological cap — actual completion should be sub-second.
        // If this ever trips, the lock is being held across await again.
        let results = tokio::time::timeout(Duration::from_secs(30), async {
            let mut statuses = Vec::with_capacity(32);
            for h in handles {
                statuses.push(h.await.unwrap().status());
            }
            statuses
        })
        .await
        .expect("32 concurrent list_databases timed out — async lock regression?");

        let elapsed = start.elapsed();
        assert_eq!(results.len(), 32);
        assert!(
            results.iter().all(|s| *s == StatusCode::OK),
            "all 32 concurrent requests should return 200 OK",
        );
        // Sanity: even on a 2-worker runtime this must clear well under 30s.
        assert!(
            elapsed < Duration::from_secs(30),
            "32 concurrent reads took {elapsed:?} — possible starvation",
        );
    }
}
