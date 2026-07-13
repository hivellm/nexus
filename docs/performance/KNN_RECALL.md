# KNN recall + latency benchmark

> Engine-level recall and latency numbers for the Nexus HNSW index,
> measured against the public SIFT1M and GloVe-200d corpora using the
> `nexus-knn-bench` harness.

Existing Nexus performance docs publish KNN **latency** numbers
(`docs/performance/PERFORMANCE_V1.md`). This document publishes the
**recall** numbers that pair with them — the methodology every other
vector DB (Pinecone, Weaviate, Qdrant, Milvus) reports alongside
latency. Without recall, latency is uncomparable: a system that
returns random IDs in 1 µs is not faster than one that returns the
true neighbours in 100 µs.

The harness lives at `crates/nexus-knn-bench/` and is reproducible
end-to-end from the recipes below.

## 0. Published results — SIFT1M (2026-07-13, first measured run)

Full 64-cell sweep (m ∈ {8,16,32,64} × ef_construction ∈ {100,200,400,800}
× ef_search ∈ {50,100,200,400}), 1,000,000 base vectors (128d), 10,000
queries, k=100, ground truth from `sift_groundtruth.ivecs`. Host: Ryzen 9
7950X3D, Windows 10. Raw grid: `docs/performance/data/sift1m-recall.json`
/ `.csv` (checked in). Representative operating points:

| Config (m / efc / efs) | recall@1 | recall@10 | recall@100 | p50 | p95 | p99 | build |
|---|---:|---:|---:|---:|---:|---:|---:|
| 8 / 100 / 50 (fastest) | 0.852 | 0.848 | 0.768 | 530 µs | 636 µs | 693 µs | 468 s |
| **16 / 100 / 50 (sweet spot)** | **0.950** | **0.948** | 0.892 | **787 µs** | **1.02 ms** | 1.10 ms | 649 s |
| 16 / 200 / 200 | 0.965 | 0.971 | 0.954 | 1.51 ms | 1.93 ms | 2.09 ms | 1,257 s |
| 32 / 100 / 50 | 0.975 | 0.975 | 0.928 | 967 µs | 1.23 ms | 1.34 ms | 752 s |
| 32 / 200 / 200 | 0.985 | 0.989 | 0.980 | 1.93 ms | 2.56 ms | 2.78 ms | 1,501 s |
| 64 / 400 / 400 (max quality) | 0.991 | 0.996 | 0.996 | 4.93 ms | 6.70 ms | 7.70 ms | 3,442 s |

Reading of the headline claims: the long-standing **"<2 ms p95"** target
holds up to ~97% recall@10 (`16/200/200` at 1.93 ms p95); the sweet-spot
config delivers **95% recall@1 at ~1 ms p95**. Recall ≥99% costs 2.5–7 ms
p95 on SIFT1M — configure per workload.

## 0.1 Published results — GloVe-200d (2026-07-13, reduced grid)

399,000 base vectors (200d, angular), 1,000 queries, k=100; reduced grid
(m ∈ {16,32} × efc ∈ {100,200} × efs ∈ {50,100,200}). Raw grid:
`docs/performance/data/glove200-recall.json` / `.csv`. GloVe at this scale
is substantially easier than SIFT1M — every cell exceeds 99% recall@1:

| Config (m / efc / efs) | recall@1 | recall@10 | p50 | p95 | build |
|---|---:|---:|---:|---:|---:|
| **16 / 200 / 50 (sweet spot)** | **0.993** | **0.994** | 483 µs | **884 µs** | 792 s |
| 16 / 100 / 50 (cheapest build) | 0.991 | 0.993 | 576 µs | 1.07 ms | 388 s |
| 32 / 200 / 200 (max quality) | 0.999 | 0.999 | 1.27 ms | 2.64 ms | 1,178 s |

Headline: **99.3% recall@1 at 884 µs p95** — comfortably inside the
"<2 ms p95" claim with recall to spare.

## 1. Methodology

