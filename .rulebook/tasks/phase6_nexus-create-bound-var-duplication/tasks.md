## 1. Diagnosis

- [ ] 1.1 Write the minimal executor-layer reproducer: `CREATE (a:X {id:1}), (b:X {id:2}), (a)-[:R]->(b)` then `MATCH (n) RETURN count(n)` — assert the current output is 4
- [ ] 1.2 Check the AST emitted by the parser — confirm whether the bug is in parsing (variable binding) or in the CREATE operator's execution phase
- [ ] 1.3 Compare against the equivalent two-statement form (`CREATE (a:X),(b:X) WITH a,b CREATE (a)-[:R]->(b)`); if that works, the fix is localised to single-statement CREATE

## 2. Fix

- [ ] 2.1 Apply the fix at whichever layer §1 identified
- [ ] 2.2 Verify `TinyDataset.load_statement` from `nexus-bench` now produces exactly 100 nodes + 50 relationships on Nexus (currently 200 + 50)
- [ ] 2.3 Verify the fix does not regress existing CREATE behaviour on the 300-test Neo4j compat diff suite

## 3. Regression coverage

- [ ] 3.1 Executor-layer unit test for the minimal reproducer
- [ ] 3.2 Integration test that loads `TinyDataset` and asserts `count(n) == 100` and `count(r) == 50`
- [ ] 3.3 Strengthen `phase6_bench-live-test-state-isolation`'s isolation tests to assert `count == 100` after each load (today they only assert the reset contract, not the load count, so they stay green under this bug)

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — patch note in `CHANGELOG.md`; mention the bound-variable binding semantic in the openCypher compat section of `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 4.2 Write tests covering the new behavior — §3 above
- [ ] 4.3 Run tests and confirm they pass — `cargo +nightly test --workspace` + the comparative bench `#[ignore]` suite under a running Nexus + Neo4j
