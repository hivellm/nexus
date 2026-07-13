# Nexus vs Neo4j Benchmark Report — 2026 (canonical)

> This is the canonical, current benchmark reference for Nexus.
> It supersedes `BENCHMARK_NEXUS_VS_NEO4J.md` (Dec 2025, v0.12.0) and the
> `BENCHMARK_RESULTS_PHASE9.md` snapshot — both are kept for historical
> continuity but their headline numbers must not be quoted going forward.
> Produced by `phase7_benchmark-rebaseline` to resolve the contradictory
> CREATE-relationship numbers between those two reports (section 3 below)
> and to publish one trustworthy, reproducible baseline after the phase5
> (lock-free reads) and phase6 (traversal/aggregation fast paths) work.

## 1. Test identity

| Field | Value |
|---|---|
| Date | 2026-07-13 |
| Nexus version | workspace Cargo.toml `2.4.0`, branch `release/2.5.0`, commit `9c02d7ce` (treat as 2.5.0-dev — the version string has not been bumped yet on this branch) |
| Neo4j version | `neo4j:5` Docker image, resolved to Neo4j 5.26.28 Community Edition, digest `sha256:4bae36aff76271e27fd6a6ed0835413f86a284cd179cfb1cb7d188f5f7533ac` |
| Nexus binary | local `cargo +nightly build --release -p nexus-server` binary, run directly on the host (not the `hivehub/nexus:2.5.0-dev-amd64` Docker image) — matches how the phase9 / Dec-2025 reports ran, lowest transport overhead |
| Neo4j runtime | `docker run --name nexus-bench-neo4j -p 7474:7474 -p 7687:7687 -e NEO4J_AUTH=neo4j/password -e NEO4J_server_memory_heap_max__size=2G neo4j:5` |
| Host OS | Windows 10 Pro 10.0.19045 (MINGW64_NT-10.0-19045 3.6.7) |
| CPU | AMD Ryzen 9 7950X3D, 16 cores / 32 threads |
| RAM | ~136 GB |
| Rust toolchain | `cargo 1.95.0-nightly (f298b8c82 2026-02-24)` |
| JVM (Neo4j container) | OpenJDK bundled in the `neo4j:5` image |
| Machine load | Quiet — no other benchmark or build running during measurement windows |

Raw machine-readable outputs live under `bench-out/` (gitignored) and
`docs/performance/data/` (checked in): `environment.txt`, `serial-74.json`,
`serial-74.md`, `concurrent-combined.json`, `concurrent-1.json`,
`concurrent-4.json`, `concurrent-16.json`, `concurrent-64.json`,
`concurrent-summary.md`, `create-rel-verdict.csv`, `create-rel-verdict.json`,
`sift1m-recall.json`, `sift1m-recall.csv`.

## 2. Reproduction

```bash
# 1. Build release binaries.
cargo +nightly build --release -p nexus-server -p nexus-bench -p nexus-knn-bench \
  --features "nexus-bench/live-bench nexus-bench/neo4j"

# 2. Start Neo4j.
docker run -d --name nexus-bench-neo4j -p 7474:7474 -p 7687:7687 \
  -e NEO4J_AUTH=neo4j/password -e NEO4J_server_memory_heap_max__size=2G neo4j:5

# 3. Start Nexus (release binary, RPC + RESP3 enabled).
NEXUS_DATA_DIR=./bench-data/nexus-vs-neo4j \
NEXUS_ADDR=127.0.0.1:15474 \
NEXUS_RPC_ENABLED=true NEXUS_RPC_ADDR=127.0.0.1:15475 \
NEXUS_RESP3_ENABLED=true NEXUS_RESP3_ADDR=127.0.0.1:15476 \
./target/release/nexus-server

# 4. Load the seed dataset on BOTH engines exactly once (loading twice
#    double-seeds fixture data and trips the harness's row-count
#    divergence guard on point-read/write scenarios).
./target/release/nexus-bench --rpc-addr 127.0.0.1:15475 --i-have-a-server-running \
  --load-dataset --compare --neo4j-url bolt://127.0.0.1:7687 \
  --neo4j-user neo4j --neo4j-password password \
  --format json --output bench-out/_seed.json

# 5. Run the full suite (serial scenarios + real concurrent sweep).
NEXUS_RPC_ADDR=127.0.0.1:15475 NEXUS_HTTP_URL=http://127.0.0.1:15474 \
NEO4J_URL=bolt://127.0.0.1:7687 NEO4J_PASSWORD=password OUT_DIR=bench-out \
bash scripts/benchmarks/run-vs-neo4j.sh

# 6. CREATE-relationship dedicated verdict (section 3).
REPS=5 OPS_PER_REP=100 OUT_DIR=bench-out \
bash scripts/benchmarks/create-rel-verdict.sh
```

