//! Regression suite for `phase0_fix-auth-management-authorization`.
//!
//! None of the `/auth/*` management handlers used to check the CALLING key's
//! permissions before acting on the request body — they only authenticated the
//! caller. With auth enabled a Read-only key could mint a Super key or manage
//! users/permissions. These tests exercise the handlers directly, injecting the
//! `AuthContext` the middleware would provide, and assert that a caller lacking
//! `Admin`/`Super` is rejected (`403`), that an Admin key cannot escalate to
//! `Super` (no vertical escalation), and that a properly privileged caller
//! still succeeds.

use axum::extract::{Extension, Json, Path, Query, State};
use axum::http::StatusCode;
use nexus_core::auth::middleware::AuthContext;
use nexus_core::auth::{ApiKey, AuthConfig, AuthManager, Permission};
use nexus_core::testing::TestContext;
use nexus_server::api::auth::{
    CreateApiKeyRequest, CreateUserRequest, UpdatePermissionsRequest, create_api_key, create_user,
    delete_user, get_api_key, get_user, get_user_permissions, grant_permissions, list_api_keys,
    list_users, revoke_permission,
};
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Build a NexusServer with authentication enabled.
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

// ── create_api_key ───────────────────────────────────────────────────────

#[tokio::test]
async fn readonly_key_cannot_mint_super_key() {
    let (server, _ctx) = server_with_auth().await;
    let res = create_api_key(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Json(CreateApiKeyRequest {
            name: "pwn".to_string(),
            username: None,
            permissions: Some(vec!["SUPER".to_string()]),
            expires_in: None,
        }),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key minting a SUPER key must be rejected with 403, got {:?}",
        res.as_ref().map(|_| "Ok")
    );
}

#[tokio::test]
async fn admin_key_cannot_escalate_to_super_via_new_key() {
    let (server, _ctx) = server_with_auth().await;
    let res = create_api_key(
        State(server),
        Extension(caller(vec![Permission::Admin])),
        Json(CreateApiKeyRequest {
            name: "escalate".to_string(),
            username: None,
            permissions: Some(vec!["SUPER".to_string()]),
            expires_in: None,
        }),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "an Admin key must not mint a SUPER key it does not itself hold (403), got Ok"
    );
}

#[tokio::test]
async fn super_key_can_mint_super_key() {
    let (server, _ctx) = server_with_auth().await;
    let res = create_api_key(
        State(server),
        Extension(caller(vec![Permission::Super])),
        Json(CreateApiKeyRequest {
            name: "legit".to_string(),
            username: None,
            permissions: Some(vec!["SUPER".to_string()]),
            expires_in: None,
        }),
    )
    .await;
    assert!(
        res.is_ok(),
        "a Super key minting a Super key must succeed, got {res:?}"
    );
}

// ── grant_permissions / create_user / delete_user / revoke_permission ─────

#[tokio::test]
async fn readonly_key_cannot_grant_permissions() {
    let (server, _ctx) = server_with_auth().await;
    let res = grant_permissions(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path("someuser".to_string()),
        Json(UpdatePermissionsRequest {
            permissions: vec!["ADMIN".to_string()],
        }),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not grant permissions (403), got Ok"
    );
}

#[tokio::test]
async fn readonly_key_cannot_create_user() {
    let (server, _ctx) = server_with_auth().await;
    let res = create_user(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Json(CreateUserRequest {
            username: "victim".to_string(),
            password: Some("pw".to_string()),
            email: None,
        }),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not create users (403), got Ok"
    );
}

#[tokio::test]
async fn admin_key_can_create_user() {
    let (server, _ctx) = server_with_auth().await;
    let res = create_user(
        State(server),
        Extension(caller(vec![Permission::Admin])),
        Json(CreateUserRequest {
            username: "legit-user".to_string(),
            password: Some("pw".to_string()),
            email: None,
        }),
    )
    .await;
    assert!(
        res.is_ok(),
        "an Admin key must still be able to create users, got {res:?}"
    );
}

#[tokio::test]
async fn readonly_key_cannot_delete_user() {
    let (server, _ctx) = server_with_auth().await;
    let res = delete_user(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path("victim".to_string()),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not delete users (403), got Ok"
    );
}

#[tokio::test]
async fn readonly_key_cannot_revoke_permission() {
    let (server, _ctx) = server_with_auth().await;
    let res = revoke_permission(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path(("victim".to_string(), "WRITE".to_string())),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not revoke permissions (403), got Ok"
    );
}

// ── read/inspection handlers (list/get users + keys) ──────────────────────

#[tokio::test]
async fn readonly_key_cannot_list_users() {
    let (server, _ctx) = server_with_auth().await;
    let res = list_users(State(server), Extension(caller(vec![Permission::Read]))).await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not enumerate users (403), got Ok"
    );
}

#[tokio::test]
async fn admin_key_can_list_users() {
    let (server, _ctx) = server_with_auth().await;
    let res = list_users(State(server), Extension(caller(vec![Permission::Admin]))).await;
    assert!(
        res.is_ok(),
        "an Admin key must be able to list users, got {res:?}"
    );
}

#[tokio::test]
async fn readonly_key_cannot_get_user() {
    let (server, _ctx) = server_with_auth().await;
    let res = get_user(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path("someuser".to_string()),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not read a user record (403), got Ok"
    );
}

#[tokio::test]
async fn readonly_key_cannot_get_user_permissions() {
    let (server, _ctx) = server_with_auth().await;
    let res = get_user_permissions(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path("someuser".to_string()),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not read a user's permissions (403), got Ok"
    );
}

#[tokio::test]
async fn readonly_key_cannot_list_api_keys() {
    let (server, _ctx) = server_with_auth().await;
    let res = list_api_keys(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Query(HashMap::new()),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not enumerate API keys (403), got Ok"
    );
}

#[tokio::test]
async fn readonly_key_cannot_get_api_key() {
    let (server, _ctx) = server_with_auth().await;
    let res = get_api_key(
        State(server),
        Extension(caller(vec![Permission::Read])),
        Path("some-key-id".to_string()),
    )
    .await;
    assert!(
        matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "a Read-only key must not read an API key record (403), got Ok"
    );
}

// ── auth-disabled (no identity) must still work — bootstrapping ────────────

#[tokio::test]
async fn auth_disabled_still_allows_management() {
    // When authentication is disabled the request carries no identity (`None`),
    // so management must not be blocked here (the disabled-surface risk is
    // handled by phase0_fix-server-secure-defaults-and-dos). A `None` caller
    // creating a user must not be rejected with 403.
    let (server, _ctx) = server_with_auth().await;
    let res = create_user(
        State(server),
        Extension(None),
        Json(CreateUserRequest {
            username: "bootstrap".to_string(),
            password: Some("pw".to_string()),
            email: None,
        }),
    )
    .await;
    assert!(
        !matches!(&res, Err((StatusCode::FORBIDDEN, _))),
        "with auth disabled (no identity), management must not be 403'd, got {res:?}"
    );
}
