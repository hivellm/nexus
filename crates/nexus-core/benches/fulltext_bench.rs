//! Full-text search benchmarks (phase6_fulltext-benchmarks).
//!
//! Three scenarios over a deterministic 100k × ~1 KB corpus:
//!
//! - `single_term_query`: bare-term BM25 query. Target: p95 < 5 ms.
//! - `phrase_query`: two-term phrase match. Target: p95 < 20 ms.
//! - `ingest_throughput`: bulk ingest rate. Target: > 5k docs/sec.
//!
//! All scenarios exercise [`FullTextRegistry`] through its public
//! API (same path `db.index.fulltext.*` procedures use), so the
//! numbers reflect end-to-end behaviour including schema / analyzer
//! registration and the synchronous-reload commit cadence.
//!
//! Corpus generation uses a seeded LCG so the byte layout is
//! reproducible run-to-run. We avoid pulling in `rand` as a bench
//! dependency — the generator is self-contained.

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{Criterion, criterion_group, criterion_main};
use nexus_core::index::fulltext_registry::FullTextRegistry;
use tempfile::TempDir;

const CORPUS_SIZE: usize = 100_000;
const TARGET_DOC_BYTES: usize = 1024;
const VOCABULARY: &[&str] = &[
    "the",
    "quick",
    "brown",
    "fox",
    "jumps",
    "over",
    "lazy",
    "dog",
    "graph",
    "database",
    "vector",
    "search",
    "knowledge",
    "retrieval",
    "index",
    "query",
    "token",
    "document",
    "content",
    "analyzer",
    "cypher",
    "node",
    "relationship",
    "property",
    "embedding",
    "semantic",
    "matching",
    "similarity",
    "ranking",
    "score",
    "distance",
    "cosine",
    "tantivy",
    "full",
    "text",
    "phrase",
    "term",
    "field",
    "value",
    "label",
    "ngram",
    "stemmer",
    "language",
    "stopword",
    "tokenizer",
    "latency",
    "throughput",
    "scalable",
    "performance",
    "cluster",
    "shard",
    "replica",
    "leader",
    "follower",
    "commit",
    "snapshot",
    "transaction",
    "recovery",
    "replay",
    "catalog",
    "schema",
    "storage",
    "buffer",
    "cache",
    "eviction",
    "prefetch",
    "traversal",
    "bfs",
    "dfs",
    "path",
    "cycle",
    "component",
    "shortest",
    "reachability",
    "aggregation",
    "projection",
    "filter",
];

/// Seeded LCG so corpus generation is deterministic across runs.
/// Constants from Numerical Recipes; state is 64-bit.
struct Lcg(u64);
impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }
    fn index(&mut self, n: usize) -> usize {
        (self.next() as usize) % n
    }
}

fn generate_document(rng: &mut Lcg, target_bytes: usize) -> String {
    let mut out = String::with_capacity(target_bytes + 32);
    while out.len() < target_bytes {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(VOCABULARY[rng.index(VOCABULARY.len())]);
    }
    out
}

/// Build a fresh registry with `size` documents ingested. Returns
/// the registry plus the TempDir guard so the caller can keep the
/// fs alive for the duration of the bench.
///
/// Uses the bulk-ingest path so corpus construction does not pay the
/// commit-per-doc cost that `add_node_document` carries.
fn build_corpus(size: usize, index_name: &str) -> (FullTextRegistry, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let reg = FullTextRegistry::new();
    reg.set_base_dir(dir.path().to_path_buf());
    reg.create_node_index(index_name, &["Doc"], &["body"], Some("standard"))
        .expect("create_node_index");
    let mut rng = Lcg::new(0xCAFEF00D_DEADBEEFu64);
    let bodies: Vec<String> = (0..size)
        .map(|_| generate_document(&mut rng, TARGET_DOC_BYTES))
        .collect();
    let docs: Vec<(u64, u32, u32, &str)> = bodies
        .iter()
        .enumerate()
        .map(|(i, body)| (i as u64, 0u32, 0u32, body.as_str()))
        .collect();
    reg.add_node_documents_bulk(index_name, &docs)
        .expect("bulk ingest");
    (reg, dir)
}

fn bench_single_term_query(c: &mut Criterion) {
    let (reg, _dir) = build_corpus(CORPUS_SIZE, "single_term_bench");
    let mut group = c.benchmark_group("fulltext_single_term");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));
    group.bench_function("corpus_100k_1kb", |b| {
        b.iter(|| {
            let hits = reg
                .query(
                    black_box("single_term_bench"),
                    black_box("tantivy"),
                    Some(10),
                )
                .expect("query");
            black_box(hits.len());
        });
    });
    group.finish();
}

fn bench_phrase_query(c: &mut Criterion) {
    let (reg, _dir) = build_corpus(CORPUS_SIZE, "phrase_bench");
    let mut group = c.benchmark_group("fulltext_phrase");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(8));
    group.bench_function("corpus_100k_1kb", |b| {
        b.iter(|| {
            let hits = reg
                .query(
                    black_box("phrase_bench"),
                    black_box("\"quick brown\""),
                    Some(10),
                )
                .expect("phrase query");
            black_box(hits.len());
        });
    });
    group.finish();
}

fn bench_ingest_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("fulltext_ingest");
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(12));
    // Bulk path: one commit per batch. Docs per iter = 10 000 so the
    // reported throughput is directly comparable to the >5k docs/sec
    // SLO spelled out in the task proposal.
    group.throughput(criterion::Throughput::Elements(10_000));
    group.bench_function("bulk_10k_docs", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let dir = TempDir::new().expect("tempdir");
                let reg = FullTextRegistry::new();
                reg.set_base_dir(dir.path().to_path_buf());
                reg.create_node_index("ingest_bench", &["Doc"], &["body"], Some("standard"))
                    .expect("create");
                let mut rng = Lcg::new(0x1234_5678_ABCD_EF01);
                let bodies: Vec<String> = (0..10_000)
                    .map(|_| generate_document(&mut rng, TARGET_DOC_BYTES))
                    .collect();
                let docs: Vec<(u64, u32, u32, &str)> = bodies
                    .iter()
                    .enumerate()
                    .map(|(i, body)| (i as u64, 0u32, 0u32, body.as_str()))
                    .collect();
                let start = Instant::now();
                reg.add_node_documents_bulk("ingest_bench", &docs)
                    .expect("bulk ingest");
                total += start.elapsed();
            }
            total
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_single_term_query,
    bench_phrase_query,
    bench_ingest_throughput,
);
criterion_main!(benches);
