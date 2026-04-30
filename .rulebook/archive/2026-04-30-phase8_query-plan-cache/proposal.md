# Proposal: phase8_query-plan-cache

## Why

Every query submitted to Nexus is **re-parsed and re-planned from scratch**. There is no plan cache and no prepared-statement path. For high-volume RAG endpoints that fire the same parameterised Cypher 1000s of times per second, the parse + plan cost is paid every call, and at scale this becomes meaningful: the Cypher parser was 290× faster post-fix but is still 3.7 ms on a 31.5 KiB query. Neo4j, Memgraph, ArangoDB, and PostgreSQL all cache query plans keyed by parameterised query text. Adding one to Nexus is straightforward and should improve latency on hot endpoints by 5–20 % at no correctness cost.

## What Changes

- Add a process-wide plan cache: `LRU<u64 (xxh3 of canonicalised query), Arc<PhysicalPlan>>` sized via `NEXUS_PLAN_CACHE_ENTRIES` (default 1024).
- Canonicalisation strips comments and normalises whitespace before hashing; parameter values are *not* part of the key (only the query text is).
- Cache invalidation: on schema change (CREATE/DROP INDEX/CONSTRAINT, label/type registry change), bump a generation counter; entries with stale generation are evicted on next lookup.
- Add `db.planCache.list()`, `db.planCache.clear()`, `db.planCache.stats()` procedures.
- Add `NEXUS_PLAN_CACHE_DISABLE=1` escape hatch.
- Surface cache hit/miss/eviction counters on `/stats`.
- Bench hit-path latency improvement vs cold-path on a hot RAG endpoint workload.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` plan-cache section, new `db.planCache.*` procedures in `docs/procedures/`.
- Affected code: `crates/nexus-core/src/executor/planner/cache.rs` (new), invalidation hooks in catalog + index modules.
- Breaking change: NO (transparent).
- User benefit: 5–20 % latency reduction on repeated queries; closer parity with Neo4j / Memgraph / Arango.
