//! Integration tests for phase5_lock-free-read-path.
//!
//! Covers the snapshot-freshness guarantee (a committed write must be
//! visible to the very next autocommit read, which now runs through the
//! lock-free `Executor` clone instead of `engine.write().await`),
//! read-your-own-writes inside an explicit transaction (unchanged —
//! still routes through the engine), committed-write visibility across
//! independent "connections" (simulated as separate concurrent calls
//! against the same `Arc<NexusServer>`, since the HTTP transport itself
//! is connectionless), and a minimal concurrent-throughput benchmark
//! (`#[ignore]`d — run on demand with `cargo test --test
//! lock_free_read_path_test -- --ignored --nocapture`).

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Json, State};
use nexus_server::NexusServer;
use nexus_server::api::cypher::{CypherRequest, CypherResponse, execute_cypher};
use nexus_server::config::RootUserConfig;

/// Build a fresh, isolated `NexusServer` backed by a temp data dir.
/// Mirrors the construction pattern shared by
/// `api::cypher::write_path_parity` and `tests/rpc_integration_test.rs`.
fn build_test_server(ctx: &nexus_core::testing::TestContext) -> Arc<NexusServer> {
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).expect("engine init");
    let engine_arc = Arc::new(tokio::sync::RwLock::new(engine));
    let executor_arc = Arc::new(nexus_core::executor::Executor::default());
    let dbm_arc = Arc::new(parking_lot::RwLock::new(
        nexus_core::database::DatabaseManager::new(ctx.path().join("databases")).expect("dbm init"),
    ));
    let rbac_arc = Arc::new(tokio::sync::RwLock::new(
        nexus_core::auth::RoleBasedAccessControl::new(),
    ));
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(
        nexus_core::auth::AuthConfig::default(),
    ));
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(
        nexus_core::auth::JwtConfig::default(),
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
    Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        dbm_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        RootUserConfig::default(),
    ))
}

async fn run_query(
    server: &Arc<NexusServer>,
    query: &str,
    params: HashMap<String, serde_json::Value>,
) -> CypherResponse {
    execute_cypher(
        State(server.clone()),
        None,
        Json(CypherRequest {
            query: query.to_string(),
            params,
            database: None,
        }),
    )
    .await
    .0
}

fn no_params() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}

fn count_column(resp: &CypherResponse) -> i64 {
    resp.rows
        .first()
        .and_then(|row| row.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| panic!("expected a single count row/column, got {:?}", resp.rows))
}

