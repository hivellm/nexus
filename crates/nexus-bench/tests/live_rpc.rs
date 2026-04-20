//! Integration tests that hit a live Nexus server over the native
//! RPC protocol. No HTTP client is involved.
//!
//! **Every test here is `#[ignore]` by default.** `cargo test -p
//! nexus-bench` passes without touching the network. To run them:
//!
//! ```bash
//! NEXUS_BENCH_RPC_ADDR=127.0.0.1:7878 \
//!     cargo test -p nexus-bench --features live-bench -- --ignored
//! ```
//!
//! Each test probes HELLO + PING at the address and bails out
//! cleanly if the server isn't reachable, so the worst that happens
//! when someone runs these without a server is a clean error —
//! never a hang.

#![cfg(feature = "live-bench")]

use std::time::Duration;

use nexus_bench::{
    Dataset,
    client::{BenchClient, NexusRpcClient, NexusRpcCredentials},
    harness::{RunConfig, run_scenario},
    scenario::ScenarioBuilder,
    scenario_catalog::seed_scenarios,
};

fn addr() -> Option<String> {
    std::env::var("NEXUS_BENCH_RPC_ADDR").ok()
}

fn credentials() -> NexusRpcCredentials {
    NexusRpcCredentials {
        api_key: std::env::var("NEXUS_BENCH_API_KEY").ok(),
        username: std::env::var("NEXUS_BENCH_USER").ok(),
        password: std::env::var("NEXUS_BENCH_PASSWORD").ok(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn health_probe_succeeds() {
    let Some(addr) = addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let client = NexusRpcClient::connect(addr, credentials(), "nexus", rt).await;
    assert!(
        client.is_ok(),
        "HELLO + PING probe failed: {:?}",
        client.err()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus RPC listener reachable at NEXUS_BENCH_RPC_ADDR"]
async fn scalar_one_shot_returns_single_row() {
    let Some(addr) = addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let mut client = NexusRpcClient::connect(addr, credentials(), "nexus", rt)
        .await
        .expect("connect");

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
    // tiny dataset, which fits in one CREATE — load the dataset
    // on entry via `BenchClient::execute`, then iterate.
    let Some(addr) = addr() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR not set");
        return;
    };
    let rt = tokio::runtime::Handle::current();
    let mut client = NexusRpcClient::connect(addr, credentials(), "nexus", rt)
        .await
        .expect("connect");

    // Load tiny dataset — single CREATE statement.
    let load = nexus_bench::dataset::TinyDataset.load_statement();
    client.execute(load, Duration::from_secs(30)).unwrap();

    let cfg = RunConfig::default().clamped();
    let mut ran = 0;
    for scen in seed_scenarios() {
        let mut c = &mut client;
        let _ = run_scenario(&scen, "nexus", &mut c, &cfg).unwrap();
        ran += 1;
    }
    assert!(ran >= 5);
}