## 3. Serial scenario sweep (100 scenarios, single-threaded)

The seed catalogue in `crates/nexus-bench/src/scenarios/` has grown from the
74 scenarios the orchestrator script's docstring still names to **100**
across 16+ categories. Full detail: `bench-out/serial-74.md`
(machine-readable: `bench-out/serial-74.json`, `schema_version: 1`).

Classification (Nexus-p50 / Neo4j-p50 ratio; lower is better for Nexus):
Lead <0.8x, Parity 0.8x-1.2x, Behind 1.2x-2x, Gap >2x.

| Classification | Count | Share of comparable scenarios |
|---|---:|---:|
| Lead (Nexus <0.8x Neo4j) | 48 | 84% |
| Parity (0.8x-1.2x) | 6 | 11% |
| Behind (1.2x-2x) | 3 | 5% |
| Gap (>2x) | 0 | 0% |
| n/a (Neo4j has no equivalent — APOC/RTREE/bytes extensions, or one side errored) | 43 | — |

57 scenarios had a runnable Neo4j comparison; of those, **54/57 (95%) are Lead
or Parity**, none are a Gap. This is a materially different picture than the
Dec-2025 report's "73/74 Nexus faster" framing because the catalogue has
grown to include APOC-shaped and extension scenarios Neo4j Community cannot
run at all (marked n/a, not "Neo4j faster").

### 3.1 Where Nexus leads (representative, not exhaustive — see serial-74.md)

| Scenario | Nexus p50 (us) | Neo4j p50 (us) | Ratio |
|---|---:|---:|---:|
| `point_read.by_id` | 250 | 2305 | 0.11x |
| `label_scan.count_a` | 309 | 2514 | 0.12x |
| `write.merge_singleton` | 383 | 2085 | 0.18x |
| `write.create_singleton` | 1067 | 4034 | 0.26x |
| `traversal.small_two_hop_from_hub` | 863 | 2334 | 0.37x |
| `hybrid.vector_plus_graph` | 236 | 2377 | 0.10x |
| `constraint.unique_insert` | 487 | 2185 | 0.22x |

### 3.2 Parity

| Scenario | Nexus p50 (us) | Neo4j p50 (us) | Ratio |
|---|---:|---:|---:|
| `filter.score_range` | 2131 | 2290 | 0.93x |
| `order.bottom_5_by_score` | 2066 | 2241 | 0.92x |
| `order.top_5_by_score` | 1979 | 2179 | 0.91x |
| `point_read.by_name` | 2040 | 2403 | 0.85x |
| `traversal.one_hop_from_zero` | 2475 | 2288 | 1.08x |
| `traversal.two_hop_chain` | 2529 | 2326 | 1.09x |

### 3.3 Behind (the honest gaps — down from the phase9 41-57% traversal gap)

| Scenario | Nexus p50 (us) | Neo4j p50 (us) | Ratio | Note |
|---|---:|---:|---:|---|
| `filter.score_gt_half` | 2503 | 2070 | 1.21x | Just over the Parity boundary |
| `procedure.db_labels` | 4186 | 2332 | 1.80x | Catalog-introspection procedure, not core query path |
| `procedure.db_relationship_types` | 4043 | 2063 | 1.96x | Same family as above |

