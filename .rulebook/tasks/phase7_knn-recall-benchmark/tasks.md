## 1. Corpus + ground truth
- [ ] 1.1 Add `scripts/benchmarks/download_knn_corpora.sh` for SIFT1M + GloVe (200d English)
- [ ] 1.2 Implement brute-force ground-truth top-k computation in `nexus-bench/src/knn_groundtruth.rs`
- [ ] 1.3 Cache ground-truth on disk so repeated runs are fast

## 2. Benchmark harness
- [ ] 2.1 Implement `nexus-bench/benches/knn_recall.rs` Criterion bench
- [ ] 2.2 Sweep `M` ∈ {8, 16, 32, 64} × `ef_construction` ∈ {100, 200, 400, 800}
- [ ] 2.3 Sweep `ef_search` ∈ {50, 100, 200, 400} per index build
- [ ] 2.4 Measure recall@1, recall@10, recall@100, latency p50/p95/p99
- [ ] 2.5 Output JSON + CSV for downstream analysis

## 3. Documentation
- [ ] 3.1 Create `docs/performance/KNN_RECALL.md` with results tables + recall/latency Pareto chart
- [ ] 3.2 Document corpus, ground-truth methodology, hardware, reproduction steps
- [ ] 3.3 Cross-link from README, competitor-comparison docs, performance summary

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior
- [ ] 4.3 Run tests and confirm they pass
