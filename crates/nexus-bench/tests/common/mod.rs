//! Shared helpers for the two live-integration test files. Lives
//! under `tests/common/` so both `live_rpc.rs` and `live_compare.rs`
//! can `mod common;` it without the Cargo test harness treating it
//! as a separate integration binary.
//!
//! What's here:
//!
//! * Env-var helpers for the three endpoint kinds the suite
//!   recognises (Nexus RPC addr, Neo4j URL, plus credentials on
//!   both sides).
//! * `reset_single` / `reset_both` — per-test fixture isolation.
//!   Without these, running `cargo test --ignored` as a batch
//!   stacks TinyDataset loads on top of each other and trips the
//!   row-count divergence guard on the second run.

#![cfg(feature = "live-bench")]
#![allow(dead_code)]

use std::time::Duration;

use nexus_bench::client::BenchClient;
#[cfg(feature = "neo4j")]
use nexus_bench::client::Neo4jBoltClient;
use nexus_bench::client::NexusRpcCredentials;

/// 30 s ceiling on the reset round-trip. Matches the dataset-load
/// timeout; a reset that takes longer than the load itself is a
/// signal to abort rather than cloak as latency.
pub const RESET_TIMEOUT: Duration = Duration::from_secs(30);

/// Nexus RPC address from `NEXUS_BENCH_RPC_ADDR`, if set.
pub fn nexus_rpc_addr() -> Option<String> {
    std::env::var("NEXUS_BENCH_RPC_ADDR").ok()
}

/// Neo4j Bolt URL from `NEO4J_BENCH_URL`, if set.
#[cfg(feature = "neo4j")]
pub fn neo4j_bolt_url() -> Option<String> {
    std::env::var("NEO4J_BENCH_URL").ok()
}

/// `(nexus_rpc_addr, neo4j_url)` when both env vars are set.
#[cfg(feature = "neo4j")]
pub fn both_endpoints() -> Option<(String, String)> {
    let nexus = nexus_rpc_addr()?;
    let neo4j = neo4j_bolt_url()?;
    Some((nexus, neo4j))
}

/// Nexus RPC credentials built from the env vars.
pub fn nexus_rpc_credentials() -> NexusRpcCredentials {
    NexusRpcCredentials {
        api_key: std::env::var("NEXUS_BENCH_API_KEY").ok(),
        username: std::env::var("NEXUS_BENCH_USER").ok(),
        password: std::env::var("NEXUS_BENCH_PASSWORD").ok(),
    }
}

/// Bolt credentials. Default to `neo4j` / `neo4j` so an
/// `NEO4J_AUTH=none` container (accepts any HELLO) and a
/// stock-password setup both work.
#[cfg(feature = "neo4j")]
pub fn bolt_credentials() -> (String, String) {
    (
        std::env::var("NEO4J_BENCH_USER").unwrap_or_else(|_| "neo4j".into()),
        std::env::var("NEO4J_BENCH_PASSWORD").unwrap_or_else(|_| "neo4j".into()),
    )
}

/// Wipe all nodes + relationships on a single engine. Panics with
/// the engine label on failure — a reset that cannot complete is a
/// test-harness bug, not a flaky signal.
pub fn reset_single<C: BenchClient>(client: &mut C) {
    let label = client.engine_name().to_string();
    client
        .reset(RESET_TIMEOUT)
        .unwrap_or_else(|e| panic!("{label}: reset failed: {e}"));
}

/// Wipe both engines. Nexus first, then Neo4j — same order as
/// every other cross-engine step in the suite so a failure points
/// at the exact side that drifted.
#[cfg(feature = "neo4j")]
pub fn reset_both<C: BenchClient>(nexus: &mut C, neo4j: &mut Neo4jBoltClient) {
    reset_single(nexus);
    reset_single(neo4j);
}
