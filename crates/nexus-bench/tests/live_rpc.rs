//! Integration tests that hit a live Nexus server over the native
//! RPC protocol. No HTTP client is involved.
//!
//! **Every test here is `#[ignore]` by default.** `cargo test -p
//! nexus-bench` passes without touching the network. To run them:
//!
//! ```bash
//! NEXUS_BENCH_RPC_ADDR=127.0.0.1:15475 \
//!     cargo test -p nexus-bench --features live-bench -- --ignored
//! ```
//!
//! Each test probes HELLO + PING at the address and bails out
//! cleanly if the server isn't reachable, so the worst that happens
//! when someone runs these without a server is a clean error —
//! never a hang.
//!
//! State isolation: every test calls `common::reset_single` right
//! after the connect handshake so two tests can run back-to-back
//! against the same Nexus server without the second one tripping
//! the row-count divergence guard on the previous iteration's
//! residual data.

#![cfg(feature = "live-bench")]

use std::time::Duration;

use nexus_bench::{
    Dataset,
    client::{BenchClient, NexusRpcClient},
    harness::{RunConfig, run_scenario},
    scenario::ScenarioBuilder,
    scenario_catalog::seed_scenarios,
};

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn health_probe_succeeds() {
    let Some(addr) = common::nexus_rpc_addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let client = NexusRpcClient::connect(addr, common::nexus_rpc_credentials(), "nexus", rt).await;
    assert!(
        client.is_ok(),
        "HELLO + PING probe failed: {:?}",
        client.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn scalar_one_shot_returns_single_row() {
    let Some(addr) = common::nexus_rpc_addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let mut client = NexusRpcClient::connect(addr, common::nexus_rpc_credentials(), "nexus", rt)
        .await
        .expect("connect");
    // Fresh state per test — this scenario does not load data but
    // resetting keeps the suite order-independent.
    common::reset_single(&mut client);

    let scen = ScenarioBuilder::new(
        "integration.scalar",
        "",
        nexus_bench::dataset::DatasetKind::Tiny,
        "RETURN 1 AS n",
    )
    .warmup(1)
    .measured(3)
    .expected_rows(1)
    .timeout(Duration::from_secs(2))
    .build();

    let mut c = &mut client;
    let result = run_scenario(&scen, "nexus", &mut c, &RunConfig::default()).unwrap();
    assert!(result.samples_us.len() >= 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn seed_catalog_run_completes() {
    // Smoke that the whole seed catalogue finishes in a bounded
    // time against a live server. The scenarios all target the
    // tiny dataset, which fits in one CREATE — reset + load on
    // entry, then iterate.
    let Some(addr) = common::nexus_rpc_addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let mut client = NexusRpcClient::connect(addr, common::nexus_rpc_credentials(), "nexus", rt)
        .await
        .expect("connect");

    common::reset_single(&mut client);

    let scenarios = seed_scenarios();
    // Load every dataset kind the catalogue references — TinyDataset
    // + SmallDataset + whatever future fixture shows up.
    let kinds: std::collections::HashSet<nexus_bench::dataset::DatasetKind> =
        scenarios.iter().map(|s| s.dataset).collect();
    let timeout = Duration::from_secs(30);
    for kind in kinds {
        let load = match kind {
            nexus_bench::dataset::DatasetKind::Tiny => {
                nexus_bench::dataset::TinyDataset.load_statement()
            }
            nexus_bench::dataset::DatasetKind::Small => nexus_bench::SmallDataset.load_statement(),
        };
        client
            .execute(load, timeout)
            .unwrap_or_else(|e| panic!("load {kind:?}: {e}"));
    }

    let cfg = RunConfig::default().clamped();
    let mut ran = 0;
    for scen in &scenarios {
        let mut c = &mut client;
        let _ = run_scenario(scen, "nexus", &mut c, &cfg).unwrap();
        ran += 1;
    }
    assert!(ran >= 5);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn isolation_between_loads_works() {
    // Load the tiny dataset, count nodes, reset, load again, count
    // again. Both counts must be 100 — if the reset hook is a
    // no-op, the second count comes back as 200 and trips.
    let Some(addr) = common::nexus_rpc_addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let mut client = NexusRpcClient::connect(addr, common::nexus_rpc_credentials(), "nexus", rt)
        .await
        .expect("connect");

    let load = nexus_bench::dataset::TinyDataset.load_statement();
    let timeout = Duration::from_secs(30);

    for pass in 1..=2 {
        common::reset_single(&mut client);
        // Reset contract (phase6_nexus-delete-executor-bug) —
        // post-reset count must be zero.
        let pre = client
            .execute("MATCH (n) RETURN count(n) AS c", timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: pre-load count failed: {e}"));
        assert_eq!(
            pre.rows,
            vec![vec![serde_json::json!(0)]],
            "pass {pass}: reset did not clear — DELETE regression?"
        );
        client
            .execute(load, timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: load failed: {e}"));
        // Load contract (phase6_nexus-create-bound-var-duplication) —
        // TinyDataset produces exactly 100 nodes + 50 edges; if
        // either number drifts the bound-variable binding in CREATE
        // regressed and the edge section duplicated the declared
        // nodes.
        let post_n = client
            .execute("MATCH (n) RETURN count(n) AS c", timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: post-load node count failed: {e}"));
        assert_eq!(
            post_n.rows,
            vec![vec![serde_json::json!(100)]],
            "pass {pass}: expected 100 nodes after load"
        );
        let post_r = client
            .execute("MATCH ()-[r]->() RETURN count(r) AS c", timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: post-load rel count failed: {e}"));
        assert_eq!(
            post_r.rows,
            vec![vec![serde_json::json!(50)]],
            "pass {pass}: expected 50 relationships after load"
        );
    }
}
