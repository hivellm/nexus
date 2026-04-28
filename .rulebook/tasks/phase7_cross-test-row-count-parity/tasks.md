## 1. Audit
- [ ] 1.1 Re-run the 74-test cross-bench, save full per-test diff
- [ ] 1.2 Classify each of the 22 incompatible tests into one of: OPTIONAL-NULL-row, WITH-projection, Write-success-row, ORDER-tie-stable, other
- [ ] 1.3 Document per-category root cause in `docs/analysis/nexus/04_neo4j_compatibility.md` appendix

## 2. Implementation
- [ ] 2.1 Add `neo4j_strict_rows` config flag (default true)
- [ ] 2.2 Fix OPTIONAL MATCH NULL-row emission to wrap when subquery has zero results
- [ ] 2.3 Fix WITH projection grouping carry-through in chained WITH
- [ ] 2.4 Fix write operations to emit Neo4j-style success row when no RETURN
- [ ] 2.5 Fix ORDER BY tie-stability to match Neo4j's iteration order
- [ ] 2.6 Add per-category regression tests
- [ ] 2.7 Re-run 74-test bench — target ≥ 70/74 row-identical
- [ ] 2.8 Confirm Neo4j diff-suite still passes 300/300
- [ ] 2.9 Document changed shapes in CHANGELOG with migration note for clients

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 3.1 Update or create documentation covering the implementation
- [ ] 3.2 Write tests covering the new behavior
- [ ] 3.3 Run tests and confirm they pass
