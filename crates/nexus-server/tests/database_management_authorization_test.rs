//! Regression suite for `phase0_fix-multi-database-persistence-and-default`
//! §G3.
//!
//! `CREATE DATABASE` / `DROP DATABASE` mutate server-wide state shared by
//! every caller, but neither the Cypher DDL path (`execute_cypher` routing
//! to `execute_database_commands`) nor the REST `/databases` endpoints
//! checked the CALLING key's own permissions — they only authenticated the
//! caller (or ran unauthenticated). With auth enabled, a Read-only key could
//! create or drop any database. These tests exercise both surfaces and
//! assert that a caller lacking `Admin`/`Super` is rejected, that an Admin
//! caller still succeeds, and that auth-disabled (no identity) callers are
//! not blocked — mirroring `auth_management_authorization_test.rs`.
//!
//! Note: `execute_database_commands` itself is `pub(crate)` (internal to
//! `nexus_server`), so the Cypher-path cases below drive it indirectly
//! through the public `execute_cypher` handler — the same entry point real
//! clients use, and the only one this external test crate can reach.

use axum::extract::{Extension, Json, Path, State};
use nexus_core::auth::middleware::AuthContext;
use nexus_core::auth::{ApiKey, AuthConfig, AuthManager, Permission};
use nexus_core::testing::TestContext;
use nexus_server::api::cypher::{CypherRequest, CypherResponse, execute_cypher};
use nexus_server::api::database::{
    CreateDatabaseRequest, DatabaseState, create_database, drop_database,
};
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Build a NexusServer with authentication enabled — mirrors
/// `auth_management_authorization_test.rs::server_with_auth`.
async fn server_with_auth() -> (Arc<NexusServer>, TestContext) {
    use nexus_core::{
        Engine, auth::RoleBasedAccessControl, database::DatabaseManager, executor::Executor,
    };

    let ctx = TestContext::new();
    let data_dir = ctx.path().to_path_buf();
    std::fs::create_dir_all(&data_dir).unwrap();

    let engine = Engine::with_data_dir(&data_dir).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));
    let executor_arc = Arc::new(Executor::default());
    let database_manager_arc =
        Arc::new(RwLock::new(DatabaseManager::new(data_dir.clone()).unwrap()));
    let rbac_arc = Arc::new(TokioRwLock::new(RoleBasedAccessControl::new()));

    let auth_config = AuthConfig {
        enabled: true,
        required_for_public: false,
        default_permissions: vec![Permission::Read, Permission::Write],
        rate_limits: nexus_core::auth::RateLimits {
            per_minute: 1000,
            per_hour: 10000,
        },
    };
    let auth_storage_path = data_dir.join("auth");
    std::fs::create_dir_all(&auth_storage_path).unwrap();
    let auth_manager = Arc::new(AuthManager::with_storage(auth_config, auth_storage_path).unwrap());

    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
        nexus_core::auth::JwtConfig::from_env(),
    ));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
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
    (server, ctx)
}

/// An `AuthContext` for a caller holding exactly `permissions`, as the auth
/// middleware would inject after authenticating the request.
fn caller(permissions: Vec<Permission>) -> Option<AuthContext> {
    Some(AuthContext {
        api_key: ApiKey::new(
            "test-key-id".to_string(),
            "test-key".to_string(),
            permissions,
            "hashed".to_string(),
        ),
        required: true,
    })
}

/// Run a Cypher query through the public `execute_cypher` handler under a
/// given caller identity (`None` == auth disabled / no identity).
async fn run_cypher(
    server: &Arc<NexusServer>,
    query: &str,
    auth_context: Option<AuthContext>,
) -> CypherResponse {
    let req = CypherRequest {
        query: query.to_string(),
        params: Default::default(),
        database: None,
    };
    execute_cypher(
        State(server.clone()),
        Some(Extension(auth_context)),
        Json(req),
    )
    .await
    .0
}

fn database_state(server: &Arc<NexusServer>) -> DatabaseState {
    DatabaseState {
        manager: server.database_manager.clone(),
    }
}

// ── Cypher DDL path (`CREATE/DROP DATABASE` via `execute_cypher`) ─────────

#[tokio::test]
async fn readonly_caller_cannot_create_database_via_cypher() {
    let (server, _ctx) = server_with_auth().await;
    let resp = run_cypher(
        &server,
        "CREATE DATABASE cypher_readonly_db",
        caller(vec![Permission::Read]),
    )
    .await;
    assert!(
        resp.error.is_some(),
        "a Read-only caller must not be able to CREATE DATABASE over Cypher, got {resp:?}"
    );
    assert!(
        resp.error.unwrap().contains("Insufficient permissions"),
        "expected a permissions error"
    );
}

#[tokio::test]
async fn admin_caller_can_create_database_via_cypher() {
    let (server, _ctx) = server_with_auth().await;
    let resp = run_cypher(
        &server,
        "CREATE DATABASE cypher_admin_db",
        caller(vec![Permission::Admin]),
    )
    .await;
    assert!(
        resp.error.is_none(),
        "an Admin caller must be able to CREATE DATABASE over Cypher, got {resp:?}"
    );
}

