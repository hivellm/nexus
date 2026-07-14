//! Streaming MCP handler write-path tests (bug B7).
//!
//! Before the fix, `handle_execute_cypher` hand-rolled a CREATE-only
//! mini-fork that only understood literal node properties: `$params`
//! and any non-literal expression silently became `null`, and anything
//! other than a top-level CREATE (MERGE, SET, REMOVE, FOREACH) fell
//! through to the lock-free, write-unaware executor.
//! (docs/nexus/02-bug-inventory.md, docs/nexus/04-write-path-unification.md)

use nexus_core::auth::{
    AuditConfig, AuditLogger, AuthConfig, AuthManager, JwtConfig, JwtManager, Permission,
    RateLimits, RoleBasedAccessControl,
};
use nexus_core::catalog::{CATALOG_MMAP_INITIAL_SIZE, Catalog};
use nexus_core::database::DatabaseManager;
use nexus_core::index::{DEFAULT_VECTORIZER_DIMENSION, KnnIndex, LabelIndex};
use nexus_core::storage::RecordStore;
use nexus_core::testing::TestContext;
use nexus_core::{Engine, executor::Executor};
use nexus_server::{NexusServer, config::RootUserConfig};
use parking_lot::RwLock;
use rmcp::model::CallToolRequestParam;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

/// Mirrors `graphql_integration_test.rs::create_test_server` — an
/// isolated catalog/store/engine per test so parallel test-binary
/// execution can't cross-contaminate label/property state.
async fn create_test_server() -> (Arc<NexusServer>, TestContext) {
    let ctx = TestContext::new();
    let data_dir = ctx.path().to_path_buf();
    std::fs::create_dir_all(&data_dir).unwrap();

    let engine = Engine::with_isolated_catalog(&data_dir).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));

    let catalog = Catalog::with_isolated_path(
        data_dir.join("executor_catalog.mdb"),
        CATALOG_MMAP_INITIAL_SIZE,
    )
    .unwrap();
    let store = RecordStore::new(&data_dir).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
    let executor_arc = Arc::new(executor);

    let database_manager = DatabaseManager::new(data_dir.clone()).unwrap();
    let database_manager_arc = Arc::new(RwLock::new(database_manager));

    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(TokioRwLock::new(rbac));

    let auth_config = AuthConfig {
        enabled: false,
        required_for_public: false,
        default_permissions: vec![Permission::Read, Permission::Write],
        rate_limits: RateLimits {
            per_minute: 1000,
            per_hour: 10000,
        },
    };
    let auth_storage_path = data_dir.join("auth");
    std::fs::create_dir_all(&auth_storage_path).unwrap();
    let auth_manager =
        Arc::new(AuthManager::with_storage(auth_config.clone(), auth_storage_path).unwrap());

    let jwt_manager = Arc::new(JwtManager::new(JwtConfig::from_env()));

    let audit_logger = Arc::new(
        AuditLogger::new(AuditConfig {
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

/// Call the `execute_cypher` MCP tool through the public dispatcher —
/// the same entry point the StreamableHTTP transport uses — and parse
/// its JSON text response.
async fn call_execute_cypher(
    server: &Arc<NexusServer>,
    query: &str,
    params: Option<Value>,
) -> Value {
    let mut args = serde_json::Map::new();
    args.insert("query".to_string(), json!(query));
    if let Some(p) = params {
        args.insert("params".to_string(), p);
    }

    let request = CallToolRequestParam {
        name: "execute_cypher".into(),
        arguments: Some(args),
    };

    let result = nexus_server::api::streaming::handle_nexus_mcp_tool(request, server.clone())
        .await
        .expect("execute_cypher tool call failed");

    let text_content = result.content[0]
        .as_text()
        .expect("expected text content from execute_cypher tool result");
    serde_json::from_str(&text_content.text).expect("tool result was not valid JSON")
}

#[tokio::test]
async fn test_streaming_create_with_params_persists_value() {
    let (server, _ctx) = create_test_server().await;

    let create_response = call_execute_cypher(
        &server,
        "CREATE (n:StreamParamTest {x: $v}) RETURN id(n)",
        Some(json!({ "v": 7 })),
    )
    .await;
    assert_eq!(
        create_response["row_count"].as_u64(),
        Some(1),
        "CREATE should report exactly one created row: {:?}",
        create_response
    );

    // Re-read via a separate execute_cypher call to prove the
    // parameterised property was actually stored as 7, not null.
    let verify_response = call_execute_cypher(
        &server,
        "MATCH (n:StreamParamTest {x: 7}) RETURN id(n)",
        None,
    )
    .await;

    assert_eq!(
        verify_response["row_count"].as_u64(),
        Some(1),
        "CREATE with a $param property must persist the parameterised value, \
         not null (bug B7): {:?}",
        verify_response
    );
}
