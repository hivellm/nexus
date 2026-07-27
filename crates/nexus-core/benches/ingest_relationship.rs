//! Engine-level benchmark for relationship-ingest throughput.
//!
//! Two families of benchmark cases are measured:
//!
//! - `distinct_source` / `same_source` (the pre-fix baseline): call the
//!   COUNTED `Engine::create_relationship` — the pre-fix `/ingest` code
//!   path — with no caller-supplied session transaction (`tx_ref =
//!   None`), so every call opens, commits, and durably syncs its own
//!   write transaction. Each edge costs a full LMDB fsync, so
//!   throughput sits around ~3,000-5,500 rel/s regardless of batch
//!   size or fan-out shape.
//! - `batched_distinct_source` / `batched_same_source` (the §1.3
//!   post-fix path): create every edge in the batch via the UNCOUNTED
//!   `Engine::create_relationship_uncounted` (no per-edge catalog
//!   fsync), accumulate a `(TypeId, count)` map, then flush it ONCE
//!   per batch via `Catalog::batch_increment_rel_counts` — mirroring
//!   the batch handler in `nexus-server/src/api/ingest.rs` exactly, so
//!   the delta between the two families quantifies the real `/ingest`
//!   speedup (`docs/analysis/relationship-ingest-perf/`).
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench ingest_relationship
//! ```
//!
//! Two edge shapes are measured per batch size and per family, since a
//! hub-and-spoke write pattern (many edges from one source) is common
//! in ingest pipelines and could plausibly hit different lock/index
//! contention than a chain of distinct sources:
//!
//! - **distinct-source**: edge `i` connects node `i` -> node `i + 1`
//!   (each node has degree <= 2).
//! - **same-source**: edge `i` connects node `0` -> node `i + 1` (a
//!   single hub node accumulates all outgoing edges).
//!
//! Relationship creation is stateful and mutates the graph on every
//! call, so each timed iteration needs its own fresh `Engine` and
//! pre-created nodes — `iter_batched` with `BatchSize::PerIteration`
//! builds that setup outside the timed routine.

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::{Engine, catalog};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

/// Edge-endpoint shape under test.
#[derive(Clone, Copy)]
enum EdgeShape {
    /// Edge `i`: node `i` -> node `i + 1`.
    DistinctSource,
    /// Edge `i`: node `0` -> node `i + 1` (hub).
    SameSource,
}

impl EdgeShape {
    fn label(self) -> &'static str {
        match self {
            EdgeShape::DistinctSource => "distinct_source",
            EdgeShape::SameSource => "same_source",
        }
    }

    fn source_index(self, i: u64) -> u64 {
        match self {
            EdgeShape::DistinctSource => i,
            EdgeShape::SameSource => 0,
        }
    }
}

/// Build a fresh, self-cleaning engine (`Engine::new()` — temp-directory
/// store torn down on drop, see `Engine::new` doc comment) with `n + 1`
/// pre-created `:Bench` nodes so the timed routine only pays for edge
/// creation, not node setup.
fn setup_engine_with_nodes(n: u64) -> (Engine, Vec<u64>) {
    let mut engine = Engine::new().expect("fresh engine");
    let mut node_ids = Vec::with_capacity((n + 1) as usize);
    for _ in 0..=n {
        let id = engine
            .create_node(vec!["Bench".to_string()], serde_json::json!({}))
            .expect("create node");
        node_ids.push(id);
    }
    (engine, node_ids)
}

/// Create exactly `n` relationships of the given `shape` on `engine`,
/// using `node_ids` as the endpoint pool. This is the timed routine.
fn create_relationships(engine: &mut Engine, node_ids: &[u64], shape: EdgeShape, n: u64) {
    for i in 0..n {
        let from = node_ids[shape.source_index(i) as usize];
        let to = node_ids[(i + 1) as usize];
        let rel_id = engine
            .create_relationship(from, to, "REL".to_string(), serde_json::json!({}))
            .expect("create relationship");
        black_box(rel_id);
    }
}

/// Create exactly `n` relationships of the given `shape` on `engine` via
/// the post-fix batched-count `/ingest` path: every edge is created with
/// `Engine::create_relationship_uncounted` (no per-edge catalog fsync),
/// the resulting per-type counts are accumulated in-memory, and flushed
/// ONCE via `Catalog::batch_increment_rel_counts` after the loop —
/// exactly mirroring `create_relationship_direct` +
/// `batch_increment_rel_counts` in `nexus-server/src/api/ingest.rs`.
/// This is the timed routine for the `batched_*` benchmark cases.
fn create_relationships_batched(engine: &mut Engine, node_ids: &[u64], shape: EdgeShape, n: u64) {
    let mut rel_counts: HashMap<catalog::TypeId, u32> = HashMap::new();
    for i in 0..n {
        let from = node_ids[shape.source_index(i) as usize];
        let to = node_ids[(i + 1) as usize];
        let (rel_id, type_id) = engine
            .create_relationship_uncounted(from, to, "REL".to_string(), serde_json::json!({}))
            .expect("create relationship");
        black_box(rel_id);
        *rel_counts.entry(type_id).or_insert(0) += 1;
    }

    let updates: Vec<(catalog::TypeId, u32)> = rel_counts.into_iter().collect();
    engine
        .catalog
        .batch_increment_rel_counts(&updates)
        .expect("batch increment rel counts");
}

/// Batch sizes to sweep. Each edge costs roughly ~0.33 ms (fsync-bound
/// pre-fix), so 5,000 edges is already ~1.7s per iteration — sample
/// counts and measurement time below are sized down accordingly to
/// keep total wall-clock reasonable.
const BATCH_SIZES: [u64; 3] = [100, 1_000, 5_000];

fn measurement_time_for(n: u64) -> Duration {
    // ~0.33 ms/edge pre-fix * n edges * sample_size(10), plus headroom.
    let estimated_secs = (n as f64 * 0.00033 * 10.0).ceil() as u64;
    Duration::from_secs(estimated_secs.max(5))
}

fn bench_ingest_relationship(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest_relationship");

    for &n in &BATCH_SIZES {
        group.sample_size(10);
        group.measurement_time(measurement_time_for(n));
        group.throughput(Throughput::Elements(n));

        for shape in [EdgeShape::DistinctSource, EdgeShape::SameSource] {
            group.bench_with_input(BenchmarkId::new(shape.label(), n), &n, |b, &n| {
                b.iter_batched(
                    || setup_engine_with_nodes(n),
                    |(mut engine, node_ids)| {
                        create_relationships(&mut engine, &node_ids, shape, n);
                    },
                    criterion::BatchSize::PerIteration,
                )
            });

            let batched_label = format!("batched_{}", shape.label());
            group.bench_with_input(BenchmarkId::new(batched_label, n), &n, |b, &n| {
                b.iter_batched(
                    || setup_engine_with_nodes(n),
                    |(mut engine, node_ids)| {
                        create_relationships_batched(&mut engine, &node_ids, shape, n);
                    },
                    criterion::BatchSize::PerIteration,
                )
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_ingest_relationship);
criterion_main!(benches);
