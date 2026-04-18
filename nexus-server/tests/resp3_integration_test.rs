//! RESP3 end-to-end integration tests.
//!
//! Spins up a real `spawn_resp3_listener` bound to an ephemeral port,
//! connects a raw `TcpStream` to it, and exchanges byte-accurate RESP3
//! frames to exercise the parser/writer/dispatcher path end-to-end. This
//! mirrors what `redis-cli -p <port> PING` / `CYPHER ...` would do, but
//! without the external binary dependency.
//!
//! Prior-art note: these tests were the first real usage of the RESP3
//! module, so several of them are also proofs that the top-level spawn
//! wiring in `main.rs` keeps working for a freshly-booted server.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use nexus_server::NexusServer;
use nexus_server::config::RootUserConfig;
use parking_lot::RwLock;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock as TokioRwLock;

/// Build a minimal in-process Nexus server and start the RESP3 listener
/// on a loopback port of the OS's choosing. Returns the bind address so
/// each test can connect to it independently.
async fn start_test_server(auth_required: bool) -> SocketAddr {
    let ctx = nexus_core::testing::TestContext::new();
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(TokioRwLock::new(engine));
    let executor_arc = Arc::new(nexus_core::executor::Executor::default());
    let dbm_arc = Arc::new(RwLock::new(
        nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).unwrap(),
    ));
    let rbac_arc = Arc::new(TokioRwLock::new(
        nexus_core::auth::RoleBasedAccessControl::new(),
    ));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: ctx.path().join("audit"),
            retention_days: 1,
            compress_logs: false,
        })
        .unwrap(),
    );
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
        nexus_core::auth::AuthConfig::default(),
    ));
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
        nexus_core::auth::JwtConfig::default(),
    ));

    let server = Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        dbm_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        RootUserConfig::default(),
    ));

    // Leak the TestContext so the on-disk files stay valid for the life of
    // the test process. Unit tests are one-shot.
    let _leaked = Box::leak(Box::new(ctx));

    // Bind port 0 so the kernel picks an ephemeral port.
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0);
    // spawn_resp3_listener binds first, then returns — so reading the
    // concrete port needs a pre-bind + wrap-in-listener dance. Instead we
    // bind here, then let spawn_resp3_listener take over.
    //
    // Simpler path: call spawn on port 0 and then look up the bound
    // address via a side channel. We use an OS-assigned port via a
    // std TcpListener to find an available port, drop it, and pass that
    // port to spawn_resp3_listener. Race-condition risk is acceptable for
    // an in-process test harness.
    let probe = std::net::TcpListener::bind(addr).unwrap();
    let picked = probe.local_addr().unwrap();
    drop(probe);

    nexus_server::protocol::resp3::spawn_resp3_listener(server, picked, auth_required)
        .await
        .unwrap();

    // Give the listener a tick to start accepting.
    tokio::time::sleep(Duration::from_millis(50)).await;
    picked
}

/// Helper: read exactly `n` bytes from a stream, failing on timeout.
async fn read_n(stream: &mut TcpStream, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    tokio::time::timeout(Duration::from_secs(2), stream.read_exact(&mut buf))
        .await
        .expect("read timed out")
        .expect("read failed");
    buf
}

/// Read until the buffer contains the expected substring (`\r\n`-ended
/// frames are variable-length). 8 KiB cap keeps runaway tests bounded.
async fn read_until_contains(stream: &mut TcpStream, needle: &str) -> String {
    let mut acc = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut tmp = [0u8; 1024];
    while std::time::Instant::now() < deadline && acc.len() < 8192 {
        let got = tokio::time::timeout(Duration::from_millis(100), stream.read(&mut tmp)).await;
        match got {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => acc.extend_from_slice(&tmp[..n]),
            Ok(Err(_)) => break,
            Err(_) => {
                // Timeout on a single poll — check for needle and loop.
                if let Ok(s) = std::str::from_utf8(&acc) {
                    if s.contains(needle) {
                        return s.to_string();
                    }
                }
            }
        }
        if let Ok(s) = std::str::from_utf8(&acc) {
            if s.contains(needle) {
                return s.to_string();
            }
        }
    }
    String::from_utf8_lossy(&acc).into_owned()
}

