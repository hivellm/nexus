# Tasks: phase17_fix-duplicate-node-in-write-path-create

Fix silent data corruption: `CREATE (a)-[:R]->(b) SET …` creates a phantom duplicate
of `b` and binds the variable to the orphan. Confirmed empirically (beta=2,
connected=1). Cause: the `Relationship` arm in `write_exec.rs` creates the target node
by peeking `elements.get(i + 1)`, but the loop never skips that index, so the `Node`
arm creates it again. Plain `CREATE` (executor path) is correct and unaffected.

## 1. Pin the bug with failing tests first (TDD)
- [ ] 1.1 Add a failing test: `CREATE (a:Alpha)-[:LINKS]->(b:Beta) SET a.k = 1` produces exactly ONE `Beta`, and that `Beta` is the one reachable via `MATCH (:Alpha)-[:LINKS]->(b:Beta)`. Use the isolated-catalog pattern (`nexus_core::testing::TestContext` + `Engine::with_isolated_catalog`)
- [ ] 1.2 Add a failing test proving the variable binding is wrong today: after `CREATE (a)-[:R]->(b) SET b.marked = true`, the node reachable through the relationship must carry `marked = true` (currently the property lands on the orphan)
- [ ] 1.3 Add a chained-pattern test: `CREATE (a)-[:R]->(b)-[:S]->(c) SET a.k = 1` yields exactly one node per variable and a connected 3-node chain — this is the case a naive skip-by-one fix breaks, since `b` is both a target and the next source
- [ ] 1.4 Add a regression guard for the working path: plain `CREATE (a)-[:R]->(b)` with no SET still produces one `Beta` (proves the executor route stays correct and that the fix did not move the bug)

## 2. Fix the element walk
- [ ] 2.1 Give the linear CREATE loop (`crates/nexus-core/src/engine/write_exec.rs`, ~`:126-210`) a way to skip elements already consumed by the `Relationship` arm — e.g. a `HashSet<usize>` of consumed indices, or restructure the walk to advance past the target node. Each pattern element must produce exactly one node
- [ ] 2.2 Ensure the variable binding and `last_node_id` point at the **connected** node created by the `Relationship` arm, never at a later re-creation; confirm the `last_node_id` handoff still works for chained patterns so `b` can serve as the next relationship's source
- [ ] 2.3 Confirm the phase14 `ext_id_consumed` guard still holds after the restructure — with the duplicate gone, the first node is the only candidate for `_id`, so re-verify the phase14 external-id tests still pass unchanged
- [ ] 2.4 Check the UNWIND+CREATE arm (`write_exec.rs:440-475`) for the same defect — it currently rejects relationship elements outright, so it is likely unaffected, but confirm rather than assume; if it does accept them anywhere, fix it the same way

## 3. Assess existing corrupted data
- [ ] 3.1 Determine whether deployments can be left holding orphans from this bug and, if so, ship a detection query (nodes of a label with no incoming relationship that have an identical-property twin which does) plus guidance on safe cleanup. If assessment concludes no remediation is warranted, record why rather than skipping silently

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update or create documentation covering the implementation (`docs/specs/cypher-subset.md` CREATE semantics if the documented behaviour needs clarifying; CHANGELOG entry describing the corruption and which query shapes were affected, since users may need to audit their data)
- [ ] 4.2 Write tests covering the new behavior (section 1 — they must fail before the fix and pass after; a test that passes on the unfixed code is worthless here)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo +nightly test --workspace`)
