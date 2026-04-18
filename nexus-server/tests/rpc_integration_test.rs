//! End-to-end integration tests for the native binary RPC transport.
//!
//! These start a real server via `NexusServer::new`, bind a listener on an
//! OS-picked loopback port, and drive it through `TcpStream` so the full
//! accept -> read -> dispatch -> write -> read pipeline is exercised.

use std::sync::Arc;
use std::time::Duration;

use nexus_protocol::rpc::{NexusValue, PUSH_ID, Request, read_response, write_request};
use tokio::net::{TcpListener, TcpStream};

use nexus_server::config::{RootUserConfig, RpcConfig};
use nexus_server::protocol::rpc::spawn_rpc_listener;

async fn spawn_server(auth_required: bool) -> std::net::SocketAddr {
    let ctx = nexus_core::testing::TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path())
        .expect("engine init for rpc integration test");
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
        RootUserConfig::default(),
    ));
    let _leaked = Box::leak(Box::new(ctx));

    // Bind + immediately drop just to learn a free port; the listener
    // will bind the same port inside `spawn_rpc_listener`.
    let scratch = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = scratch.local_addr().unwrap();
    drop(scratch);

    spawn_rpc_listener(server, addr, RpcConfig::default(), auth_required)
        .await
        .unwrap();

    addr
}

async fn connect(addr: std::net::SocketAddr) -> TcpStream {
    for _ in 0..20 {
        if let Ok(s) = TcpStream::connect(addr).await {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("could not connect to {addr}");
}

#[tokio::test]
async fn ping_round_trip_returns_pong_in_under_50ms() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    let started = std::time::Instant::now();
    write_request(
        &mut stream,
        &Request {
            id: 1,
            command: "PING".into(),
            args: vec![],
        },
    )
    .await
    .unwrap();
    let resp = read_response(&mut stream).await.unwrap();
    let elapsed_ms = started.elapsed().as_millis();

    assert_eq!(resp.id, 1);
    assert_eq!(resp.result.unwrap(), NexusValue::Str("PONG".into()));
    // 50 ms is loose enough for CI jitter on a busy runner.
    assert!(elapsed_ms < 50, "PING round-trip took {elapsed_ms}ms");
}

#[tokio::test]
async fn cypher_return_1_matches_expected_envelope_shape() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    write_request(
        &mut stream,
        &Request {
            id: 7,
            command: "CYPHER".into(),
            args: vec![NexusValue::Str("RETURN 1 AS x".into())],
        },
    )
    .await
    .unwrap();

    let resp = read_response(&mut stream).await.unwrap();
    assert_eq!(resp.id, 7);
    let value = resp.result.unwrap();
    match value {
        NexusValue::Map(entries) => {
            let rows = entries
                .iter()
                .find_map(|(k, v)| (k.as_str() == Some("rows")).then_some(v))
                .expect("rows key missing");
            match rows {
                NexusValue::Array(row_arr) => {
                    assert_eq!(row_arr.len(), 1);
                    match &row_arr[0] {
                        NexusValue::Array(cols) => assert_eq!(cols[0].as_int(), Some(1)),
                        other => panic!("expected row Array, got {other:?}"),
                    }
                }
                other => panic!("expected rows Array, got {other:?}"),
            }

            let cols = entries
                .iter()
                .find_map(|(k, v)| (k.as_str() == Some("columns")).then_some(v))
                .expect("columns key missing");
            match cols {
                NexusValue::Array(c) => assert_eq!(c[0].as_str(), Some("x")),
                other => panic!("expected columns Array, got {other:?}"),
            }
        }
        other => panic!("expected Map envelope, got {other:?}"),
    }
}

#[tokio::test]
async fn crud_round_trip_over_rpc() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    // CREATE_NODE
    write_request(
        &mut stream,
        &Request {
            id: 1,
            command: "CREATE_NODE".into(),
            args: vec![
                NexusValue::Array(vec![NexusValue::Str("Person".into())]),
                NexusValue::Map(vec![(
                    NexusValue::Str("name".into()),
                    NexusValue::Str("Alice".into()),
                )]),
            ],
        },
    )
    .await
    .unwrap();
    let created = read_response(&mut stream).await.unwrap();
    let node_id = match created.result.unwrap() {
        NexusValue::Int(id) => id,
        other => panic!("{other:?}"),
    };

    // MATCH_NODES
    write_request(
        &mut stream,
        &Request {
            id: 2,
            command: "MATCH_NODES".into(),
            args: vec![
                NexusValue::Str("Person".into()),
                NexusValue::Map(vec![]),
                NexusValue::Int(0),
            ],
        },
    )
    .await
    .unwrap();
    let matched = read_response(&mut stream).await.unwrap();
    match matched.result.unwrap() {
        NexusValue::Array(items) => assert!(!items.is_empty()),
        other => panic!("{other:?}"),
    }

    // DELETE_NODE (detach)
    write_request(
        &mut stream,
        &Request {
            id: 3,
            command: "DELETE_NODE".into(),
            args: vec![NexusValue::Int(node_id), NexusValue::Bool(true)],
        },
    )
    .await
    .unwrap();
    let deleted = read_response(&mut stream).await.unwrap();
    assert_eq!(deleted.result.unwrap(), NexusValue::Bool(true));
}