// ==========================================================================
// Tests.
// ==========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_resp_array_ping_returns_pong() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    // *1\r\n$4\r\nPING\r\n
    s.write_all(b"*1\r\n$4\r\nPING\r\n").await.unwrap();
    let got = read_n(&mut s, 7).await;
    assert_eq!(&got, b"+PONG\r\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inline_ping_returns_pong() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"PING\r\n").await.unwrap();
    let got = read_n(&mut s, 7).await;
    assert_eq!(&got, b"+PONG\r\n");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hello_3_returns_map_with_proto_3() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"*2\r\n$5\r\nHELLO\r\n$1\r\n3\r\n")
        .await
        .unwrap();
    let out = read_until_contains(&mut s, "proto").await;
    assert!(
        out.starts_with('%'),
        "expected a RESP3 Map reply, got: {out:?}"
    );
    assert!(out.contains("proto"), "reply missing proto field: {out:?}");
    assert!(out.contains(":3\r\n"), "reply missing proto=3: {out:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cypher_return_1_round_trips() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    // CYPHER "RETURN 1 AS v"
    let query = b"RETURN 1 AS v";
    let cmd = format!(
        "*2\r\n$6\r\nCYPHER\r\n${}\r\n{}\r\n",
        query.len(),
        std::str::from_utf8(query).unwrap()
    );
    s.write_all(cmd.as_bytes()).await.unwrap();
    let out = read_until_contains(&mut s, "rows").await;
    // The reply is a Map; look for the `rows` key value containing :1 somewhere.
    assert!(
        out.contains("rows") && out.contains(":1"),
        "expected rows=[[1]], got: {out:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn node_create_and_node_get_round_trip() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    // NODE.CREATE Person {"name":"Alice"}
    let labels = b"Person";
    let props = br#"{"name":"Alice"}"#;
    let cmd = format!(
        "*3\r\n$11\r\nNODE.CREATE\r\n${}\r\n{}\r\n${}\r\n{}\r\n",
        labels.len(),
        std::str::from_utf8(labels).unwrap(),
        props.len(),
        std::str::from_utf8(props).unwrap()
    );
    s.write_all(cmd.as_bytes()).await.unwrap();

    // Reply: `:<id>\r\n` — e.g. `:0\r\n` or `:1\r\n`.
    let reply = read_until_contains(&mut s, "\r\n").await;
    assert!(
        reply.starts_with(':'),
        "expected Integer reply, got: {reply:?}"
    );
    let id: i64 = reply.trim_start_matches(':').trim().parse().unwrap();
    assert!(id >= 0);

    // NODE.GET <id>
    let id_str = id.to_string();
    let cmd = format!(
        "*2\r\n$8\r\nNODE.GET\r\n${}\r\n{}\r\n",
        id_str.len(),
        id_str
    );
    s.write_all(cmd.as_bytes()).await.unwrap();
    let out = read_until_contains(&mut s, "\r\n").await;
    assert!(
        out.starts_with('%') || out.starts_with('_') || out.starts_with('$'),
        "expected Map/Null/Bulk reply to NODE.GET, got: {out:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_command_returns_err() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$1\r\nv\r\n")
        .await
        .unwrap();
    let out = read_until_contains(&mut s, "unknown").await;
    assert!(
        out.starts_with('-'),
        "expected error reply to unknown cmd, got: {out:?}"
    );
    assert!(
        out.contains("ERR unknown command 'SET'"),
        "error text should mention the rejected command verbatim: {out:?}"
    );
    assert!(
        out.contains("Nexus is a graph DB"),
        "error should steer the user toward the Nexus vocabulary: {out:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn noauth_rejected_until_auth_sent() {
    let addr = start_test_server(true).await;
    let mut s = TcpStream::connect(addr).await.unwrap();

    // Pre-auth PING is allowed.
    s.write_all(b"*1\r\n$4\r\nPING\r\n").await.unwrap();
    let got = read_n(&mut s, 7).await;
    assert_eq!(&got, b"+PONG\r\n");

    // Pre-auth CYPHER is refused with -NOAUTH.
    let query = b"RETURN 1 AS v";
    let cmd = format!(
        "*2\r\n$6\r\nCYPHER\r\n${}\r\n{}\r\n",
        query.len(),
        std::str::from_utf8(query).unwrap()
    );
    s.write_all(cmd.as_bytes()).await.unwrap();
    let out = read_until_contains(&mut s, "NOAUTH").await;
    assert!(
        out.contains("NOAUTH"),
        "expected NOAUTH error on pre-auth CYPHER, got: {out:?}"
    );

    // Now authenticate as root/root (the default config).
    let auth_cmd = b"*3\r\n$4\r\nAUTH\r\n$4\r\nroot\r\n$4\r\nroot\r\n";
    s.write_all(auth_cmd).await.unwrap();
    let ok = read_n(&mut s, 5).await;
    assert_eq!(&ok, b"+OK\r\n");

    // After AUTH, CYPHER proceeds.
    s.write_all(cmd.as_bytes()).await.unwrap();
    let out = read_until_contains(&mut s, "rows").await;
    assert!(out.contains("rows"), "post-auth CYPHER reply: {out:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn quit_closes_connection_cleanly() {
    let addr = start_test_server(false).await;
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(b"*1\r\n$4\r\nQUIT\r\n").await.unwrap();
    // Expect +OK\r\n then EOF.
    let ok = read_n(&mut s, 5).await;
    assert_eq!(&ok, b"+OK\r\n");

    // Subsequent read should return 0 bytes (clean EOF).
    let mut buf = [0u8; 16];
    let n = tokio::time::timeout(Duration::from_secs(2), s.read(&mut buf))
        .await
        .expect("post-QUIT read timed out")
        .expect("post-QUIT read failed");
    assert_eq!(n, 0, "connection should be closed after QUIT");
}