| Step | Detail |
|---|---|
| Engine | [`nexus_core::index::KnnIndex`](../../crates/nexus-core/src/index/mod.rs) backed by [`hnsw_rs::Hnsw`](https://docs.rs/hnsw_rs) with `DistSimdCosine` (SIMD-dispatched cosine distance). |
| Distance | Cosine. Both base and query vectors are L2-normalised before the HNSW insert / search; recall is therefore measured against the cosine-distance brute-force ground truth. |
| Ground truth | Exact brute-force top-`k` per query, computed once (`O(\|base\| × \|queries\| × dim)`) and cached on disk under `data/knn-corpora/.cache/groundtruth-<hash>.nkgb`. Cache key includes corpus shape + first/last vector bits, so any change invalidates it. |
| Recall metric | `recall@k = |truth_top_k ∩ approx_top_k| / k`, averaged across queries. We report `recall@1`, `recall@10`, and `recall@100` per cell. |
| Latency metric | Per-query `Instant::now()` deltas, summarised as mean / p50 / p95 / p99 / min / max in microseconds. Each cell measures one full pass over the query set — no warm-up loop, since HNSW caches converge after a handful of queries and the headline number is the steady-state behaviour. |
| Sweep grid | `M ∈ {8, 16, 32, 64}` × `ef_construction ∈ {100, 200, 400, 800}` × `ef_search ∈ {50, 100, 200, 400}` = 64 cells. Each `(M, ef_construction)` triple builds one index; `ef_search` only changes the query path. |
| Hardware | One physical core; no concurrency at the bench level. Numbers are RAM-resident — disk I/O is not on the search path of the HNSW algorithm. |

The full reproducibility recipe is below; results tables are populated
by running the harness on a representative box and checking the JSON
+ CSV under `docs/performance/data/`. Until the first reference run
lands, only the methodology + commands are documented — published
numbers will follow.

## 2. Reproduction

### 2.1 Fetch the corpora

```bash
bash scripts/benchmarks/download_knn_corpora.sh
# → data/knn-corpora/sift/sift_base.fvecs
# → data/knn-corpora/sift/sift_query.fvecs
# → data/knn-corpora/sift/sift_groundtruth.ivecs
# → data/knn-corpora/glove/glove.6B.200d.txt   (and 50d / 100d / 300d siblings)
```

The download script is idempotent — re-running it costs only the HEAD
checks the mirrors validate.

### 2.2 Run the SIFT1M sweep

```bash
cargo +nightly run --release -p nexus-knn-bench --bin knn-recall -- \
  --json-out docs/performance/data/sift1m-recall.json \
  --csv-out  docs/performance/data/sift1m-recall.csv \
  sift \
  --base   data/knn-corpora/sift/sift_base.fvecs \
  --queries data/knn-corpora/sift/sift_query.fvecs \
  --groundtruth data/knn-corpora/sift/sift_groundtruth.ivecs
```

The first invocation computes the brute-force ground truth (a few
minutes on a single core). Every subsequent run reuses the disk
cache and goes straight to the sweep.

### 2.3 Run the GloVe-200d sweep

```bash
cargo +nightly run --release -p nexus-knn-bench --bin knn-recall -- \
  --json-out docs/performance/data/glove200-recall.json \
  --csv-out  docs/performance/data/glove200-recall.csv \
  glove \
  --path data/knn-corpora/glove/glove.6B.200d.txt \
  --query-count 1000 \
  --base-limit 400000
```

GloVe ships as one combined file. The harness picks the first
`--query-count` lines as the query set and the rest as the base set;
`--base-limit` caps the base size (the full file is 400 K vectors,
which fits comfortably in 300 MB of RAM at 200-d × `f32`).

### 2.4 Smoke (synthetic) run

For CI or quick sanity checks, the unit tests exercise the full
pipeline against a synthetic corpus:

```bash
cargo +nightly test -p nexus-knn-bench --lib
```

21 tests cover corpus loaders, ground-truth correctness + cache
roundtrip, recall + latency aggregations, and a 4-cell sweep that
asserts `ef_search` monotonicity in recall@10.

## 3. Sweep dimensions

### 3.1 `M` (max outgoing edges per node)

Larger `M` builds a denser navigability graph: better recall at the
cost of build time and memory. Typical sweet spots for general-purpose
embeddings:

| Dataset | `M` | Notes |
|---|---|---|
| SIFT1M (128-d) | 16 | Good recall@10 trade with sub-millisecond p95. |
| GloVe-200d | 32 | Higher dimensionality benefits from more edges. |

### 3.2 `ef_construction` (build-time candidate list)

Affects build quality. Fixing `M` and increasing `ef_construction`
improves recall, especially recall@1, until it plateaus around 400–
800 for SIFT/GloVe-scale corpora.

### 3.3 `ef_search` (query-time candidate list)

The recall/latency Pareto frontier. The harness sweeps four values
per built index and emits one CSV row per cell. Plot `recall_at_10`
on the y-axis, `latency_p95_us` on the x-axis, group by `(M,
ef_construction)`, and the resulting curves drop directly into the
performance brief.

The Nexus engine's [`KnnIndex::search_knn`](../../crates/nexus-core/src/index/mod.rs)
keeps `ef_search = 50` as the default for compatibility — the bench
binary ([`knn-recall`](../../crates/nexus-knn-bench/src/bin/knn-recall.rs))
calls [`KnnIndex::search_knn_with_ef`](../../crates/nexus-core/src/index/mod.rs)
to override it per cell.

## 4. Output schema

JSON envelope (`docs/performance/data/<corpus>-recall.json`):

```json
{
  "generated_at": "2026-04-29T14:00:00Z",
  "host": "build-host",
  "corpus_kind": "Sift",
  "cells": [
    {
      "m": 16,
      "ef_construction": 200,
      "ef_search": 100,
      "k": 100,
      "corpus_kind": "Sift",
      "base_count": 1000000,
      "query_count": 10000,
      "dim": 128,
      "build_time_seconds": 35.2,
      "recall": {
        "recall_at_1": 0.99,
        "recall_at_10": 0.97,
        "recall_at_100": 0.93,
        "query_count": 10000
      },
      "latency": {
        "mean_us": 240.0,
        "p50_us": 220.0,
        "p95_us": 410.0,
        "p99_us": 680.0,
        "min_us": 110.0,
        "max_us": 1300.0,
        "samples": 10000
      }
    }
  ]
}
```

CSV (`docs/performance/data/<corpus>-recall.csv`) carries the same
columns flattened into one row per sweep cell — drop directly into
your favourite plotting tool.

## 5. Cross-comparable numbers

Other vector DBs publish the same triple (recall@1, recall@10,
recall@100) on the same SIFT1M corpus. To stay apples-to-apples:

- **k = 100** at the engine. Recall@k for k ∈ {1, 10, 100} is computed
  by truncating the same result list. Larger engine `k` does not
  improve recall@1; smaller engine `k` truncates recall@100.
- **Cosine** distance, normalised inputs. Some publications use raw
  L2 — which is equivalent for normalised inputs but produces
  different ground-truth orderings on raw SIFT vectors.
- **Single-thread search**. Concurrency is an engine-level concern
  layered on top, not part of the algorithmic recall claim.

## 6. Source links

- Harness crate: [`crates/nexus-knn-bench`](../../crates/nexus-knn-bench/README.md)
- Engine entry point: [`KnnIndex::search_knn_with_ef`](../../crates/nexus-core/src/index/mod.rs)
- Download script: [`scripts/benchmarks/download_knn_corpora.sh`](../../scripts/benchmarks/download_knn_corpora.sh)
- Latency-only KNN benchmarks (existing): [`docs/performance/PERFORMANCE_V1.md`](./PERFORMANCE_V1.md)
- Comparative bench harness (Nexus ↔ Neo4j): [`crates/nexus-bench`](../../crates/nexus-bench/)
