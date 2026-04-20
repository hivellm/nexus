//! Nexus ↔ Neo4j comparative benchmark harness.
//!
//! # Contract
//!
//! * **HTTP-only**. No `nexus-core` dep. The client speaks to a Nexus
//!   server the operator has started themselves; the harness cannot
//!   instantiate an engine.
//! * **Unit tests never hit the network**. Every module here is
//!   pure-logic — `Scenario`, `Classification`, `ComparativeRow`, the
//!   Markdown / JSON emitters, the `TinyDataset` generator (which
//!   returns a single Cypher string, not 280 of them). Integration
//!   tests that require a live server live under `tests/`, are
//!   `#[ignore]` by default, and fire only under the `live-bench`
//!   feature.
//! * **Hard timeout per RPC**. Whenever the optional `live-bench`
//!   client makes a request, it's wrapped in
//!   `tokio::time::timeout`. No caller can hang the runtime.
//! * **Debug-build refusal**. The CLI binary checks
//!   `cfg!(debug_assertions)` at boot and refuses to run with a loud
//!   error unless `NEXUS_BENCH_ALLOW_DEBUG=1` is set — benchmark
//!   numbers from a debug build are meaningless.
//!
//! See `docs/benchmarks/README.md` for the full operator workflow.

pub mod dataset;
pub mod harness;
pub mod report;
pub mod scenario;
pub mod scenario_catalog;

#[cfg(feature = "live-bench")]
pub mod client;

pub use dataset::{Dataset, TinyDataset};
pub use harness::{HarnessError, RunConfig, ScenarioResult};
pub use report::{Classification, ComparativeRow, json::JsonReport, markdown::MarkdownReport};
pub use scenario::{Scenario, ScenarioBuilder};

#[cfg(feature = "live-bench")]
pub use client::{BenchClient, ClientError, ExecOutcome, HttpClient};

#[cfg(feature = "neo4j")]
pub use client::Neo4jBoltClient;
