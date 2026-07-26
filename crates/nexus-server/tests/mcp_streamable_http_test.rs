//! Real rmcp `StreamableHttpService` transport, end-to-end.
//!
//! `streaming_mcp_write_test.rs` calls `handle_nexus_mcp_tool` directly —
//! it never touches the HTTP wire. This test drives the actual
//! `StreamableHttpService` + `LocalSessionManager` wired the same way
//! production does (`crates/nexus-server/src/main.rs::create_mcp_router`,
//! mounted at `/mcp`): raw JSON-RPC request bodies in, SSE-framed
//! responses out, real session-id handshake. It proves `ServerHandler`'s
//! `get_info`, `list_tools`, and `call_tool` all flow correctly through
//! the real transport, not just through the in-process dispatcher.

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use futures::StreamExt;
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
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock as TokioRwLock;

/// Mirrors `streaming_mcp_write_test.rs::create_test_server` — an isolated
/// catalog/store/engine per test so parallel test-binary execution can't
/// cross-contaminate label/property state.
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

/// Build a raw-JSON POST request for the MCP StreamableHTTP endpoint,
/// with an optional `Mcp-Session-Id` header attached once a session has
/// been negotiated by `initialize`.
fn mcp_post_request(body: Value, session_id: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(Method::POST)
        .uri("/mcp")
        // rmcp >= 1.x enforces DNS-rebinding protection: every request must
        // carry a `Host` header in the allowed list (default: localhost,
        // 127.0.0.1, ::1). A real localhost HTTP/1.1 client always sends one.
        .header(header::HOST, "localhost")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json, text/event-stream");
    if let Some(session_id) = session_id {
        builder = builder.header("Mcp-Session-Id", session_id);
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("valid MCP request")
}

/// Read an SSE-framed response body (`event: message\ndata: {json}\n\n`)
/// until the first `data:` line parses as JSON, then return it.
///
/// The response stream stays OPEN (15s keep-alive pings) so this must NOT
/// drain the body to completion — it returns as soon as a data frame
/// decodes.
async fn read_sse_json(resp: axum::response::Response) -> Value {
    let mut stream = resp.into_body().into_data_stream();
    let mut buf = String::new();
    loop {
        let next = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("SSE read timed out");
        match next {
            Some(Ok(bytes)) => {
                buf.push_str(std::str::from_utf8(&bytes).expect("SSE frame was not valid utf8"));
                for line in buf.lines() {
                    if let Some(rest) = line.strip_prefix("data:") {
                        if let Ok(v) = serde_json::from_str::<Value>(rest.trim()) {
                            return v;
                        }
                    }
                }
            }
            _ => panic!("SSE stream ended before a data frame; buffered:\n{buf}"),
        }
    }
}

/// Wrap an rmcp `StreamableHttpService` response
/// (`http::Response<BoxBody<Bytes, Infallible>>`) into an
/// `axum::response::Response` so it can be read with `read_sse_json`.
///
/// The concrete body type is left to inference (rather than named via
/// `http_body_util`/`bytes` paths, which aren't direct dependencies of
/// this crate) — `Body::new` type-checks it against its own bounds.
macro_rules! into_axum_response {
    ($resp:expr) => {{
        let (parts, body) = $resp.into_parts();
        axum::response::Response::from_parts(parts, Body::new(body))
    }};
}

#[tokio::test]
async fn test_streamable_http_transport_full_handshake() {
    let (server, _ctx) = create_test_server().await;

    let streamable = StreamableHttpService::new(
        move || {
            Ok(nexus_server::api::streaming::NexusMcpService::new(
                server.clone(),
            ))
        },
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // 1. initialize (no session header yet) — proves `get_info()` flows
    //    through the real transport.
    let init_request = mcp_post_request(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "nexus-test-client", "version": "0.0.0"}
            }
        }),
        None,
    );
    let init_response = streamable.handle(init_request).await;
    assert_eq!(
        init_response.status(),
        StatusCode::OK,
        "initialize must return 200"
    );
    let session_id = init_response
        .headers()
        .get("Mcp-Session-Id")
        .expect("initialize response must carry an Mcp-Session-Id header")
        .to_str()
        .expect("session id header must be valid utf8")
        .to_string();

    let init_json = read_sse_json(into_axum_response!(init_response)).await;
    assert_eq!(
        init_json["result"]["serverInfo"]["name"],
        json!("nexus-server"),
        "initialize response must carry get_info()'s serverInfo through the real transport: {init_json:?}"
    );

    // 2. notifications/initialized (WITH session header) — expect 202,
    //    empty body, no SSE framing to read.
    let initialized_request = mcp_post_request(
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        Some(&session_id),
    );
    let initialized_response = streamable.handle(initialized_request).await;
    assert_eq!(
        initialized_response.status(),
        StatusCode::ACCEPTED,
        "notifications/initialized must be accepted"
    );

    // 3. tools/list (WITH session header) — exercises `list_tools`.
    let list_request = mcp_post_request(
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        Some(&session_id),
    );
    let list_response = streamable.handle(list_request).await;
    assert_eq!(list_response.status(), StatusCode::OK);
    let list_json = read_sse_json(into_axum_response!(list_response)).await;
    let tools = list_json["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("tools/list result.tools must be an array: {list_json:?}"));
    assert_eq!(
        tools.len(),
        9,
        "expected 9 registered MCP tools: {list_json:?}"
    );
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    for expected in [
        "create_node",
        "execute_cypher",
        "knn_search",
        "graph_correlation_generate",
    ] {
        assert!(
            names.contains(&expected),
            "tools/list missing '{expected}': {names:?}"
        );
    }

    // 4. tools/call execute_cypher (WITH session header) — exercises
    //    `call_tool` end-to-end through the real transport.
    let call_request = mcp_post_request(
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "execute_cypher",
                "arguments": {"query": "RETURN 1 as test"}
            }
        }),
        Some(&session_id),
    );
    let call_response = streamable.handle(call_request).await;
    assert_eq!(call_response.status(), StatusCode::OK);
    let call_json = read_sse_json(into_axum_response!(call_response)).await;
    assert!(
        call_json.get("result").is_some() && call_json.get("error").is_none(),
        "tools/call execute_cypher must succeed: {call_json:?}"
    );
}
