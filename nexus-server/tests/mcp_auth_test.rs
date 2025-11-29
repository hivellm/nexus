//! MCP Authentication Tests
//!
//! Tests for MCP authentication middleware and API key validation

use nexus_core::auth::{AuthConfig, AuthManager, Permission, middleware::AuthMiddleware};
use nexus_core::testing::TestContext;
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Helper function to create a test server with authentication enabled
async fn create_test_server_with_auth() -> (Arc<NexusServer>, Arc<AuthManager>, TestContext) {
    use nexus_core::{
        Engine, auth::RoleBasedAccessControl, database::DatabaseManager, executor::Executor,
    };

    let ctx = TestContext::new();
    let data_dir = ctx.path().to_path_buf();

    // Ensure data directory exists
    std::fs::create_dir_all(&data_dir).unwrap();

    // Initialize Engine
    let engine = Engine::with_data_dir(&data_dir).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));

    // Initialize executor
    let executor = Executor::default();
    let executor_arc = Arc::new(executor);

    // Initialize DatabaseManager (uses parking_lot::RwLock)
    let database_manager = DatabaseManager::new(data_dir.clone()).unwrap();
    let database_manager_arc = Arc::new(RwLock::new(database_manager));

    // Initialize RBAC (uses tokio::sync::RwLock)
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(TokioRwLock::new(rbac));

    // Initialize AuthManager with authentication enabled
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
    let auth_manager =
        Arc::new(AuthManager::with_storage(auth_config.clone(), auth_storage_path).unwrap());

    // Initialize JWT manager
    let jwt_config = nexus_core::auth::JwtConfig::from_env();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

    // Initialize audit logger
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false, // Disable audit logging in tests
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );

    // Create NexusServer
    let server = Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager.clone(),
        jwt_manager,
        audit_logger,
        RootUserConfig::default(),
    ));

    (server, auth_manager, ctx)
}

#[test]
fn test_mcp_auth_extract_api_key_bearer() {
    use axum::http::HeaderMap;

    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        axum::http::HeaderValue::from_str("Bearer nx_test123456789").unwrap(),
    );

    let api_key = AuthMiddleware::extract_api_key(&headers);
    assert_eq!(api_key, Some("nx_test123456789".to_string()));
}

#[test]
fn test_mcp_auth_extract_api_key_x_api_key() {
    use axum::http::HeaderMap;

    let mut headers = HeaderMap::new();
    headers.insert(
        "x-api-key",
        axum::http::HeaderValue::from_str("nx_test123456789").unwrap(),
    );

    let api_key = AuthMiddleware::extract_api_key(&headers);
    assert_eq!(api_key, Some("nx_test123456789".to_string()));
}

#[test]
fn test_mcp_auth_extract_api_key_none() {
    use axum::http::HeaderMap;

    let headers = HeaderMap::new();
    let api_key = AuthMiddleware::extract_api_key(&headers);
    assert_eq!(api_key, None);
}

#[tokio::test]
async fn test_mcp_auth_manager_verify_valid_key() {
    let (_server, auth_manager, _ctx) = create_test_server_with_auth().await;

    // Generate a valid API key
    let (_api_key, full_key) = auth_manager
        .generate_api_key("test-key".to_string(), vec![Permission::Read])
        .unwrap();

    // Verify the key
    let result = auth_manager.verify_api_key(&full_key);
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());
}

#[tokio::test]
async fn test_mcp_auth_manager_verify_invalid_key() {
    let (_server, auth_manager, _ctx) = create_test_server_with_auth().await;

    // Try to verify an invalid key
    let result = auth_manager.verify_api_key("nx_invalid_key_12345678901234567890");
    assert!(result.is_ok());
    assert!(result.unwrap().is_none());
}

#[tokio::test]
#[ignore] // TODO: Fix temp dir race condition - LMDB "No such file or directory" error
async fn test_mcp_auth_manager_revoke_key() {
    let (_server, auth_manager, _ctx) = create_test_server_with_auth().await;

    // Generate and revoke an API key
    let (api_key, full_key) = auth_manager
        .generate_api_key("test-key".to_string(), vec![Permission::Read])
        .unwrap();

    // Verify key works initially
    let result = auth_manager.verify_api_key(&full_key);
    assert!(result.is_ok());
    assert!(result.unwrap().is_some());

    // Revoke the key
    let revoke_result =
        auth_manager.revoke_api_key(&api_key.id, Some("Test revocation".to_string()));
    assert!(revoke_result.is_ok());

    // Verify key is now revoked (verify_api_key still returns the key, but it's marked as revoked)
    let result = auth_manager.verify_api_key(&full_key);
    assert!(result.is_ok());
    let verified = result.unwrap();
    if let Some(key) = verified {
        assert!(key.is_revoked);
    }
}

#[tokio::test]
async fn test_mcp_auth_manager_permissions() {
    let (_server, auth_manager, _ctx) = create_test_server_with_auth().await;

    // Generate API key with Read permission
    let (api_key, _full_key) = auth_manager
        .generate_api_key("test-key".to_string(), vec![Permission::Read])
        .unwrap();

    // Check permissions - Admin includes Read and Write
    // Since Admin includes Read, we check if Admin permission includes Read
    assert!(api_key.permissions.contains(&Permission::Read));
    assert!(!api_key.permissions.contains(&Permission::Write));
    assert!(!api_key.permissions.contains(&Permission::Admin));
}

#[tokio::test]
async fn test_mcp_auth_manager_admin_permissions() {
    let (_server, auth_manager, _ctx) = create_test_server_with_auth().await;

    // Generate API key with Admin permission
    let (api_key, _full_key) = auth_manager
        .generate_api_key("test-key".to_string(), vec![Permission::Admin])
        .unwrap();

    // Admin should have Read and Write permissions (via Permission::includes)
    assert!(Permission::Admin.includes(&Permission::Read));
    assert!(Permission::Admin.includes(&Permission::Write));
    assert!(api_key.permissions.contains(&Permission::Admin));
}
