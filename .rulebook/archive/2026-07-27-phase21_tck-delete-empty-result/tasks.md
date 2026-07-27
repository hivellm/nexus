## 1. Implementation
- [x] 1.1 RETURN-less mutation -> empty result set (query_pipeline.rs delete `else` branch now returns `ResultSet::new(vec![], vec![])`; side_effects still stamped by the top-level wrapper, so stats stay accurate)
- [x] 1.2 RETURN-present mutation unchanged (`DELETE ... RETURN` / `RETURN count(*)` paths untouched)

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [x] 2.1 Update or create documentation covering the implementation (code comment documents the Neo4j/TCK semantics + side_effect flow; behavior change noted for the phase CHANGELOG rollup)
- [x] 2.2 Write tests covering the new behavior (engine/tests/write.rs: return_less_delete_is_empty_with_side_effects + return_less_detach_delete_reports_relationships_deleted)
- [x] 2.3 Run tests and confirm they pass (2/2 new tests pass; full nexus-core suite 4309 passed / 0 failed; clippy/fmt clean; TCK clauses/delete 41 -> 35 failures)
