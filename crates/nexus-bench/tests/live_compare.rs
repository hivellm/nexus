//! Comparative integration tests — hit a live Nexus RPC listener +
//! a live Neo4j Bolt listener and verify both sides of the bench
//! harness run to completion with consistent shapes. Both transports
//! are binary; HTTP is intentionally not involved on either side.
//!
//! **Every test here is `#[ignore]` by default.** `cargo test -p
//! nexus-bench` passes without touching the network. To run them,
//! point the harness at both engines and pass `-- --ignored`:
//!
//! ```bash
//! NEXUS_BENCH_RPC_ADDR=127.0.0.1:15475 \
//! NEO4J_BENCH_URL=bolt://127.0.0.1:17687 \
//!     cargo test -p nexus-bench --features live-bench,neo4j -- --ignored
//! ```
//!
//! Each test skips cleanly (not `unwrap`) if either env var is
//! missing, so arming only one engine still lets `--ignored` run
//! the single-engine tests in `live_rpc.rs` without side effects
//! here.
//!
//! State isolation: every test calls `common::reset_both` right
//! after the connect handshake so the whole suite can run as a
//! single `cargo test --ignored` pass without the second test
//! duplicating the previous one's TinyDataset load.

#![cfg(feature = "neo4j")]

use std::time::Duration;

use nexus_bench::{
    ComparativeRow, Dataset,
    client::{BenchClient, Neo4jBoltClient, NexusRpcClient},
    harness::{RunConfig, run_scenario},
    scenario::ScenarioBuilder,
    scenario_catalog::seed_scenarios,
};

mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both NEXUS_BENCH_RPC_ADDR and NEO4J_BENCH_URL to be set"]
async fn both_health_probes_succeed() {
    let Some((nexus_addr, neo4j_url)) = common::both_endpoints() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR / NEO4J_BENCH_URL not set");
        return;
    };
    let (user, password) = common::bolt_credentials();
    let rt = tokio::runtime::Handle::current();

    let nexus = NexusRpcClient::connect(
        nexus_addr,
        common::nexus_rpc_credentials(),
        "nexus",
        rt.clone(),
    )
    .await;
    assert!(
        nexus.is_ok(),
        "nexus RPC HELLO/PING probe failed: {:?}",
        nexus.err()
    );

    let neo4j = Neo4jBoltClient::connect(neo4j_url, user, password, "neo4j", rt).await;
    assert!(neo4j.is_ok(), "neo4j bolt probe failed: {:?}", neo4j.err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both NEXUS_BENCH_RPC_ADDR and NEO4J_BENCH_URL to be set"]
async fn both_engines_accept_tiny_dataset() {
    // The tiny dataset is a single CREATE literal — both engines
    // must parse + apply it without error. Divergence here is a
    // loud signal that one of them disagrees on the Cypher dialect
    // the seed catalogue assumes.
    let Some((nexus_addr, neo4j_url)) = common::both_endpoints() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR / NEO4J_BENCH_URL not set");
        return;
    };
    let (user, password) = common::bolt_credentials();
    let rt = tokio::runtime::Handle::current();

    let mut nexus = NexusRpcClient::connect(
        nexus_addr,
        common::nexus_rpc_credentials(),
        "nexus",
        rt.clone(),
    )
    .await
    .expect("nexus connect");
    let mut neo4j = Neo4jBoltClient::connect(neo4j_url, user, password, "neo4j", rt)
        .await
        .expect("neo4j connect");
    common::reset_both(&mut nexus, &mut neo4j);

    let load = nexus_bench::dataset::TinyDataset.load_statement();
    let timeout = Duration::from_secs(30);

    // Nexus first; if it errors, the seed literal itself is broken.
    nexus
        .execute(load, timeout)
        .expect("nexus failed to load tiny dataset");
    // Neo4j second.
    neo4j
        .execute(load, timeout)
        .expect("neo4j failed to load tiny dataset");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both NEXUS_BENCH_RPC_ADDR and NEO4J_BENCH_URL to be set"]
async fn comparative_scalar_one_shot() {
    // A trivial `RETURN 1` scenario must produce the same row count
    // on both engines (both should return exactly one row). This is
    // the smallest divergence check the suite performs; the richer
    // row-content comparison lives in §3.4 once the JSON
    // normalisation layer lands.
    let Some((nexus_addr, neo4j_url)) = common::both_endpoints() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR / NEO4J_BENCH_URL not set");
        return;
    };
    let (user, password) = common::bolt_credentials();
    let rt = tokio::runtime::Handle::current();

    let mut nexus = NexusRpcClient::connect(
        nexus_addr,
        common::nexus_rpc_credentials(),
        "nexus",
        rt.clone(),
    )
    .await
    .expect("nexus connect");
    let mut neo4j = Neo4jBoltClient::connect(neo4j_url, user, password, "neo4j", rt)
        .await
        .expect("neo4j connect");
    common::reset_both(&mut nexus, &mut neo4j);

    let scen = ScenarioBuilder::new(
        "integration.scalar",
        "RETURN 1",
        nexus_bench::dataset::DatasetKind::Tiny,
        "RETURN 1 AS n",
    )
    .warmup(1)
    .measured(3)
    .expected_rows(1)
    .timeout(Duration::from_secs(2))
    .build();

    let cfg = RunConfig::default().clamped();

    let mut nc = &mut nexus;
    let nexus_result = run_scenario(&scen, "nexus", &mut nc, &cfg).expect("nexus scenario");
    let mut nc2 = &mut neo4j;
    let neo4j_result = run_scenario(&scen, "neo4j", &mut nc2, &cfg).expect("neo4j scenario");

    assert_eq!(
        nexus_result.rows_returned, neo4j_result.rows_returned,
        "row-count divergence: nexus={}, neo4j={}",
        nexus_result.rows_returned, neo4j_result.rows_returned
    );

    // Smoke that ComparativeRow computes a ratio + classification
    // when both sides are populated.
    let row = ComparativeRow::new(nexus_result, Some(neo4j_result));
    assert!(
        row.ratio_p50.is_some(),
        "ratio_p50 should be Some when both sides populated"
    );
    assert!(
        row.classification.is_some(),
        "classification should be Some when both sides populated"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both NEXUS_BENCH_RPC_ADDR and NEO4J_BENCH_URL to be set"]
async fn comparative_seed_catalogue_completes() {
    // The full seed catalogue must run to completion against both
    // engines in a bounded time. Per-engine row-count divergence is
    // caught by the harness itself via `expected_row_count`; this
    // test additionally asserts the two engines agree on every
    // scenario's row count.
    let Some((nexus_addr, neo4j_url)) = common::both_endpoints() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR / NEO4J_BENCH_URL not set");
        return;
    };
    let (user, password) = common::bolt_credentials();
    let rt = tokio::runtime::Handle::current();

    let mut nexus = NexusRpcClient::connect(
        nexus_addr,
        common::nexus_rpc_credentials(),
        "nexus",
        rt.clone(),
    )
    .await
    .expect("nexus connect");
    let mut neo4j = Neo4jBoltClient::connect(neo4j_url, user, password, "neo4j", rt)
        .await
        .expect("neo4j connect");
    common::reset_both(&mut nexus, &mut neo4j);

    let load = nexus_bench::dataset::TinyDataset.load_statement();
    nexus
        .execute(load, Duration::from_secs(30))
        .expect("nexus load");
    neo4j
        .execute(load, Duration::from_secs(30))
        .expect("neo4j load");

    let cfg = RunConfig::default().clamped();
    let scenarios = seed_scenarios();
    assert!(!scenarios.is_empty(), "seed catalogue must be non-empty");

    for scen in scenarios {
        let mut nc = &mut nexus;
        let nexus_result = run_scenario(&scen, "nexus", &mut nc, &cfg)
            .unwrap_or_else(|e| panic!("{}: nexus scenario failed: {e}", scen.id));
        let mut nc2 = &mut neo4j;
        let neo4j_result = run_scenario(&scen, "neo4j", &mut nc2, &cfg)
            .unwrap_or_else(|e| panic!("{}: neo4j scenario failed: {e}", scen.id));
        assert_eq!(
            nexus_result.rows_returned, neo4j_result.rows_returned,
            "{}: row count divergence nexus={} vs neo4j={}",
            scen.id, nexus_result.rows_returned, neo4j_result.rows_returned
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires both NEXUS_BENCH_RPC_ADDR and NEO4J_BENCH_URL to be set"]
async fn isolation_between_tests_works() {
    // Load the dataset, count nodes, reset, load again, count
    // again — on BOTH engines. If either reset hook is a no-op
    // the second count comes back doubled and trips here.
    let Some((nexus_addr, neo4j_url)) = common::both_endpoints() else {
        eprintln!("skipping: NEXUS_BENCH_RPC_ADDR / NEO4J_BENCH_URL not set");
        return;
    };
    let (user, password) = common::bolt_credentials();
    let rt = tokio::runtime::Handle::current();

    let mut nexus = NexusRpcClient::connect(
        nexus_addr,
        common::nexus_rpc_credentials(),
        "nexus",
        rt.clone(),
    )
    .await
    .expect("nexus connect");
    let mut neo4j = Neo4jBoltClient::connect(neo4j_url, user, password, "neo4j", rt)
        .await
        .expect("neo4j connect");

    let load = nexus_bench::dataset::TinyDataset.load_statement();
    let count = "MATCH (n) RETURN count(n) AS c";
    let timeout = Duration::from_secs(30);

    for pass in 1..=2 {
        common::reset_both(&mut nexus, &mut neo4j);
        nexus
            .execute(load, timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: nexus load failed: {e}"));
        neo4j
            .execute(load, timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: neo4j load failed: {e}"));

        // 100 nodes per engine — each load writes exactly that.
        // If the count is >100, the previous pass's reset was a
        // no-op (Nexus DELETE regression, or Neo4j connection
        // leak). Assert on the value, not just the row shape.
        let n = nexus
            .execute(count, timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: nexus count failed: {e}"));
        let m = neo4j
            .execute(count, timeout)
            .unwrap_or_else(|e| panic!("pass {pass}: neo4j count failed: {e}"));
        assert_eq!(
            n.rows,
            vec![vec![serde_json::json!(100)]],
            "pass {pass}: nexus count after load — reset did not clear?"
        );
        assert_eq!(
            m.rows,
            vec![vec![serde_json::json!(100)]],
            "pass {pass}: neo4j count after load — reset did not clear?"
        );
    }
}
