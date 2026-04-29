//! KNN recall + latency benchmark harness for the Nexus HNSW index.
//!
//! # Why a separate crate
//!
//! The existing `nexus-bench` crate is locked to the native RPC
//! transport — it intentionally has no `nexus-core` dependency so its
//! unit tests stay sandboxed. This crate, by contrast, drives the
//! engine-level [`KnnIndex`](nexus_core::index::KnnIndex) directly so
//! it can sweep HNSW parameters (`M`, `ef_construction`, `ef_search`)
//! that the RPC surface does not expose. Numbers from this benchmark
//! are therefore engine-level and directly comparable to numbers
//! published by other vector DBs that use the same `hnswlib`-derived
//! algorithm (Pinecone, Weaviate, Qdrant, Milvus).
//!
//! # Workflow
//!
//! 1. Download a public corpus (`SIFT1M`, `GloVe-200d`) using
//!    `scripts/benchmarks/download_knn_corpora.sh`.
//! 2. Compute brute-force ground-truth top-k for the query set
//!    ([`groundtruth::compute`]) — cached on disk so repeat runs are
//!    cheap.
//! 3. Sweep `(M, ef_construction, ef_search)` ([`sweep::run`]) and
//!    record recall@k + latency p50/p95/p99 per cell
//!    ([`metrics::Recall`], [`metrics::LatencyStats`]).
//! 4. Emit results as JSON + CSV ([`report::write_json`],
//!    [`report::write_csv`]).
//!
//! See `docs/performance/KNN_RECALL.md` for the full methodology and
//! published numbers.

pub mod corpus;
pub mod groundtruth;
pub mod metrics;
pub mod report;
pub mod sweep;

pub use corpus::{Corpus, CorpusFormat, CorpusKind};
pub use groundtruth::{Groundtruth, GroundtruthError};
pub use metrics::{LatencyStats, Recall, recall_at_k};
pub use report::{ReportError, write_csv, write_json};
pub use sweep::{SweepCell, SweepConfig, SweepError, run as run_sweep};

/// Floating-point type used by every public surface in this crate.
///
/// SIFT and GloVe both ship as `f32`. We keep the alias narrow rather
/// than generic so the bench code can rely on bit-exact reproducibility
/// for the brute-force ground-truth checks.
pub type Vector = Vec<f32>;