#[tokio::test]
async fn multiplexed_requests_return_with_matching_ids() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    // Fire 10 requests without waiting between them.
    for id in 1..=10u32 {
        write_request(
            &mut stream,
            &Request {
                id,
                command: "PING".into(),
                args: vec![NexusValue::Str(format!("msg-{id}"))],
            },
        )
        .await
        .unwrap();
    }
    let mut collected = Vec::new();
    for _ in 0..10 {
        collected.push(read_response(&mut stream).await.unwrap());
    }
    let mut ids: Vec<u32> = collected.iter().map(|r| r.id).collect();
    ids.sort();
    assert_eq!(ids, (1..=10u32).collect::<Vec<_>>());
    assert!(collected.iter().all(|r| r.result.is_ok()));
}

#[tokio::test]
async fn push_id_requests_are_refused_with_error() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    write_request(
        &mut stream,
        &Request {
            id: PUSH_ID,
            command: "PING".into(),
            args: vec![],
        },
    )
    .await
    .unwrap();
    let resp = read_response(&mut stream).await.unwrap();
    assert_eq!(resp.id, PUSH_ID);
    let err = resp.result.err().expect("should be Err");
    assert!(err.contains("reserved"));
}

#[tokio::test]
async fn auth_required_blocks_cypher_until_successful_auth() {
    let addr = spawn_server(true).await;
    let mut stream = connect(addr).await;

    // CYPHER before AUTH: NOAUTH.
    write_request(
        &mut stream,
        &Request {
            id: 1,
            command: "CYPHER".into(),
            args: vec![NexusValue::Str("RETURN 1".into())],
        },
    )
    .await
    .unwrap();
    let noauth = read_response(&mut stream).await.unwrap();
    let err = noauth.result.err().unwrap();
    assert!(err.starts_with("NOAUTH"), "got: {err}");

    // Root credentials should pass.
    write_request(
        &mut stream,
        &Request {
            id: 2,
            command: "AUTH".into(),
            args: vec![
                NexusValue::Str("root".into()),
                NexusValue::Str("root".into()),
            ],
        },
    )
    .await
    .unwrap();
    let ok = read_response(&mut stream).await.unwrap();
    assert_eq!(ok.result.unwrap(), NexusValue::Str("OK".into()));

    // CYPHER now succeeds.
    write_request(
        &mut stream,
        &Request {
            id: 3,
            command: "CYPHER".into(),
            args: vec![NexusValue::Str("RETURN 42 AS a".into())],
        },
    )
    .await
    .unwrap();
    let cypher_ok = read_response(&mut stream).await.unwrap();
    assert!(cypher_ok.result.is_ok());
}

#[tokio::test]
async fn stats_returns_counter_map_over_rpc() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    write_request(
        &mut stream,
        &Request {
            id: 1,
            command: "STATS".into(),
            args: vec![],
        },
    )
    .await
    .unwrap();
    let resp = read_response(&mut stream).await.unwrap();
    match resp.result.unwrap() {
        NexusValue::Map(entries) => {
            assert!(entries.iter().any(|(k, _)| k.as_str() == Some("nodes")));
        }
        other => panic!("expected Map, got {other:?}"),
    }
}

#[tokio::test]
async fn hello_returns_server_identity_and_proto_1() {
    let addr = spawn_server(false).await;
    let mut stream = connect(addr).await;

    write_request(
        &mut stream,
        &Request {
            id: 1,
            command: "HELLO".into(),
            args: vec![],
        },
    )
    .await
    .unwrap();
    let resp = read_response(&mut stream).await.unwrap();
    let map = match resp.result.unwrap() {
        NexusValue::Map(p) => p,
        other => panic!("{other:?}"),
    };
    let proto = map
        .iter()
        .find_map(|(k, v)| (k.as_str() == Some("proto")).then_some(v))
        .and_then(|v| v.as_int())
        .expect("proto missing");
    assert_eq!(proto, 1);
    let server = map
        .iter()
        .find_map(|(k, v)| (k.as_str() == Some("server")).then_some(v))
        .and_then(|v| v.as_str().map(String::from))
        .expect("server missing");
    assert_eq!(server, "nexus");
}
