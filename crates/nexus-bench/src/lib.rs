//! Nexus ↔ Neo4j comparative benchmark harness.
//!
//! # Shape
//!
//! * [`client`] — engine-agnostic [`BenchClient`] trait + concrete
//!   [`NexusClient`] (in-process) and (feature-gated)
//!   [`Neo4jClient`] (Bolt via neo4rs). A benchmark is written
//!   against the trait so the same scenario description drives both
//!   engines.
//! * [`dataset`] — reproducible dataset catalogue. Each dataset is a
//!   deterministic generator + a loader-per-engine. The `micro`
//!   dataset ships in-tree; LDBC SNB and vector datasets are
//!   scaffolded with the same shape.
//! * [`scenario`] — a [`Scenario`] struct pairs a Cypher query with
//!   its dataset, warmup / measured iteration counts, and an
//!   expected row-count for the output-divergence guard.
//! * [`harness`] — the driver loop: install the dataset, warm the
//!   engine, measure N iterations, collect p50 / p95 / p99 + peak
//!   latency + throughput + a timeout safety net.
//! * [`report`] — Markdown / JSON emitters. The JSON form is the
//!   machine-readable baseline the CI gate compares against; the
//!   Markdown form is human-consumable.
//!
//! # Design rationale
//!
//! The harness is intentionally **one crate, not a workspace** inside
//! `crates/nexus-bench`: scenarios import the same [`BenchClient`] as
//! the runner, so adding a scenario is a single-file edit — no
//! cross-crate glue. The Neo4j client is opt-in via a feature flag so
//! `cargo test --package nexus-bench` passes on a developer's
//! machine without a running Neo4j server.

pub mod client;
pub mod dataset;
pub mod harness;
pub mod report;
pub mod scenario;
pub mod scenario_catalog;

#[cfg(feature = "neo4j")]
pub use client::Neo4jClient;
pub use client::{BenchClient, ClientError, ExecOutcome, NexusClient, Row};
pub use dataset::{Dataset, DatasetKind, DatasetLoadError, micro::MicroDataset};
pub use harness::{HarnessError, RunConfig, ScenarioResult, run_scenario};
pub use report::{Classification, ComparativeRow, json::JsonReport, markdown::MarkdownReport};
pub use scenario::{Scenario, ScenarioBuilder};
