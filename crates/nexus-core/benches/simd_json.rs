//! Criterion bench comparing `serde_json` (scalar) to `simd-json`
//! via the `simd::json` dispatch layer.
//!
//! Sizes span the ingest payloads Nexus sees in practice:
//! - 4 KiB: small transaction batch (10–50 nodes)
//! - 64 KiB: crossover point for the dispatch threshold
//! - 1 MiB: typical RAG ingest payload (1K nodes + f32 embeddings)
//! - 10 MiB: large bulk import (100K nodes)
//!
//! ```text
//! cargo +nightly bench -p nexus-core --bench simd_json
//! ```

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use nexus_core::simd::json as simd_json_dispatch;
use serde::{Deserialize, Serialize};
use std::hint::black_box;

#[derive(Serialize, Deserialize)]
struct Node {
    id: u64,
    labels: Vec<String>,
    properties: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct Bulk {
    nodes: Vec<Node>,
}

/// Build a Bulk payload whose serialised size is ≥ `target_bytes`.
///
/// Avoids the O(n²) "push + re-serialise every iteration" trap by
/// estimating the per-node footprint from a small sample first.
fn make_payload(target_bytes: usize) -> Vec<u8> {
    // Probe payload size per node on a 32-node sample.
    let sample: Vec<Node> = (0..32).map(node_at).collect();
    let sample_bytes = serde_json::to_vec(&Bulk { nodes: sample }).unwrap().len();
    let bytes_per_node = (sample_bytes / 32).max(1);

    let estimated_count = target_bytes.div_ceil(bytes_per_node) + 16;
    let nodes: Vec<Node> = (0..estimated_count as u64).map(node_at).collect();
    let body = serde_json::to_vec(&Bulk { nodes }).unwrap();
    assert!(
        body.len() >= target_bytes / 2,
        "estimator produced {} bytes for target {}",
        body.len(),
        target_bytes
    );
    body
}

fn node_at(i: u64) -> Node {
    let embedding: Vec<f32> = (0..16).map(|j| ((i * 16 + j) as f32) * 0.001).collect();
    Node {
        id: i,
        labels: vec![format!("Label{}", i % 5)],
        properties: serde_json::json!({
            "idx": i,
            "name": format!("node-{i}"),
            "embedding": embedding,
            "tags": ["a", "b", "c"],
        }),
    }
}

const SIZES: &[usize] = &[4_096, 64 * 1024, 1024 * 1024];

fn bench_json(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_bulk_ingest");
    for &target in SIZES {
        let payload = make_payload(target);
        let actual_len = payload.len();
        group.throughput(Throughput::Bytes(actual_len as u64));
        group.bench_with_input(
            BenchmarkId::new("serde_json", actual_len),
            &actual_len,
            |b, _| {
                b.iter(|| {
                    let _: Bulk = serde_json::from_slice(black_box(&payload)).unwrap();
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("dispatch", actual_len),
            &actual_len,
            |b, _| {
                b.iter_batched(
                    || payload.clone(),
                    |mut body| {
                        let _: Bulk = simd_json_dispatch::parse_mut(black_box(&mut body)).unwrap();
                    },
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_json);
criterion_main!(benches);
