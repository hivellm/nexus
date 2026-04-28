# Proposal: phase7_knn-recall-benchmark

## Why

Nexus's KNN benchmarks (in `docs/performance/PERFORMANCE_V1.md` and the protocol bench suite) are **latency-only**. There is no published recall-at-k vs ground-truth ANN. Vector-DB convention requires both: latency *and* recall. Pinecone, Weaviate, Qdrant, Milvus all publish recall numbers on standard corpora (SIFT1M, GloVe, MS MARCO). Without ours, users cannot compare apples-to-apples and a competitor running their own Nexus benchmark could publish numbers we have no defence against.

## What Changes

- New benchmark harness `nexus-bench/benches/knn_recall.rs` that:
  - Loads a public corpus (SIFT1M for f32, GloVe for English embeddings).
  - Computes brute-force ground-truth top-k for a query set.
  - Builds a Nexus HNSW index and queries the same set.
  - Reports recall@1, recall@10, recall@100 alongside p50/p95/p99 latency.
- Sweep over `M` and `ef_construction` parameters; report the recall/latency Pareto frontier.
- Document the corpus + reproduction in `docs/performance/KNN_RECALL.md`.
- Cross-link from README + competitor-comparison docs.
- Add a v1.2 perf summary entry.

## Impact

- Affected specs: new `docs/performance/KNN_RECALL.md`, README cross-link.
- Affected code: new `nexus-bench/benches/knn_recall.rs`, `nexus-bench/data/` placeholder for corpus paths (download script in `scripts/benchmarks/download_knn_corpora.sh`).
- Breaking change: NO.
- User benefit: defensible vector-DB credibility numbers; head-to-head comparable with Pinecone / Weaviate / Qdrant / Milvus.