/// Snapshot-freshness gate (tasks.md §3.3 / §4.1): a write committed via
/// the engine's autocommit write path must be visible to the very next
/// read, even though that read now runs through the lock-free
/// `Executor` clone instead of `engine.write().await`.
/// `Engine::refresh_executor()` replaces `Engine::executor` with a
/// fresh snapshot before the write's `engine.write().await` guard is
/// released, and this read only clones `Engine::executor` after
/// acquiring its own (later) `engine.read().await` — so the clone it
/// gets is guaranteed to postdate the write.
#[tokio::test]
async fn lock_free_read_sees_immediately_preceding_committed_write() {
    let ctx = nexus_core::testing::TestContext::new();
    let server = build_test_server(&ctx);

    let create = run_query(&server, "CREATE (n:FreshnessProbe {x: 1})", no_params()).await;
    assert!(create.error.is_none(), "CREATE failed: {:?}", create.error);

    // This MATCH is a pure autocommit read with no open transaction —
    // it routes through the new lock-free path (`routing::is_read_only`
    // + no active "default"-session transaction).
    let read = run_query(
        &server,
        "MATCH (n:FreshnessProbe) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert!(read.error.is_none(), "MATCH failed: {:?}", read.error);
    assert_eq!(
        count_column(&read),
        1,
        "the lock-free read path must see a write committed immediately before it"
    );
}

/// Read-your-own-writes inside an explicit transaction must stay
/// unchanged: a MATCH issued while a `BEGIN TRANSACTION` is still open
/// on the "default" session must still route through the engine (not
/// the lock-free path), so it observes the transaction's own
/// not-yet-committed write.
#[tokio::test]
async fn read_your_own_write_inside_open_transaction_is_unchanged() {
    let ctx = nexus_core::testing::TestContext::new();
    let server = build_test_server(&ctx);

    let begin = run_query(&server, "BEGIN TRANSACTION", no_params()).await;
    assert!(begin.error.is_none(), "BEGIN failed: {:?}", begin.error);

    let create = run_query(&server, "CREATE (n:InTxProbe {x: 1})", no_params()).await;
    assert!(
        create.error.is_none(),
        "CREATE inside open tx failed: {:?}",
        create.error
    );

    // Still inside the SAME open transaction — must see the just-created
    // node (`routing::is_read_only` is true for this MATCH, but the
    // "default" session's `has_active_transaction()` is true, so it must
    // fall through to the engine-locked path, not the lock-free clone).
    let read = run_query(
        &server,
        "MATCH (n:InTxProbe) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert!(read.error.is_none(), "MATCH failed: {:?}", read.error);
    assert_eq!(
        count_column(&read),
        1,
        "a read inside an open explicit transaction must see that transaction's own write \
         (read-your-own-writes) — this must be unchanged by the lock-free read-path routing"
    );

    let commit = run_query(&server, "COMMIT TRANSACTION", no_params()).await;
    assert!(commit.error.is_none(), "COMMIT failed: {:?}", commit.error);

    // After COMMIT, the same MATCH now runs through the lock-free path
    // (no more open transaction) and must still see the committed node.
    let read_after_commit = run_query(
        &server,
        "MATCH (n:InTxProbe) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert!(
        read_after_commit.error.is_none(),
        "MATCH after COMMIT failed: {:?}",
        read_after_commit.error
    );
    assert_eq!(count_column(&read_after_commit), 1);
}

/// Committed-write visibility across independent "connections". The
/// `/cypher` HTTP transport is connectionless — every request is an
/// independent call against the shared `Arc<NexusServer>` with no
/// per-connection session state — so two concurrently-issued requests
/// (simulated here as two `tokio::spawn`ed tasks) already model two
/// distinct client connections. The second task's read (spawned only
/// after the first task's write has completed) must observe the
/// committed data through the lock-free path.
#[tokio::test]
async fn committed_write_visible_to_a_separate_concurrent_connection() {
    let ctx = nexus_core::testing::TestContext::new();
    let server = build_test_server(&ctx);

    let writer_server = server.clone();
    let write_resp = tokio::spawn(async move {
        run_query(
            &writer_server,
            "CREATE (n:CrossConnProbe {x: 1})",
            no_params(),
        )
        .await
    })
    .await
    .expect("writer task panicked");
    assert!(
        write_resp.error.is_none(),
        "CREATE failed: {:?}",
        write_resp.error
    );

    let reader_server = server.clone();
    let read_resp = tokio::spawn(async move {
        run_query(
            &reader_server,
            "MATCH (n:CrossConnProbe) RETURN count(n) AS c",
            no_params(),
        )
        .await
    })
    .await
    .expect("reader task panicked");
    assert!(
        read_resp.error.is_none(),
        "MATCH failed: {:?}",
        read_resp.error
    );
    assert_eq!(
        count_column(&read_resp),
        1,
        "a separate concurrent connection must see a write committed by another connection"
    );
}

/// Minimal concurrent-throughput benchmark (tasks.md §1.1 / §4.2):
/// N parallel simulated clients repeatedly running a MATCH-by-label
/// workload against the same `Arc<NexusServer>`, reporting aggregate
/// QPS. `#[ignore]`d — this is a perf measurement, not a correctness
/// gate, and its absolute numbers are machine-dependent. Run on demand:
///
/// ```text
/// cargo +nightly test --release -p nexus-server --test lock_free_read_path_test \
///     concurrent_match_throughput -- --ignored --nocapture
/// ```
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore = "perf measurement — run on demand, not part of the correctness gate"]
async fn concurrent_match_throughput() {
    const SEED_NODES: usize = 500;
    const CLIENTS: usize = 8;
    const QUERIES_PER_CLIENT: usize = 200;

    let ctx = nexus_core::testing::TestContext::new();
    let server = build_test_server(&ctx);

    // Seed data once, sequentially, before measuring.
    for i in 0..SEED_NODES {
        let resp = run_query(&server, "CREATE (n:ThroughputProbe {i: $i})", {
            let mut p = HashMap::new();
            p.insert("i".to_string(), serde_json::json!(i));
            p
        })
        .await;
        assert!(resp.error.is_none(), "seed CREATE failed: {:?}", resp.error);
    }

    let started = std::time::Instant::now();
    let mut handles = Vec::with_capacity(CLIENTS);
    for _ in 0..CLIENTS {
        let server = server.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..QUERIES_PER_CLIENT {
                let resp = run_query(
                    &server,
                    "MATCH (n:ThroughputProbe) RETURN count(n) AS c",
                    no_params(),
                )
                .await;
                assert!(resp.error.is_none(), "MATCH failed: {:?}", resp.error);
            }
        }));
    }
    for h in handles {
        h.await.expect("client task panicked");
    }
    let elapsed = started.elapsed();
    let total_queries = (CLIENTS * QUERIES_PER_CLIENT) as f64;
    let qps = total_queries / elapsed.as_secs_f64();

    println!(
        "concurrent_match_throughput: {} clients x {} queries = {} total in {:?} -> {:.1} qps",
        CLIENTS,
        QUERIES_PER_CLIENT,
        CLIENTS * QUERIES_PER_CLIENT,
        elapsed,
        qps
    );
}
