# Tasks: phase17_fix-duplicate-node-in-write-path-create

Fix silent data corruption: `CREATE (a)-[:R]->(b) SET …` creates a phantom duplicate
of `b` and binds the variable to the orphan. Confirmed empirically (beta=2,
connected=1). Cause: the `Relationship` arm in `write_exec.rs` creates the target node
by peeking `elements.get(i + 1)`, but the loop never skips that index, so the `Node`
arm creates it again. Plain `CREATE` (executor path) is correct and unaffected.

## 1. Pin the bug with failing tests first (TDD)
- [x] 1.1 Failing test (now green): `CREATE (a:Alpha)-[:LINKS]->(b:Beta) SET a.k=1` → exactly ONE `Beta`, reachable via `MATCH (:Alpha)-[:LINKS]->(b:Beta)`. In `tests/regression/create_rel_with_set_phantom_node_test.rs` (TestContext + isolated catalog); confirmed FAILING before the fix (beta=2)
- [x] 1.2 Failing test (now green): after `CREATE (a)-[:R]->(b) SET b.marked=true`, the node reachable THROUGH the relationship carries `marked=true` (was landing on the orphan)
- [x] 1.3 Chained-pattern test (now green): `CREATE (a)-[:R]->(b)-[:S]->(c) SET a.k=1` → one node per variable + connected 3-node chain (the case a naive skip-by-one breaks — `b` is both R's target and S's source)
- [x] 1.4 Working-path guard (green throughout): plain `CREATE (a)-[:R]->(b)` (no SET) still produces one `Beta` — executor route unaffected, fix didn't move the bug

## 2. Fix the element walk
- [x] 2.1 Gave the linear CREATE loop (`crates/nexus-core/src/engine/write_exec.rs`) a skip mechanism so each pattern element produces exactly one node (the `Relationship` arm consumes the target index, the loop no longer re-creates it in the `Node` arm)
- [x] 2.2 Variable binding and `last_node_id` now point at the CONNECTED node the `Relationship` arm created; chained `(a)-[:R]->(b)-[:S]->(c)` handoff verified (1.3 green — `b` created once, serves as S's source via `last_node_id`)
- [x] 2.3 phase14 external-id tests re-verified: the full nexus-core suite (which includes them) passes; with the duplicate gone the first node is the only `_id` candidate — `ext_id_consumed` guard holds
- [x] 2.4 UNWIND+CREATE arm checked: it does NOT have the duplicate defect. A relationship pattern with a trailing write clause (SET) is rejected (errors, no corruption) and without one stays correct; both locked by tests (`unwind_create_with_relationship_element_and_set_is_rejected_not_duplicated`, chained variant, and the no-trailing-write control). No fix needed there

## 3. Assess existing corrupted data
- [x] 3.1 Assessment: deployments that ran the affected shapes CAN hold orphans, so remediation is warranted. Shipped `docs/data-corruption/CREATE-relationship-phantom-target-audit.md` — bug explanation, a conservative detection query (`MATCH (orphan) WHERE NOT (orphan)--()` narrowed by suspected label), and a manual audit/cleanup process with an explicit caution that legitimately-unconnected nodes exist and must be reviewed before deletion. Linked from the CHANGELOG so users can audit

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG `[3.0.0]` `Fixed` entry (the corruption, exact affected query shapes, that plain CREATE was never affected, the fix, + link to the audit doc); `docs/specs/cypher-subset.md` § CREATE note (each pattern element produces exactly one node even with write clauses); the audit doc from 3.1
- [x] 4.2 Write tests covering the new behavior — `tests/regression/create_rel_with_set_phantom_node_test.rs` (7 tests); the section-1 tests were confirmed FAILING before the fix (beta=2, property on orphan) and pass after
- [x] 4.3 Run tests and confirm they pass — `cargo +nightly fmt` + `clippy -p nexus-core --all-targets -- -D warnings` clean; nexus-core suite green (2504 lib + regression/write-path groups; the one intermittent failure was the pre-existing `index::fulltext_registry` Tantivy flake — phase18's target — verified passing 3/3 in isolation, NOT a write-path regression); `engine::tests` (147, incl. CREATE/MERGE) all green
