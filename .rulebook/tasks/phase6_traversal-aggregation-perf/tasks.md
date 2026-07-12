## 1. COUNT(*) metadata shortcut (smallest, ship first)
- [ ] 1.1 Bench baseline: unfiltered `MATCH (n) RETURN count(n)` and `MATCH (n:L) RETURN count(*)` at 10k/100k/1M nodes
- [ ] 1.2 Answer unfiltered COUNT from label-bitmap/catalog cardinality; predicate or grouped cases fall back to scan
- [ ] 1.3 Correctness tests: matches scan result incl. after deletes and inside transactions (MVCC visibility)

## 2. Relationship-type pre-filter
- [ ] 2.1 Bench baseline: single-hop typed traversal on a node with mixed rel types
- [ ] 2.2 Filter on `type_id` during adjacency-list walk before record/property materialization
- [ ] 2.3 Verify `AdvancedTraversalEngine` is used on all traversal entry paths (doc flags inconsistent use)

## 3. GROUP BY sizing
- [ ] 3.1 Pre-size the aggregation HashMap from upstream cardinality estimate

## 4. Statistics-driven planning
- [ ] 4.1 Wire `StatisticsCollector` (label/type cardinality, avg degree) into join-order and Expand-direction costing in `planner/queries/cost.rs`
- [ ] 4.2 Plan-quality tests: representative JOIN-shaped queries pick the lower-cardinality side first (assert on EXPLAIN output)

## 5. Gate
- [ ] 5.1 nexus-bench after: traversal gap cut ≥30%, COUNT(*) ~O(1), GROUP BY ≥25% faster at 100k; no regressions

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation (performance docs before/after)
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass (workspace suite, clippy zero warnings)
