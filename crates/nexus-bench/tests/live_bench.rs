//! Integration tests that hit a live server over HTTP.
//!
//! **Every test here is `#[ignore]` by default.** `cargo test -p
//! nexus-bench` passes without touching the network. To run them:
//!
//! ```bash
//! NEXUS_BENCH_URL=http://127.0.0.1:15474 \
//!     cargo test -p nexus-bench --features live-bench -- --ignored
//! ```
//!
//! Each test probes `/health` at the URL and bails out cleanly if the
//! server isn't reachable, so the worst that happens when someone
//! runs these without a server is a clean error — never a hang.

#![cfg(feature = "live-bench")]

use std::time::Duration;

use nexus_bench::{
    Dataset,
    client::{BenchClient, HttpClient},
    harness::{RunConfig, run_scenario},
    scenario::ScenarioBuilder,
    scenario_catalog::seed_scenarios,
};

fn url() -> Option<String> {
    std::env::var("NEXUS_BENCH_URL").ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus server reachable at NEXUS_BENCH_URL"]
async fn health_probe_succeeds() {
    let url = url().expect("set NEXUS_BENCH_URL");
    let rt = tokio::runtime::Handle::current();
    let client = HttpClient::connect(url, "nexus", rt).await;
    assert!(client.is_ok(), "health probe failed: {:?}", client.err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires a running Nexus server reachable at NEXUS_BENCH_URL"]
async fn scalar_one_shot_returns_single_row() {
    let url = url().expect("set NEXUS_BENCH_URL");
    let rt = tokio::runtime::Handle::current();
    let mut client = HttpClient::connect(url, "nexus", rt).await.unwrap();

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
#[ignore = "requires a running Nexus server reachable at NEXUS_BENCH_URL"]
async fn seed_catalog_run_completes() {
    // Smoke that the whole seed catalogue finishes in a bounded
    // time against a live server. The scenarios all target the
    // tiny dataset, which fits in one CREATE — load the dataset
    // on entry via `BenchClient::execute`, then iterate.
    let url = url().expect("set NEXUS_BENCH_URL");
    let rt = tokio::runtime::Handle::current();
    let mut client = HttpClient::connect(url, "nexus", rt).await.unwrap();

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
