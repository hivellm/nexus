## 1. Implementation
- [ ] 1.1 Add an integration test that sends `UNWIND range(0, 9) AS id CREATE (n:Probe {id: id})` and asserts `MATCH (n:Probe) RETURN count(n)` returns 10 — this is currently the failing red case
- [ ] 1.2 Trace why CREATE at `executor/mod.rs:609` sees `result_set.rows=N, variables=[]` — follow the UNWIND operator output into CREATE input
- [ ] 1.3 Fix CREATE to walk `context.result_set.rows` when `variables` is empty but result_set is not
- [ ] 1.4 Remove the silent `WARN ... skipping CREATE` branch and replace with either the correct implementation or an explicit error
- [ ] 1.5 Re-run the Neo4j compatibility test suite (`scripts/test-neo4j-nexus-compatibility-200.ps1`) and reconcile any tests that were passing because of the broken behaviour

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update `docs/specs/cypher-subset.md` to list UNWIND+CREATE as supported
- [ ] 2.2 Add the integration test from 1.1 to `tests/cypher_write_operations_test.rs` (covers single-line UNWIND range and UNWIND with list literal)
- [ ] 2.3 Run tests and confirm they pass
