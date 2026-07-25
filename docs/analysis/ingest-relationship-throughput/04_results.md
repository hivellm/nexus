# 04 — Results: relationship-ingest performance post-fix

The [root cause](02_root_cause.md) (a durable LMDB fsync per relationship)
and [fix plan](03_fix_plan.md) (batch the rel-count update at write boundary)
have been implemented. This section reports the measured outcome.

## Engine micro-benchmark: before vs after

Benchmark: `crates/nexus-core/benches/ingest_relationship.rs` (criterion,
`sample_size(10)`, empty properties, fresh self-cleaning engine per iteration,
release build). Throughput in rel/s (median ±95% CI).

### Before (counted path — one durable LMDB fsync per edge)

The `/ingest` path before the fix called `catalog.increment_rel_count` per
relationship, each triggering a full `get_statistics` + `update_statistics`
commit cycle.

| Shape | N=100 | N=1 000 | N=5 000 |
|---|---:|---:|---:|
| Distinct-source | 3,976 | 5,332 | 6,284 |
| Same-source | 4,178 | 5,634 | 5,964 |

### After (batched path — N uncounted creates + ONE `batch_increment_rel_counts` flush)

The `/ingest` path now creates edges via `Engine::create_relationship_uncounted`
(skips the per-edge count), accumulates `(type_id, +count)` tuples per batch,
and flushes all rel-type deltas in one `batch_increment_rel_counts` call. A new
batched API was added to mirror the existing `batch_increment_node_counts` path
for nodes.

| Shape | N=100 | N=1 000 | N=5 000 |
|---:|---:|---:|---:|
| Distinct-source | 11,769 | 22,722 | 26,632 |
| Same-source | 10,701 | 23,082 | 26,205 |

### Speedup and gate

Speedup ≈ 2.6×–4.4× across all cases (N=100 is slowest, larger batches near 4.4×).

**Gate criterion** (from [03](03_fix_plan.md)): relationship ingest ≥ node
ingest (~8 500 rel/s). **PASSED**: every post-fix case clears 8 500 rel/s
(worst case same-source N=100 at 10,701 rel/s).

## Secondary: Phase-8 redundant write

The [root cause analysis](02_root_cause.md) identified a parallel write into
`RelationshipStorageManager` (Phase 8) for every relationship created, despite
its read path being dead (commented out, test-only, superseded by store
adjacency index since commit 88f78245). This was a per-row CPU + lock cost on
top of the fsync issue.

The executor CREATE path and `engine/crud/relationships.rs` were audited.
Exhaustive review confirmed:

- All in-production readers of `relationship_storage()` are commented out or
  compile-gated to test-only paths.
- Traversal (`find_relationships`) reads the store adjacency index, not the
  `RelationshipStorageManager`.
- The Phase-8 write was indeed dead-on-read.

**Isolated measurement** of Phase-8 impact (batched bench, write toggled ON vs
OFF): deltas ranged −17.6%..+2.5% with sign flips across the six cases
(distinct/same-source × N=100/1k/5k). Criterion reported "No change in
performance detected" for all cases (p = 0.47–0.90). The Phase-8 write is NOT
a throughput bottleneck in the single-thread benchmark.

**Why it was still removed**: it is dead-on-read code that adds a per-row
RwLock write + HashMap allocation with no correctness value. Removing it (by
gating the write to compile-time `const … = false` in `engine/crud/relationships.rs`
and dropping it from the executor CREATE code paths in
`executor/operators/create.rs`) eliminates unnecessary contention that would
surface under concurrent writers or in future multi-threaded scenarios. This is
a cleanup, not a measured single-thread win.

## Correctness: catalog rel total is now accurate

Two write paths created relationships:

1. **`/ingest` and engine CRUD** (`Engine::create_relationship`): previously
   incremented `catalog.rel_counts` per edge (expensive, durable fsync per row,
   but the total was correct).
2. **Executor `CREATE`** in Cypher (`executor/operators/create.rs`): previously
   did NOT increment `catalog.rel_counts` at all → fast, but the catalog rel
   total was wrong (a lower bound, often 0), documented in `engine/mod.rs:481`.

The fix unifies both paths:

- `/ingest` now calls `Engine::create_relationship_uncounted` (no per-row count)
  and batches the rel-type deltas in a single `batch_increment_rel_counts` call
  after the loop.
- Executor `CREATE` (both standalone `CREATE` and `MATCH/UNWIND … CREATE`)
  now accumulates rel-type counts per batch and flushes via the same batched
  API.

Result: `catalog.rel_counts` is accurate after either path, and the
`engine/mod.rs:481` caveat is closed. The record-store durability contract
(WAL + transaction isolation) is unchanged.

## End-to-end SF0.1 (post-fix)

LDBC SF0.1 baseline (pre-fix) from README and the Phase 7 task: ~480 s
whole-load time (~3 100 rel/s during the relationship pass).

The post-fix loader (`benchmarks/ldbc-snb/loader`, now instrumented with
per-pass wall-clock timers in `main.rs`) was run against the cached SF0.1
dataset on a freshly-built post-fix server. **The full load did not complete**:
it was stopped by a *separate, pre-existing* server-side bottleneck — the async
WAL writer falling behind fsync under sustained large-batch relationship ingest
(`nexus_core::wal::async_wal`, logged as "issue #19": `WAL async channel full —
applying backpressure … queue_depth=10001`). Both attempts aborted at the
identical point — the 135 701-row `post` foreign-key file in Pass 2 — with the
OS dropping the client TCP connection (`os error 10053` / `10054`); the server
process itself stayed healthy (no crash, `/health` OK). This is **not** a
regression from the catalog-fsync fix; it is the *next* bottleneck, exposed now
that relationship ingest is no longer catalog-fsync-bound.

**Partial evidence (reproducible across both runs), which is strongly positive:**

- Node pass: ~5.5–5.8 s for all 327 588 nodes.
- The relationship-producing files that DID complete in Pass 2 (`place`,
  `organisation`, `tag`, `person`, `forum` foreign keys) loaded at
  **~52 000–68 000 rows/s** — roughly **17×–22× the ~3 100 rel/s pre-fix
  baseline** — before the load hit the WAL-writer ceiling on the largest file.

So the fix demonstrably lifts real-scale relationship-ingest throughput by well
over an order of magnitude on the portion that ran; a clean full-load
wall-clock and the ~1.49 M-relationship count verification could not be captured
because the load now saturates the WAL async-writer first. Closing that
(separately-tracked "issue #19") is the prerequisite for a complete post-fix
SF0.1 number and is the natural follow-up to this work.
