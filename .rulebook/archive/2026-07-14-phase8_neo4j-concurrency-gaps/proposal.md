# Proposal: phase8_neo4j-concurrency-gaps

## Why

The 2026 re-baseline (`docs/performance/BENCHMARK_2026.md`, produced by
`phase7_benchmark-rebaseline` on clean data) shows Nexus **leading 84% of
comparable serial scenarios** — but it also pinpoints exactly where we still
lose to Neo4j 5.26. Every remaining loss is concentrated in high-concurrency
scaling plus three minor serial scenarios:

### Concurrency losses (the real gaps — from `bench-out/concurrent-summary.md`)

| Scenario | Engine | 1w qps | 16w qps | 64w qps | 64w p99 |
|---|---|---:|---:|---:|---:|
| `aggregation.count_all` | nexus | 546 | 2,508 | **2,876** | **124 ms** |
| `aggregation.count_all` | neo4j | 533 | 6,145 | **13,095** | 11 ms |
| `traversal.small_two_hop_from_hub` | nexus | 1,047 | 6,631 | **8,095** | 11.3 ms |
| `traversal.small_two_hop_from_hub` | neo4j | 521 | 6,421 | **13,202** | 11.1 ms |
| `write.merge_singleton` | nexus | 2,202 | 2,507 | **2,530** | 28.8 ms |
| `write.merge_singleton` | neo4j | 480 | 6,227 | **12,882** | 11.1 ms |
| `point_read.by_id` | nexus | 3,525 | 25,087 | 34,764 | 3.1 ms (wins, but 16→64w only +39%) |

1. **`aggregation.count_all` collapses under concurrency** — flat from 16
   workers (2.5k → 2.9k qps) with p99 exploding to 124 ms while Neo4j
   reaches 13k. At 1–4 workers Nexus is at parity, so the aggregation READ
   path is serializing on something the phase5 lock-free routing was
   supposed to bypass (hypotheses: the COUNT/aggregation path still takes
   the engine lock; the label-bitmap COUNT shortcut sits behind an exclusive
   lock; a shared mutex inside the executor's aggregation).
2. **`traversal.small_two_hop_from_hub` scaling ceiling** — Nexus wins to
   16 workers, then saturates (7.7x total scaling vs Neo4j's 25.3x),
   losing 8.1k vs 13.2k at 64w. Suspects: storage/page-cache lock
   contention in the expand walk, spawn_blocking pool sizing, allocation
   churn per hop.
3. **Concurrent writes flatline at ~2.5k qps** from 4 workers (single-writer
   engine lock — by design), while Neo4j reaches 12.9k. The MODEL stays
   single-writer, but the critical section can shrink: group-commit/WAL
   batching (the deferred bottleneck #7), moving parse/plan outside the
   exclusive lock, batching executor refresh.
4. **`point_read.by_id` self-saturation** — already 2.9x ahead of Neo4j at
   64w, but Nexus's own curve flattens 16→64w (+39% for 4x workers);
   secondary target (tokio/spawn_blocking pool or session-lookup overhead).

### Serial losses (minor — from `serial-74.md`)

5. `procedure.db_labels` 1.80x and `procedure.db_relationship_types` 1.96x
   slower — trivial catalog introspection that should be near-instant;
   likely routed through the exclusive engine path or rebuilding state
   per call.
6. `filter.score_gt_half` 1.21x — borderline; unindexed predicate
   evaluation cost over a label scan (compare with `filter.score_range`
   at 0.93x — the gap is in the comparison-evaluation path, not the scan).

## What Changes

Instrument → fix → re-measure, one gap at a time, using the reproducible
harness from the re-baseline (`scripts/benchmarks/run-vs-neo4j.sh`,
`crates/nexus-bench/examples/`):

- Profile the aggregation-read path at 16/64 workers; eliminate the
  serialization point (route through the lock-free executor clone like
  other reads, or make the COUNT shortcut lock-shared).
- Profile the expand/traversal hot path under concurrency (lock contention
  in storage/page-cache, per-hop allocations); apply the minimal fix the
  profile justifies.
- Shrink the write critical section: parse/plan outside the exclusive
  lock; group-commit WAL batching for concurrent writers; measure the
  single-writer ceiling honestly afterward.
- Fix the two catalog-introspection procedures (cache or lock-free
  catalog reads).
- Investigate the unindexed-comparison filter gap (`score_gt_half`).
- Re-run the concurrency sweep + serial sweep after each fix; update
  `BENCHMARK_2026.md` with a "post-gap-closure" addendum column.

## Impact

- Affected specs: specs/concurrency/spec.md (this task)
- Affected code: `crates/nexus-server` routing/locking; `crates/nexus-core`
  executor aggregation, expand walk, WAL commit path, catalog procedures
- Breaking change: NO (same results, faster under load; single-writer
  semantics preserved)
- User benefit: closes every measured loss vs Neo4j from the 2026
  re-baseline — the concurrency story stops being the one caveat in an
  otherwise winning benchmark report.

## Success criteria (gates, measured with the same harness)

- `aggregation.count_all` @64w: ≥3x current qps (≥8.6k) and p99 < 15 ms
- `traversal.small_two_hop_from_hub` @64w: ≥ Neo4j (≥13.2k qps)
- `write.merge_singleton` @64w: ≥2x current (≥5k qps) without breaking
  single-writer ordering (parity harness + transaction tests stay green)
- `procedure.db_labels` / `db_relationship_types`: ≤1.2x vs Neo4j serial
- Zero regressions in the serial sweep and the full test suites