No traversal scenario is classified Behind or Gap any more — the phase6
relationship type pre-filter and phase5 lock-free reads closed the
41-57% single-hop gap the phase9 report flagged as the primary
follow-up. `traversal.one_hop_from_zero` / `two_hop_chain` (Tiny dataset,
cold single-shot pattern) sit at Parity (1.08x-1.09x); every Small-dataset
traversal scenario (`small_one_hop_hub`, `small_two_hop_from_hub`,
`small_var_length_1_to_3`) is a clear Lead (0.17x-0.37x).

## 4. Concurrency sweep (1 / 4 / 16 / 64 workers, real numbers)

Prior generations of this report shipped a placeholder here (the
orchestrator wrote a `TODO-fill-with-real-numbers` stub with empty
`rows: []` — see git history of `scripts/benchmarks/run-vs-neo4j.sh`
before this rebaseline). This run replaces the stub with a new
`crates/nexus-bench/examples/concurrent_sweep.rs` binary that drives
`nexus_bench::concurrent::run_concurrent` against both engines for real,
using the existing `NexusRpcClient` / `Neo4jBoltClient` bench clients (one
per worker thread, independent connections). 15s measured window + 2s
warmup per cell. Full data: `bench-out/concurrent-combined.json`,
`bench-out/concurrent-summary.md`.

| Scenario | Engine | 1w qps | 4w qps | 16w qps | 64w qps | 1w-\>64w scaling |
|---|---|---:|---:|---:|---:|---:|
| `point_read.by_id` | nexus | 3525 | 9764 | 25087 | 34764 | **9.9x** |
| `point_read.by_id` | neo4j | 489 | 1887 | 6330 | 12133 | 24.8x |
| `traversal.small_two_hop_from_hub` | nexus | 1047 | 3126 | 6631 | 8095 | 7.7x |
| `traversal.small_two_hop_from_hub` | neo4j | 521 | 1911 | 6421 | 13202 | 25.3x |
| `aggregation.count_all` | nexus | 546 | 1681 | 2508 | 2876 | 5.3x |
| `aggregation.count_all` | neo4j | 533 | 1761 | 6145 | 13095 | 24.6x |
| `write.merge_singleton` | nexus | 2202 | 2774 | 2507 | 2530 | 1.1x (flat) |
| `write.merge_singleton` | neo4j | 480 | 1760 | 6227 | 12882 | 26.8x |

### 4.1 Reading the shape

- **Reads scale, and Nexus starts from — and stays at — a much higher
  floor.** `point_read.by_id` at 64 workers: Nexus 34.8K qps vs Neo4j
  12.1K qps (2.9x). This is the phase5 lock-free-read-path fix
  (`~2.6x` measured in isolation, see `docs/nexus/03-performance.md`)
  showing up end-to-end against a real Neo4j instance, not just the
  synthetic 8-client test the phase5 task used.
- **Nexus reads scale sub-linearly past 16 workers** (9.9x raw
  throughput for a 64x worker increase on `point_read.by_id`) —
  session-map lookup and `spawn_blocking` scheduling overhead, exactly
  the residual cost flagged as unresolved in the phase5 retrospective.
  Neo4j's own scaling is also sub-linear (24.8x of 64x) but from a much
  lower floor, so Nexus's absolute qps still leads at every level tested.
- **`aggregation.count_all` and `traversal` degrade or plateau past 16
  workers on Nexus** while Neo4j keeps climbing — at 64 workers Neo4j
  overtakes Nexus on both (13095 vs 2876 for aggregation; 13202 vs 8095
  for traversal). `aggregation.count_all` at 64 workers shows p50=17.7ms,
  p99=124ms — a real tail-latency regression under load, not present at
  16 workers (p50=6.5ms). This scenario and the traversal one are
  MATCH-shaped autocommit reads that DO qualify for the phase5 lock-free
  path, so the degradation is not the engine write-lock; it points at a
  different contention source (label-bitmap / adjacency-list read
  contention, or `spawn_blocking` thread-pool saturation) worth a
  dedicated follow-up profiling pass.
