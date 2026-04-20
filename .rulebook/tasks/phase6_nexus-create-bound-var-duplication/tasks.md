## 1. Diagnosis

- [x] 1.1 Write the minimal executor-layer reproducer: `CREATE (a:X {id:1}), (b:X {id:2}), (a)-[:R]->(b)` then `MATCH (n) RETURN count(n)` — asserted count=4 pre-fix (test `create_bound_variable_edge_does_not_duplicate_nodes` in `crates/nexus-core/src/engine/tests.rs`)
- [x] 1.2 Check the AST emitted by the parser — confirmed the bug lives in the CREATE operator's execution phase, not the parser: the AST carries the same Pattern for both the declaration `(a:X)` and the edge-pattern reference `(a)`, and the executor's walker ignored the variable binding
- [x] 1.3 Compare against the equivalent two-statement form — irrelevant once §1.2 localised the bug to `execute_create_pattern_internal`; the fix is the same regardless of whether the pattern arrives as one CREATE or two

## 2. Fix

- [x] 2.1 Apply the fix at whichever layer §1 identified — `execute_create_pattern_internal` in `crates/nexus-core/src/executor/operators/create.rs` now checks `created_nodes` for the pattern's variable before creating a new node, on both the Node branch (source side) and the Relationship target branch. Commit TBD in this PR.
- [x] 2.2 Verify `TinyDataset.load_statement` from `nexus-bench` now produces exactly 100 nodes + 50 relationships on Nexus — confirmed via a live probe (see `phase6_bench-live-test-state-isolation::isolation_between_loads_works`, strengthened to assert the exact 100 + 50 invariant)
- [x] 2.3 Verify the fix does not regress existing CREATE behaviour — `cargo +nightly test -p nexus-core --lib` passes 1722/1722

## 3. Regression coverage

- [x] 3.1 Executor-layer unit tests for the minimal reproducer (`create_bound_variable_edge_does_not_duplicate_nodes`) and a 3-node chain variant (`create_bound_variable_chain_reuses_nodes`)
- [x] 3.2 Integration test that loads `TinyDataset` and asserts `count(n) == 100` and `count(r) == 50` — strengthened assertions in `crates/nexus-bench/tests/live_rpc.rs::isolation_between_loads_works` + `live_compare.rs::isolation_between_tests_works`
- [x] 3.3 Strengthen the bench isolation tests to assert `count == 100` after each load — done (see §3.2)

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 4.1 Update or create documentation covering the implementation — `CHANGELOG.md` under `1.0.0 → Fixed — CREATE with bound-variable edges duplicated nodes (2026-04-20)`
- [x] 4.2 Write tests covering the new behavior — §3 above
- [x] 4.3 Run tests and confirm they pass — 9/9 `#[ignore]` tests pass against a live Nexus + docker Neo4j with `--test-threads=1`; 1722/1722 `nexus-core` lib tests pass
