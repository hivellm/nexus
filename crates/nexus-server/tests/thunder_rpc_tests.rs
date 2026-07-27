//! Integration tests driving the migrated RPC server through the real
//! `thunder::client::Client` (phase10 §5) — the raw-codec suite in
//! `rpc_integration_test.rs` proves the wire, this proves the full client
//! path: handshake, the `AUTH` command under the `AuthCommand` profile, the
//! pre-auth allowlist, request/response multiplexing, and the `ClientError`
//! taxonomy.

use std::sync::Arc;

use nexus_server::protocol::rpc::{nexus_thunder_config, spawn_rpc_listener};
use thunder::Value;
use thunder::client::{Client, ClientConfig, ClientError};
use tokio::net::TcpListener;

/// Stand up a real `NexusServer` + Thunder listener on a loopback port and
/// return the address. The `ListenerHandle` is leaked so it lives for the
/// whole test (dropping it would shut the listener down).
async fn spawn_server(auth_required: bool) -> std::net::SocketAddr {
    let ctx = nexus_core::testing::TestContext::new();
    let engine =
        nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for thunder rpc test");
    let engine_arc = Arc::new(tokio::sync::RwLock::new(engine));
    let executor_arc = Arc::new(nexus_core::executor::Executor::default());
    let dbm_arc = Arc::new(parking_lot::RwLock::new(
        nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
    ));
    let rbac_arc = Arc::new(tokio::sync::RwLock::new(
        nexus_core::auth::RoleBasedAccessControl::new(),
    ));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: ctx.path().join("audit"),
            retention_days: 1,
            compress_logs: false,
        })
        .expect("audit init"),
    );
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
        nexus_core::auth::AuthConfig::default(),
    ));
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
        nexus_core::auth::JwtConfig::default(),
    ));
    let server = Arc::new(nexus_server::NexusServer::new(
        executor_arc,
        engine_arc,
        dbm_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        nexus_server::config::RootUserConfig::default(),
    ));
    let _leaked = Box::leak(Box::new(ctx));

    let scratch = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = scratch.local_addr().unwrap();
    drop(scratch);

    let handle = spawn_rpc_listener(
        server,
        addr,
        nexus_server::config::RpcConfig::default(),
        auth_required,
    )
    .await
    .unwrap();
    Box::leak(Box::new(handle));
    addr
}

async fn connect(addr: std::net::SocketAddr, cfg: ClientConfig) -> Client {
    // Small retry — the listener task may not have finished binding the
    // instant `spawn_rpc_listener` returns on a loaded runner.
    let endpoint = format!("nexus://{addr}");
    for _ in 0..20 {
        match Client::connect_with(&endpoint, nexus_thunder_config(), cfg.clone()).await {
            Ok(c) => return c,
            Err(_) => tokio::time::sleep(std::time::Duration::from_millis(10)).await,
        }
    }
    Client::connect_with(&endpoint, nexus_thunder_config(), cfg)
        .await
        .expect("connect")
}

#[tokio::test]
async fn ping_pongs_over_the_thunder_client() {
    let addr = spawn_server(false).await;
    let client = connect(addr, ClientConfig::new()).await;
    let pong = client.call("PING", vec![]).await.expect("PING");
    assert_eq!(pong, Value::Str("PONG".into()));
    client.close().await;
}

#[tokio::test]
async fn cypher_round_trips_over_the_thunder_client() {
    let addr = spawn_server(false).await;
    let client = connect(addr, ClientConfig::new()).await;
    let out = client
        .call("CYPHER", vec![Value::from("RETURN 1 AS x")])
        .await
        .expect("CYPHER");
    // The server returns a `{columns, rows, ...}` map envelope.
    let Value::Map(entries) = out else {
        panic!("expected Map envelope, got {out:?}");
    };
    let rows = entries
        .iter()
        .find_map(|(k, v)| (k.as_str() == Some("rows")).then_some(v))
        .expect("rows key");
    let Value::Array(rows) = rows else {
        panic!("rows must be an Array")
    };
    assert_eq!(rows.len(), 1);
    client.close().await;
}

#[tokio::test]
async fn unknown_command_is_a_server_error_that_keeps_the_connection() {
    let addr = spawn_server(false).await;
    let client = connect(addr, ClientConfig::new()).await;
    match client.call("NOT_A_COMMAND", vec![]).await {
        Err(ClientError::Server { message, .. }) => {
            assert!(message.contains("unknown command"), "got: {message}");
        }
        other => panic!("expected a Server error, got {other:?}"),
    }
    // The connection survives a dispatch error.
    let pong = client.call("PING", vec![]).await.expect("PING after error");
    assert_eq!(pong, Value::Str("PONG".into()));
    client.close().await;
}

#[tokio::test]
async fn auth_required_gates_cypher_then_root_authenticates() {
    let addr = spawn_server(true).await;

    // No credentials → the connection is open (AuthCommand handshake sends
    // no AUTH) but a non-allowlisted command is refused with NOAUTH, which
    // the client surfaces as the Auth error class.
    let anon = connect(addr, ClientConfig::new()).await;
    match anon.call("CYPHER", vec![Value::from("RETURN 1")]).await {
        Err(ClientError::Auth { message }) => assert!(message.contains("NOAUTH"), "got: {message}"),
        other => panic!("expected NOAUTH Auth error, got {other:?}"),
    }
    // Pre-auth PING is always allowed.
    assert_eq!(
        anon.call("PING", vec![]).await.expect("pre-auth PING"),
        Value::Str("PONG".into())
    );
    anon.close().await;

    // With root/root the client authenticates during the handshake and
    // CYPHER succeeds.
    let authed = connect(addr, ClientConfig::new().user_pass("root", "root")).await;
    let out = authed
        .call("CYPHER", vec![Value::from("RETURN 1 AS x")])
        .await
        .expect("authenticated CYPHER");
    assert!(matches!(out, Value::Map(_)));
    authed.close().await;
}

#[tokio::test]
async fn wrong_password_fails_the_handshake_with_an_auth_error() {
    let addr = spawn_server(true).await;
    let endpoint = format!("nexus://{addr}");
    let res = Client::connect_with(
        &endpoint,
        nexus_thunder_config(),
        ClientConfig::new().user_pass("root", "not-the-password"),
    )
    .await;
    match res {
        Err(ClientError::Auth { message }) => {
            assert!(
                message.contains("WRONGPASS") || message.to_lowercase().contains("auth"),
                "got: {message}"
            );
        }
        other => panic!("expected an Auth error on wrong password, got {other:?}"),
    }
}
