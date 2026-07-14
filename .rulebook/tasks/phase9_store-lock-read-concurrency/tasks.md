## 1. Profile the read ceiling (evidence before any structural change)
- [ ] 1.1 Reproduce two_hop @64w on the bench dataset; capture the ACTUAL serialization point with evidence (timing spans around each lock acquisition, allocation counts, or a sampling profiler) — not an assumption
- [ ] 1.2 Report the ranked cost breakdown (store RwLock vs property_store vs label-index vs catalog LMDB read-txn vs allocation vs spawn_blocking pool)

## 2. Fix the read ceiling
- [ ] 2.1 Apply the minimal fix the profile justifies; if it is the global store RwLock, write an ADR choosing lock-sharding vs arc-swap snapshot vs lock-free mmap read path
- [ ] 2.2 Gate: two_hop @64w ≥13.2k qps (≥ Neo4j); results identical (traversal correctness tests)

## 3. Write ceiling (merge_singleton ~2.5k qps from 4w)
- [ ] 3.1 Move parse/plan outside the exclusive engine lock (verify the engine does not re-parse inside the lock after the handler already parsed for routing)
- [ ] 3.2 Evaluate group-commit/WAL batching for concurrent writers; single-writer ordering preserved (parity harness + transaction tests green)
- [ ] 3.3 Gate: merge_singleton @64w ≥5k qps, p99 <15ms

## 4. Serial stragglers
- [ ] 4.1 Confirm phase8's db_schema.rs procedure refactor lands the gate ≤1.2x vs Neo4j for db.labels/db.relationshipTypes with a fresh measurement; finish if short
- [ ] 4.2 `filter.score_gt_half` (1.21x): diff the `>` predicate path vs the 0.93x `score_range` sibling; close to ≤1.05x or explain the cause
- [ ] 4.3 Investigate the ~100ms periodic outlier (once per ~100 rel creates) flagged in BENCHMARK_2026 — WAL fsync/checkpoint or page-cache eviction

## 5. Re-measure
- [ ] 5.1 Re-run the full concurrency + serial sweep; update the BENCHMARK_2026 addendum before/after columns with the closed gates

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
