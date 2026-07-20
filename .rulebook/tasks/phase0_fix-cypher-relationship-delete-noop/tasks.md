# Tasks: phase0_fix-cypher-relationship-delete-noop

Cypher `MATCH (a)-[r:KNOWS]->(b) DELETE r` returns `Ok` but never removes the
relationship: `execute_delete` (executor/operators/expand.rs ~606-621) is a stub
that never calls `storage::delete_rel` (~348-353). See proposal.md for the
empirical evidence.

## 1. Implementation
- [ ] 1.1 Reproduce with a failing test first: `CREATE (a)-[:KNOWS]->(b)`,
  `MATCH (a)-[r:KNOWS]->(b) DELETE r`, then assert `count(r) == 0` and
  `side_effects.relationships_deleted == 1`. Confirm it fails today.
- [ ] 1.2 Trace the `Delete` operator path: how relationship variables reach
  `execute_delete` (executor/dispatch.rs ~756-761), what bindings are available
  (rel_id of each matched `r`), and why the current body clears the result set
  without deleting. Read `storage::record_store_ops::delete_rel` to confirm its
  contract (mark_deleted + write_rel + any index upkeep).
- [ ] 1.3 Wire `execute_delete` to soft-delete each matched relationship binding
  via `delete_rel` (or the engine-level equivalent), unlinking/maintaining the
  relationship index the same way `delete_node_relationships` does. Accumulate
  `relationships_deleted` accurately in `side_effects`.
- [ ] 1.4 Handle DETACH DELETE of a node vs. bare `DELETE r` consistently, and
  make double-delete / non-matching delete an idempotent no-op
  (`relationships_deleted == 0`, no error).
- [ ] 1.5 Confirm parity: the same operator path serves Cypher/REST/RPC/RESP3, so
  verify one integration path per surface (or document that they share the
  operator and one test suffices).

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation (cypher
  subset / DELETE semantics if materially clarified; otherwise waive with note)
- [ ] 2.2 Write tests covering the new behavior: `DELETE r` soft-deletes the
  edge (record `is_deleted()` true, `count(r) == 0`, `relationships_deleted`
  correct); node becomes non-DETACH-deletable afterward; double-delete is a
  clean no-op. Once green, revisit
  `non_detach_delete_allowed_after_outgoing_edge_soft_deleted` in
  `delete_node_dangling_relationships_test.rs` — it currently soft-deletes the
  edge at the storage layer to work around THIS bug and can switch to real
  `DELETE r`.
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` — all green)

## Related
- `phase0_fix-delete-node-dangling-relationships` — the guard that stays
  permanently tripped because "deleted" edges remain live.
- `phase0_perf-delete-node-relationship-check-full-scan` — surfaced this bug.