#[tokio::test]
async fn readonly_caller_cannot_drop_database_via_cypher() {
    let (server, _ctx) = server_with_auth().await;
    // Seed the database as Admin first so the DROP attempt below fails on
    // authorization, not on a missing database.
    let seed = run_cypher(
        &server,
        "CREATE DATABASE cypher_readonly_drop_db",
        caller(vec![Permission::Admin]),
    )
    .await;
    assert!(seed.error.is_none(), "seed creation failed: {seed:?}");

    let resp = run_cypher(
        &server,
        "DROP DATABASE cypher_readonly_drop_db",
        caller(vec![Permission::Read]),
    )
    .await;
    assert!(
        resp.error.is_some(),
        "a Read-only caller must not be able to DROP DATABASE over Cypher, got {resp:?}"
    );
    assert!(
        resp.error.unwrap().contains("Insufficient permissions"),
        "expected a permissions error"
    );
}

#[tokio::test]
async fn auth_disabled_caller_can_create_database_via_cypher() {
    // When authentication is disabled the request carries no identity
    // (`None`), so database management must not be blocked here — the same
    // bootstrap allowance `caller_is_admin`/`require_admin` grant in
    // `api::auth`.
    let (server, _ctx) = server_with_auth().await;
    let resp = run_cypher(&server, "CREATE DATABASE cypher_bootstrap_db", None).await;
    assert!(
        resp.error.is_none(),
        "with auth disabled (no identity), CREATE DATABASE must not be blocked, got {resp:?}"
    );
}

// ── REST path (`POST /databases`, `DELETE /databases/{name}`) ─────────────

#[tokio::test]
async fn readonly_caller_cannot_create_database_via_rest() {
    let (server, _ctx) = server_with_auth().await;
    let response = create_database(
        State(database_state(&server)),
        Some(Extension(caller(vec![Permission::Read]))),
        Json(CreateDatabaseRequest {
            name: "rest_readonly_db".to_string(),
        }),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "a Read-only key must not create databases via REST (403)"
    );
}

#[tokio::test]
async fn admin_caller_can_create_database_via_rest() {
    let (server, _ctx) = server_with_auth().await;
    let response = create_database(
        State(database_state(&server)),
        Some(Extension(caller(vec![Permission::Admin]))),
        Json(CreateDatabaseRequest {
            name: "rest_admin_db".to_string(),
        }),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "an Admin key must be able to create databases via REST"
    );
}

#[tokio::test]
async fn readonly_caller_cannot_drop_database_via_rest() {
    let (server, _ctx) = server_with_auth().await;
    // Seed the database as Admin first so the DROP attempt below fails on
    // authorization, not on a missing database.
    let seed = create_database(
        State(database_state(&server)),
        Some(Extension(caller(vec![Permission::Admin]))),
        Json(CreateDatabaseRequest {
            name: "rest_readonly_drop_db".to_string(),
        }),
    )
    .await;
    assert_eq!(seed.status(), axum::http::StatusCode::OK, "seed failed");

    let response = drop_database(
        State(database_state(&server)),
        Some(Extension(caller(vec![Permission::Read]))),
        Path("rest_readonly_drop_db".to_string()),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "a Read-only key must not drop databases via REST (403)"
    );
}

#[tokio::test]
async fn auth_disabled_caller_can_create_database_via_rest() {
    let (server, _ctx) = server_with_auth().await;
    let response = create_database(
        State(database_state(&server)),
        Some(Extension(None)),
        Json(CreateDatabaseRequest {
            name: "rest_bootstrap_db".to_string(),
        }),
    )
    .await;
    assert_ne!(
        response.status(),
        axum::http::StatusCode::FORBIDDEN,
        "with auth disabled (no identity), REST database creation must not be 403'd"
    );
}

/// Regression test for the auth-disabled 500: when authentication is
/// disabled the auth middleware layer that injects
/// `Extension(None::<AuthContext>)` is not applied at all (see
/// `main.rs`'s `config.auth.enabled || cluster_enabled` gate), so the
/// request truly carries NO `Extension<Option<AuthContext>>` — not even a
/// `Some(None)`. Before the fix, `create_database`/`drop_database` extracted
/// this via a bare (required) `Extension<Option<AuthContext>>` parameter,
/// so axum's `FromRequestParts` failed with "Missing request extension" and
/// the handler never ran, producing an HTTP 500 for every management call
/// while auth was off. This test simulates that exact scenario — passing
/// `None` for the optional extension — and asserts the handler still
/// completes successfully instead of failing to extract.
#[tokio::test]
async fn create_database_succeeds_with_missing_auth_extension() {
    let (server, _ctx) = server_with_auth().await;
    let response = create_database(
        State(database_state(&server)),
        None,
        Json(CreateDatabaseRequest {
            name: "rest_missing_extension_db".to_string(),
        }),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "create_database must succeed when the Extension<Option<AuthContext>> is entirely \
         absent (auth-disabled, middleware not applied), not fail extraction with a 500"
    );
}

/// Same regression, for `drop_database` — the fix must cover both
/// management handlers in `api::database`.
#[tokio::test]
async fn drop_database_succeeds_with_missing_auth_extension() {
    let (server, _ctx) = server_with_auth().await;
    let seed = create_database(
        State(database_state(&server)),
        None,
        Json(CreateDatabaseRequest {
            name: "rest_missing_extension_drop_db".to_string(),
        }),
    )
    .await;
    assert_eq!(seed.status(), axum::http::StatusCode::OK, "seed failed");

    let response = drop_database(
        State(database_state(&server)),
        None,
        Path("rest_missing_extension_drop_db".to_string()),
    )
    .await;
    assert_eq!(
        response.status(),
        axum::http::StatusCode::OK,
        "drop_database must succeed when the Extension<Option<AuthContext>> is entirely \
         absent (auth-disabled, middleware not applied), not fail extraction with a 500"
    );
}
