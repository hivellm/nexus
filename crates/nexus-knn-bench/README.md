# nexus-knn-bench

KNN recall + latency benchmark for the Nexus HNSW index.

This crate is a developer tool. It drives the engine-level
[`nexus_core::index::KnnIndex`](../nexus-core/src/index/mod.rs) in-
process so it can sweep HNSW parameters that the RPC surface does
not expose: `M`, `ef_construction`, `ef_search`. The output is
directly comparable to numbers published by other vector DBs that
use the same hnswlib-derived algorithm.

For the full reproduction recipe + methodology, see
[`docs/performance/KNN_RECALL.md`](../../docs/performance/KNN_RECALL.md).

## Quick start

```bash
# 1. Download corpora (SIFT1M + GloVe-200d).
bash scripts/benchmarks/download_knn_corpora.sh

# 2. Run the sweep on SIFT1M.
cargo +nightly run --release -p nexus-knn-bench --bin knn-recall -- \
  --json-out sift1m-recall.json \
  --csv-out  sift1m-recall.csv \
  sift \
  --base   data/knn-corpora/sift/sift_base.fvecs \
  --queries data/knn-corpora/sift/sift_query.fvecs \
  --groundtruth data/knn-corpora/sift/sift_groundtruth.ivecs
```

## CLI

```text
knn-recall [--json-out PATH] [--csv-out PATH]
           [--cache-dir PATH] [--k N]
           [--base-limit N] [--query-limit N]
           [--m-values 8,16,32,64]
           [--ef-construction-values 100,200,400,800]
           [--ef-search-values 50,100,200,400]
           <corpus>

Subcommands:
  sift   --base PATH --queries PATH [--groundtruth PATH]
  glove  --path PATH [--query-count N] [--base-limit N]
```

The harness:

1. Loads the corpus (fvecs/ivecs or GloVe text).
2. Computes brute-force ground-truth top-`k` (cached on disk).
3. Sweeps `(M, ef_construction, ef_search)`. Each unique
   `(M, ef_construction)` builds one HNSW index; `ef_search`
   variations reuse it.
4. Records per-cell recall@1 / 10 / 100 and latency p50 / p95 / p99.
5. Emits JSON + CSV reports.

## Library

```rust
use nexus_knn_bench::{Corpus, Groundtruth, SweepConfig, run_sweep};

let corpus = Corpus::load_sift(&base_path, &query_path, None)?;
let truth = Groundtruth::compute_with_cache(
    &corpus.base, &corpus.queries, 100, &cache_dir,
)?;
let cells = run_sweep(&corpus, &truth, &SweepConfig::default())?;
```

## Tests

```bash
cargo +nightly test -p nexus-knn-bench --lib
```

21 unit tests cover corpus parsers, ground-truth + cache, recall +
latency aggregation, and a small synthetic sweep that pins
`ef_search` recall monotonicity.

## License

Apache-2.0 (workspace).
