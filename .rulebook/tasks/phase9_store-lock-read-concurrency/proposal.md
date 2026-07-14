# Proposal: phase9_store-lock-read-concurrency

## Why

Goal (user-directed): Nexus must be **more efficient than Neo4j with the
same functionality**. The 2026 re-baseline
(`docs/performance/BENCHMARK_2026.md`) plus phase8 closed the biggest gap
(`aggregation.count_all` @64w: 2.9k → 62.6k qps, ~5x over Neo4j) but left
the concurrency scenarios where Nexus still trails at 64 workers:

| Scenario @64w | Nexus | Neo4j | family |
|---|---:|---:|---|
| `traversal.small_two_hop_from_hub` | 8,508 qps | 13,202 | read ceiling |
| `write.merge_singleton` | ~2,530 qps | 12,882 | write ceiling |

Plus two serial stragglers: `procedure.db_labels` (1.80x) /
`db_relationship_types` (1.96x), and `filter.score_gt_half` (1.21x).

phase8's per-node lock hoisting helped 1–16w latency but not the 64w
read ceiling. The prior agent ATTRIBUTED the residual to the single
global `ExecutorShared.store` `parking_lot::RwLock`, but that hypothesis
was not profiled to proof — at ~8.5k qps a once-per-query read-guard
acquisition should not saturate a parking_lot RwLock (those sustain
millions/sec), so the real serialization point must be found with
evidence before any structural change is committed.

## What Changes

**Profile first, then fix — no structural change without proof.**

1. Instrument/sample the two_hop read path under 64 concurrent workers
   (timing spans around lock acquisitions, allocation counts, or a
   sampling profiler) to identify the ACTUAL serialization point:
   candidates are the `store` RwLock, the `property_store` lock, the
   label-index lock, the `catalog` (LMDB) read-txn, per-hop allocation
   churn, or the `spawn_blocking` pool. Report the evidence.
2. Apply the minimal fix the profile justifies. If it genuinely is the
   global store RwLock: evaluate lock sharding vs an `arc-swap` snapshot
   of the read view vs a lock-free mmap read path, and record the choice
   in an ADR (`rulebook decision create`).
3. **Write ceiling** (`merge_singleton`): move parse/plan outside the
   exclusive engine lock (the handler already parses the AST for
   routing — verify the engine does not re-parse inside the lock), and
   evaluate group-commit/WAL batching for concurrent writers. Single-
   writer ordering MUST be preserved (parity harness 26/26 + transaction
   tests are the net).
4. **Serial procedures** (`db.labels` / `db.relationshipTypes`): confirm
   phase8's db_schema.rs refactor lands the gate (≤1.2x vs Neo4j) with a
   fresh measurement; finish if not.
5. **`filter.score_gt_half`** (1.21x): diff the `>` predicate evaluation
   path against the 0.93x `score_range` sibling; close or explain.
6. Also worth a look (flagged in BENCHMARK_2026): the periodic ~100 ms
   outlier once per ~100 relationship creates (WAL fsync/checkpoint or
   page-cache eviction batch).

## Impact

- Affected specs: specs/read-concurrency/spec.md (this task)
- Affected code: `crates/nexus-core/src/executor` (store access, scan/
  expand), `crates/nexus-core/src/storage` (record store locking),
  `crates/nexus-server` (write critical section), WAL commit path
- Breaking change: NO (same results, faster under load; single-writer
  ordering preserved)
- User benefit: closes the last scenarios where Neo4j still leads at high
  concurrency — the explicit "beat Neo4j" goal.

## Success criteria (gates, same harness)

- `traversal.small_two_hop_from_hub` @64w ≥ 13.2k qps (≥ Neo4j)
- `write.merge_singleton` @64w ≥ 5k qps, p99 < 15 ms
- `procedure.db_labels` / `db_relationship_types` ≤ 1.2x vs Neo4j serial
- `filter.score_gt_half` ≤ 1.05x vs Neo4j
- Zero regressions in serial sweep + full test suites; parity 26/26
