## 1. Corpus + ground truth
- [x] 1.1 Add `scripts/benchmarks/download_knn_corpora.sh` for SIFT1M + GloVe (200d English)
- [x] 1.2 Implement brute-force ground-truth top-k computation (delivered as `crates/nexus-knn-bench/src/groundtruth.rs`; the original proposal targeted `nexus-bench/src/knn_groundtruth.rs`, but that crate's no-core-dep guardrail forced a separate dev crate)
- [x] 1.3 Cache ground-truth on disk so repeated runs are fast

## 2. Benchmark harness
- [x] 2.1 Implement parameter sweep harness (delivered at `crates/nexus-knn-bench/src/sweep.rs` plus the `knn-recall` CLI binary; see the proposal-vs-delivery note above)
- [x] 2.2 Sweep `M` ∈ {8, 16, 32, 64} × `ef_construction` ∈ {100, 200, 400, 800}
- [x] 2.3 Sweep `ef_search` ∈ {50, 100, 200, 400} per index build
- [x] 2.4 Measure recall@1, recall@10, recall@100, latency p50/p95/p99
- [x] 2.5 Output JSON + CSV for downstream analysis

## 3. Documentation
- [x] 3.1 Create `docs/performance/KNN_RECALL.md` with results tables + recall/latency Pareto methodology
- [x] 3.2 Document corpus, ground-truth methodology, hardware, reproduction steps
- [x] 3.3 Cross-link from README + performance summary

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation
- [x] 4.2 Write tests covering the new behavior
- [x] 4.3 Run tests and confirm they pass
