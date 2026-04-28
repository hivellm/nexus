## 1. Cache implementation
- [ ] 1.1 Implement query-text canonicaliser (strip comments, normalise whitespace) in `executor/planner/cache.rs`
- [ ] 1.2 Implement `LRU<u64, Arc<PhysicalPlan>>` keyed by xxh3 of canonical query
- [ ] 1.3 Add `NEXUS_PLAN_CACHE_ENTRIES` env var (default 1024) + `NEXUS_PLAN_CACHE_DISABLE` escape hatch
- [ ] 1.4 Wire cache lookup at the start of `Engine::execute`
- [ ] 1.5 Wire cache populate after successful plan

## 2. Invalidation
- [ ] 2.1 Add a `planner_generation: AtomicU64` counter
- [ ] 2.2 Bump generation on CREATE/DROP INDEX, CREATE/DROP CONSTRAINT, label/type/key registry changes
- [ ] 2.3 Cache entries store the generation at populate time; lookup compares generation, evicts stale
- [ ] 2.4 Add full-flush API for ops emergencies

## 3. Procedures
- [ ] 3.1 Implement `db.planCache.list()` (top N by hit count)
- [ ] 3.2 Implement `db.planCache.clear()` (admin-gated)
- [ ] 3.3 Implement `db.planCache.stats()` (hit / miss / eviction / size)

## 4. Observability
- [ ] 4.1 Surface cache counters on `/stats` JSON
- [ ] 4.2 Add Prometheus metrics: `nexus_plan_cache_hits_total`, `_misses_total`, `_evictions_total`, `_size`

## 5. Bench + tests
- [ ] 5.1 Bench hot-endpoint workload: cold vs warm cache, measure latency delta
- [ ] 5.2 Test correctness: same query + different params → cache hit, correct results
- [ ] 5.3 Test invalidation: schema change evicts dependent plans
- [ ] 5.4 Test concurrency: 64 concurrent queries against a populated cache → no race

## 6. Documentation
- [ ] 6.1 Document plan-cache section in `docs/specs/cypher-subset.md`
- [ ] 6.2 Document `db.planCache.*` procedures
- [ ] 6.3 CHANGELOG entry

## 7. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 7.1 Update or create documentation covering the implementation
- [ ] 7.2 Write tests covering the new behavior
- [ ] 7.3 Run tests and confirm they pass
