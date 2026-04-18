//! Binary RPC TCP listener.
//!
//! Each accepted connection spawns a dedicated read loop and a dedicated
//! writer task. The writer owns the socket's write half behind an mpsc
//! channel so responses can be produced out of order by concurrent per-
//! request tasks and still serialise correctly on the wire. In-flight
//! requests per connection are capped with a semaphore so a misbehaving
//! client cannot unboundedly queue work.
//!
//! Frame id [`crate::protocol::rpc::PUSH_ID`] (`u32::MAX`) is reserved for
//! server-initiated push frames; clients that use it for their own
//! requests get a dedicated error back so the push demultiplexing
//! invariant stays unambiguous.
//!
//! Metrics recorded per request:
//!
//! - `nexus_rpc_connections` (gauge; open/close)
//! - `nexus_rpc_commands_total{command, status}` (counter)
//! - `nexus_rpc_command_duration_seconds{command}` (histogram)
//! - `nexus_rpc_frame_size_bytes_in` / `..._out` (histograms)

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use tokio::io::BufReader;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Semaphore, mpsc};

use nexus_protocol::rpc::{
    PUSH_ID, Request, Response, encode_frame, read_request_with_limit, write_response,
};

use super::dispatch::{RpcSession, dispatch};
use crate::NexusServer;
use crate::config::RpcConfig;

/// Monotonic per-listener connection id generator. Only used for logging;
/// the wire id is a `u64` so overflow wraps in ~584 years at 1 ns
/// resolution — we don't try to special-case that.
static CONNECTION_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Spawn the binary RPC listener. Returns after the listener is bound;
/// the accept loop continues to run as a detached task so `main` can wire
/// it up alongside the HTTP server.
pub async fn spawn_rpc_listener(
    server: Arc<NexusServer>,
    addr: SocketAddr,
    config: RpcConfig,
    auth_required: bool,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    tracing::info!("Nexus RPC listening on {addr}");

    let max_frame = config.max_frame_bytes;
    let max_in_flight = config.max_in_flight_per_conn;
    let slow_ms = config.slow_threshold_ms;

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, peer)) => {
                    record_connection_open();
                    let conn_id = CONNECTION_COUNTER.fetch_add(1, Ordering::Relaxed);
                    tracing::debug!(peer = %peer, conn_id, "RPC connection accepted");
                    let server = Arc::clone(&server);
                    tokio::spawn(async move {
                        let span = tracing::info_span!("rpc.conn", peer = %peer, id = conn_id);
                        let _guard = span.enter();
                        if let Err(e) = handle_connection(
                            stream,
                            server,
                            conn_id,
                            max_frame,
                            max_in_flight,
                            slow_ms,
                            auth_required,
                        )
                        .await
                        {
                            tracing::debug!(peer = %peer, conn_id, error = %e, "RPC connection error");
                        }
                        record_connection_close();
                        tracing::debug!(peer = %peer, conn_id, "RPC connection closed");
                    });
                }
                Err(e) => {
                    tracing::error!(error = %e, "RPC accept error");
                }
            }
        }
    });

    Ok(())
}