- **Writes do not scale past ~4 workers on Nexus — by design.**
  `write.merge_singleton` plateaus at ~2500-2800 qps from 4 to 64
  workers (single-writer transaction model — every write takes the
  exclusive engine lock, unaffected by the phase5 fix, which only
  routes read-only autocommit queries around that lock). Neo4j's write
  path keeps scaling to 12.9K qps at 64 workers. This is an honest,
  expected gap given Nexus's V1 single-writer architecture
  (`docs/nexus/03-performance.md` bottleneck #1) and a concrete
  argument for the write-side follow-up work tracked there.

## 5. CREATE-relationship contradiction — resolved

Prior reports disagreed by two orders of magnitude on the same
operation:

| Report | Nexus | Neo4j | Verdict claimed |
|---|---:|---:|---|
| `BENCHMARK_RESULTS_PHASE9.md` (2025-11-20) | 25.38ms | 3.14ms | Nexus **87.6% slower** |
| `BENCHMARK_NEXUS_VS_NEO4J.md` (2025-12-01, v0.12.0, self-flagged stale) | 2.85ms | 121.93ms | Nexus **42.7x faster** |

Neither number is reproducible from the current codebase: the live
`crates/nexus-bench/src/scenarios/write.rs` seed catalogue has **no
CREATE-relationship scenario at all** (every write scenario there is
deliberately idempotent-safe — see the file's header comment — a
"create one edge per iteration" scenario would grow the graph every
iteration and trip the harness's row-count divergence guard). Both
historical numbers came from a harness generation that no longer
exists in this form, most likely measuring different things entirely
(a cold single first-ever write including JVM/query-plan warmup vs. a
batch of already-warm repeated writes) rather than a real regression or
improvement in Nexus itself.

### 5.1 Dedicated repeated-run measurement

New script: `scripts/benchmarks/create-rel-verdict.sh`. Methodology:
5 independent repetitions, 100 `CREATE (a:CreateRelBench)-[:REL]->(b:CreateRelBench)
RETURN 1` statements per repetition, timed individually via
`curl -w '%{time_total}'` over each engine's REST/HTTP transport (both
engines expose HTTP, so this is the one apples-to-apples transport
available — Nexus's native RPC and Neo4j's Bolt are different wire
protocols and would not isolate engine work from driver overhead the
same way). Median, min, max, and mean computed per repetition; the
grand verdict is the median of the 5 per-repetition medians (robust to
a single bad repetition), with the spread across those 5 medians
reported so the result is not a lucky single sample.

Raw data: `bench-out/create-rel-verdict.csv`, `bench-out/create-rel-verdict.json`.

| Engine | Rep 1 | Rep 2 | Rep 3 | Rep 4 | Rep 5 | Grand median | Spread across reps |
|---|---:|---:|---:|---:|---:|---:|---:|
| Nexus | 2.276ms | 2.275ms | 2.348ms | 2.447ms | 2.384ms | **2.348ms** | 0.172ms |
| Neo4j | 5.858ms | 5.699ms | 5.628ms | 5.527ms | 5.412ms | **5.628ms** | 0.446ms |

**Verdict: Nexus is 2.40x faster than Neo4j for CREATE-relationship over
HTTP** (Neo4j/Nexus grand-median ratio = 2.397). This is directionally
consistent with the Dec-2025 report's "Nexus faster" claim but at a much
more modest, and far more reproducible, magnitude than that report's 42.7x
— and it directly contradicts the phase9 report's "87.6% slower" claim.
Neither historical number should be quoted going forward; this
measurement, run against the current release/2.5.0 codebase with a
documented, re-runnable methodology, is the one to cite.

One observed artifact worth flagging honestly rather than hiding: every
single Nexus repetition had one outlier request at ~102ms (visible as
the `max` column in the raw CSV, consistent across all 5 reps to within
0.4ms) — a periodic ~100ms stall roughly once per 100 relationship
creates. The median is unaffected (only 1 sample in 100 is hit), but
this is a concrete, reproducible signal worth a dedicated follow-up
(candidate causes: periodic WAL fsync/checkpoint, page-cache eviction
batch, or a GC-equivalent compaction pass) — filed as a note here rather
than a full investigation, which is out of scope for a benchmark
rebaseline task.
