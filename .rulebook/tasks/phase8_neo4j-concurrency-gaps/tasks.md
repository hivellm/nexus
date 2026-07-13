## 1. Aggregation-read concurrency collapse (count_all: 2.9k qps / p99 124ms @64w vs Neo4j 13k)
- [ ] 1.1 Reproduce + profile at 16/64 workers; identify the serialization point (engine lock on the aggregation path? COUNT shortcut behind an exclusive lock? executor-internal mutex)
- [ ] 1.2 Fix: aggregation reads take the same lock-free executor-clone path as other autocommit reads (or make the shortcut lock-shared); correctness guarded by the existing count/MVCC tests
- [ ] 1.3 Gate: count_all @64w ≥8.6k qps, p99 <15ms (re-run the concurrency sweep)

## 2. Traversal scaling ceiling (two_hop: 8.1k qps @64w vs Neo4j 13.2k)
- [ ] 2.1 Profile the expand walk under 64-worker load (storage/page-cache lock contention, per-hop allocations, spawn_blocking pool)
- [ ] 2.2 Apply the minimal fix the profile justifies; results identical (traversal correctness tests)
- [ ] 2.3 Gate: two_hop @64w ≥13.2k qps (≥ Neo4j)

## 3. Concurrent-write ceiling (merge_singleton flat at ~2.5k qps from 4w)
- [ ] 3.1 Shrink the exclusive critical section: parse/plan outside the engine write lock where safe
- [ ] 3.2 Group-commit/WAL batching for concurrent writers (bottleneck #7 from docs/nexus/03, previously out of 2.5.0 scope — now in scope here); single-writer ordering preserved (parity harness + transaction tests green)
- [ ] 3.3 Gate: merge_singleton @64w ≥5k qps with p99 <15ms

## 4. Serial stragglers
- [ ] 4.1 `procedure.db_labels` (1.80x) + `procedure.db_relationship_types` (1.96x): make catalog introspection lock-free/cached; gate ≤1.2x vs Neo4j
- [ ] 4.2 `filter.score_gt_half` (1.21x): profile the unindexed-comparison evaluation vs the 0.93x `score_range` sibling; close the delta
- [ ] 4.3 Secondary: investigate point_read.by_id 16→64w self-saturation (+39% for 4x workers) — pool sizing / session lookup

## 5. Re-measure
- [ ] 5.1 Re-run the full concurrency sweep + serial sweep with the phase7 harness; add a "post-gap-closure" addendum to `docs/performance/BENCHMARK_2026.md` with before/after columns

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