async fn handle_connection(
    stream: TcpStream,
    server: Arc<NexusServer>,
    conn_id: u64,
    max_frame_bytes: usize,
    max_in_flight: usize,
    slow_threshold_ms: u64,
    auth_required: bool,
) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;
    let (read_half, write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    // Per-connection writer. Dispatch tasks hand it
    // `(response, command, elapsed_secs, in_bytes)` so metrics are
    // recorded after a successful write (the counter definition treats
    // a response as delivered only once it leaves the socket).
    let (tx, mut rx) = mpsc::channel::<(Response, String, f64, usize)>(64);
    let mut writer = write_half;
    let writer_task = tokio::spawn(async move {
        while let Some((response, command, elapsed, in_bytes)) = rx.recv().await {
            let is_err = response.result.is_err();
            match encode_frame(&response) {
                Ok(frame) => {
                    let out_bytes = frame.len();
                    if let Err(e) = write_response(&mut writer, &response).await {
                        tracing::debug!(error = %e, "RPC write error");
                        break;
                    }
                    record_command(&command, !is_err, elapsed);
                    record_frame_sizes(in_bytes, out_bytes);

                    let elapsed_ms = elapsed * 1_000.0;
                    if elapsed_ms > slow_threshold_ms as f64 {
                        tracing::warn!(
                            cmd = %command,
                            elapsed_ms,
                            threshold_ms = slow_threshold_ms,
                            "RPC slow command"
                        );
                    } else {
                        tracing::debug!(
                            cmd = %command,
                            elapsed_us = elapsed * 1_000_000.0,
                            ok = !is_err,
                            "RPC command"
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(cmd = %command, error = %e, "RPC encode error");
                }
            }
        }
    });

    // Per-connection session state — the auth flag is shared with every
    // request task via `Arc` so a successful AUTH is immediately visible
    // to the next frame on the same connection.
    let session = Arc::new(RpcSession {
        server: Arc::clone(&server),
        authenticated: Arc::new(AtomicBool::new(false)),
        auth_required,
        connection_id: conn_id,
    });

    // Semaphore caps the number of concurrent dispatch tasks per
    // connection. `Arc<Semaphore>` so acquire permits outlive the read
    // loop iteration that spawned them.
    let in_flight = Arc::new(Semaphore::new(max_in_flight));

    loop {
        let req = match read_request_with_limit(&mut reader, max_frame_bytes).await {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                tracing::debug!(peer = %peer, error = %e, "RPC read error");
                break;
            }
        };

        if req.id == PUSH_ID {
            let _ = tx
                .send((
                    Response::err(
                        req.id,
                        "ERR request id u32::MAX is reserved for server push frames",
                    ),
                    req.command.clone(),
                    0.0,
                    encoded_request_size(&req),
                ))
                .await;
            continue;
        }

        let permit = match Arc::clone(&in_flight).acquire_owned().await {
            Ok(p) => p,
            Err(_) => break, // Semaphore closed — connection is shutting down.
        };

        let in_bytes = encoded_request_size(&req);
        let command = req.command.clone();
        let session = Arc::clone(&session);
        let tx_dispatch = tx.clone();

        tokio::spawn(async move {
            let start = Instant::now();
            let span = tracing::debug_span!("rpc.req", id = req.id, cmd = %req.command);
            let response = {
                let _g = span.enter();
                dispatch(&session, req).await
            };
            let elapsed = start.elapsed().as_secs_f64();
            let _ = tx_dispatch
                .send((response, command, elapsed, in_bytes))
                .await;
            drop(permit);
        });
    }

    drop(tx);
    let _ = writer_task.await;
    Ok(())
}

/// Estimate the byte size of an incoming request by re-encoding it. We
/// could track this through the decoder but it would complicate the
/// codec API for a metric-only use case.
fn encoded_request_size(req: &Request) -> usize {
    // 4-byte length prefix + MessagePack body.
    rmp_serde::to_vec(req).map(|b| b.len() + 4).unwrap_or(0)
}

// ── Metrics hooks ────────────────────────────────────────────────────────────

use super::metrics;

fn record_connection_open() {
    metrics::rpc_connection_open();
}

fn record_connection_close() {
    metrics::rpc_connection_close();
}

fn record_command(cmd: &str, ok: bool, elapsed_secs: f64) {
    metrics::record_rpc_command(cmd, ok, elapsed_secs);
}

fn record_frame_sizes(in_bytes: usize, out_bytes: usize) {
    metrics::record_rpc_frame_sizes(in_bytes, out_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_protocol::rpc::{NexusValue, read_response, write_request};
    use std::time::Duration;
    use tokio::net::TcpStream;

    async fn spawn_test_server() -> (SocketAddr, Arc<NexusServer>) {
        let ctx = nexus_core::testing::TestContext::new();
        let engine =
            nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init for rpc server test");
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
        let server = Arc::new(crate::NexusServer::new(
            executor_arc,
            engine_arc,
            dbm_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            crate::config::RootUserConfig::default(),
        ));
        let _leaked = Box::leak(Box::new(ctx));

        // Bind on an OS-picked port so parallel test runs never collide.
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let listener = TcpListener::bind(addr).await.unwrap();
        let actual = listener.local_addr().unwrap();
        drop(listener); // we only needed it for its port choice

        spawn_rpc_listener(Arc::clone(&server), actual, RpcConfig::default(), false)
            .await
            .unwrap();

        (actual, server)
    }

    async fn connect(addr: SocketAddr) -> TcpStream {
        for _ in 0..20 {
            if let Ok(s) = TcpStream::connect(addr).await {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("could not connect to {addr}");
    }

    #[tokio::test]
    async fn ping_round_trips_over_tcp() {
        let (addr, _server) = spawn_test_server().await;
        let mut stream = connect(addr).await;
        let req = Request {
            id: 1,
            command: "PING".into(),
            args: vec![],
        };
        write_request(&mut stream, &req).await.unwrap();
        let resp = read_response(&mut stream).await.unwrap();
        assert_eq!(resp.id, 1);
        assert_eq!(resp.result.unwrap(), NexusValue::Str("PONG".into()));
    }

    #[tokio::test]
    async fn multiplexed_requests_share_one_connection() {
        let (addr, _server) = spawn_test_server().await;
        let mut stream = connect(addr).await;

        for id in 1..=5 {
            let req = Request {
                id,
                command: "PING".into(),
                args: vec![NexusValue::Str(format!("msg-{id}"))],
            };
            write_request(&mut stream, &req).await.unwrap();
        }
        let mut ids = Vec::new();
        for _ in 0..5 {
            let resp = read_response(&mut stream).await.unwrap();
            ids.push(resp.id);
            assert!(resp.result.is_ok());
        }
        ids.sort();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn push_id_requests_are_refused() {
        let (addr, _server) = spawn_test_server().await;
        let mut stream = connect(addr).await;
        let req = Request {
            id: PUSH_ID,
            command: "PING".into(),
            args: vec![],
        };
        write_request(&mut stream, &req).await.unwrap();
        let resp = read_response(&mut stream).await.unwrap();
        assert_eq!(resp.id, PUSH_ID);
        let msg = resp.result.err().expect("should be an error");
        assert!(msg.contains("reserved"), "got: {msg}");
    }

    #[tokio::test]
    async fn unknown_command_returns_error_not_disconnection() {
        let (addr, _server) = spawn_test_server().await;
        let mut stream = connect(addr).await;
        let req = Request {
            id: 99,
            command: "BOGUS".into(),
            args: vec![],
        };
        write_request(&mut stream, &req).await.unwrap();
        let resp = read_response(&mut stream).await.unwrap();
        assert_eq!(resp.id, 99);
        assert!(resp.result.err().unwrap().contains("unknown command"));

        // Connection is still usable after an error.
        let ok_req = Request {
            id: 100,
            command: "PING".into(),
            args: vec![],
        };
        write_request(&mut stream, &ok_req).await.unwrap();
        let resp = read_response(&mut stream).await.unwrap();
        assert_eq!(resp.id, 100);
        assert!(resp.result.is_ok());
    }

    #[tokio::test]
    async fn auth_required_rejects_cypher_until_auth() {
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
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
        let server = Arc::new(crate::NexusServer::new(
            executor_arc,
            engine_arc,
            dbm_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            crate::config::RootUserConfig::default(),
        ));
        let _leaked = Box::leak(Box::new(ctx));

        // Pick a port and launch with auth_required = true.
        let bound = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = bound.local_addr().unwrap();
        drop(bound);

        spawn_rpc_listener(Arc::clone(&server), addr, RpcConfig::default(), true)
            .await
            .unwrap();

        let mut stream = connect(addr).await;
        let blocked = Request {
            id: 1,
            command: "CYPHER".into(),
            args: vec![NexusValue::Str("RETURN 1".into())],
        };
        write_request(&mut stream, &blocked).await.unwrap();
        let resp = read_response(&mut stream).await.unwrap();
        assert!(resp.result.err().unwrap().starts_with("NOAUTH"));

        // AUTH with root/root, then CYPHER succeeds.
        let auth_req = Request {
            id: 2,
            command: "AUTH".into(),
            args: vec![
                NexusValue::Str("root".into()),
                NexusValue::Str("root".into()),
            ],
        };
        write_request(&mut stream, &auth_req).await.unwrap();
        let auth_resp = read_response(&mut stream).await.unwrap();
        assert_eq!(auth_resp.result.unwrap(), NexusValue::Str("OK".into()));

        let again = Request {
            id: 3,
            command: "PING".into(),
            args: vec![],
        };
        write_request(&mut stream, &again).await.unwrap();
        let ok = read_response(&mut stream).await.unwrap();
        assert_eq!(ok.id, 3);
        assert!(ok.result.is_ok());
    }
}
