# Tasks: phase0_fix-delete-node-dangling-relationships

Three related defects let a node with live relationships be hard-deleted,
corrupting every subsequent traversal through the dangling edge:

- **C-2a** — the Cypher plain-DELETE guard (`engine/match_exec.rs:138-144`)
  checks only `node_record.first_rel_ptr != 0`, but `create_relationship`
  never sets `first_rel_ptr` on the destination node
  (`storage/record_store_ops.rs:792-805` — it only tracks OUTGOING edges),
  so an incoming-only node passes the guard.
- **C-2b** — `Engine::delete_node` (`engine/crud/nodes.rs:364-402`) has no
  relationship check at all; every direct caller — REST `DELETE /data/nodes`
  (`nexus-server/src/api/data.rs:602-643`), RPC `DELETE_NODE`
  (`nexus-server/src/protocol/rpc/dispatch/graph.rs:130-154`), RESP3
  `NODE.DELETE` (`nexus-server/src/protocol/resp3/command/graph.rs:107-133`)
  — deletes unconditionally.
- **C-2c** — `Expand` (`executor/operators/expand.rs:437-497`) resolves a
  dangling endpoint to `Value::Null` and pushes the row anyway instead of
  skipping it.

Trigger:
```
CREATE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})
MATCH (b:Person{name:'Bob'}) DELETE b        -- guard passes (b.first_rel_ptr==0), b hard-deleted
MATCH (a)-[r:KNOWS]->(b) RETURN a,r,b         -- returns a, live r, b=null; count(r) still counts it forever
```

Order matters: reproduce all three symptoms with failing tests first (§1),
because they share one trigger and a partial fix (e.g. C-2b without C-2c)
still returns a wrong row from step 3 of the trigger even after delete
correctly refuses in isolation. Confirm the mechanism per defect (§2)
before writing the fix (§3), since C-2a and C-2b share a root cause (no
real relationship-existence check) but C-2c is independent (an
operator-level defensive gap) and must not be conflated with it. The
engine-level check (C-2a/C-2b) must land before its callers are
re-verified, since REST/RPC/RESP3 inherit it without code changes.

## 1. Reproduce all three symptoms first
- [x] 1.1 non-DETACH Cypher `MATCH...DELETE` of an incoming-only node errors (C-2a)
      Done: `tests/executor/delete_node_dangling_relationships_test.rs`
      `non_detach_cypher_delete_of_incoming_only_node_errors` (failed pre-fix: succeeded).
- [x] 1.2 non-DETACH delete via engine/REST/RPC errors (C-2b)
      Done: engine covered by the Cypher test above; REST
      `test_delete_node_refuses_incoming_only_node_with_live_relationship`
      (nexus-server api/data.rs) and RPC
      `delete_node_non_detach_refuses_incoming_only_node_with_live_relationship`
      (rpc/dispatch/graph.rs). All pass.
- [x] 1.3 Expand returns zero rows (not a `b=null` row) for a dangling edge (C-2c)
      Done: `expand_skips_dangling_endpoint_instead_of_null_row` (fabricates a
      genuine dangling edge by soft-deleting b's record via the storage layer,
      mirroring the pre-fix hard delete); asserts zero rows / count 0.

## 2. Confirm the mechanism per defect
- [x] 2.1 `first_rel_ptr` tracks OUTGOING only (deliberate in `create_relationship`);
  the bug was `match_exec.rs` treating it as a complete liveness check. Confirmed.
- [x] 2.2 `Engine::delete_node` had no relationship check; REST/RPC/RESP3 called it
  directly. Confirmed (code review traced all three protocols).
- [x] 2.3 `read_node_as_value_with_store` returns `Value::Null` for a deleted node;
  `Expand` already has a non-optional skip branch for the empty-relationship case.
  Confirmed — used as the template for C-2c.

## 3. Implement the fix
- [x] 3.1 Real relationship-existence check in `Engine::delete_node`
      Done: new `node_has_live_relationship(node_id)` (crud/nodes.rs) scans for any
      non-deleted rel with `src_id == id || dst_id == id` — BOTH directions.
- [x] 3.2 `delete_node` errors on a live relationship unless DETACH pre-cleared
      Done: returns `Error::CypherExecution`. NO `detach` param added — every DETACH
      path (Cypher DETACH, REST/RPC/RESP3 detach) already calls
      `delete_node_relationships` FIRST, so the guard passes for them automatically
      (verified by code review). Simplest design, no signature churn.
- [x] 3.3 Reduce the standalone `first_rel_ptr` checks to the centralized guard
      Done: removed from `match_exec.rs` (plain DELETE) AND `write_exec.rs`
      (`FOREACH...DELETE`); both now rely on `delete_node`'s authoritative check.
- [x] 3.4 REST/RPC/RESP3 inherit the guard by calling the fixed `delete_node`
      Confirmed by code review: RESP3 (unmodified) routes through `delete_node`; its
      doc comment now describes enforced behavior. REST/RPC tests added.
- [x] 3.5 `Expand` skips the row when a non-optional endpoint is `Value::Null`
      Done: `if !optional && target_node.is_null()` skip (expand.rs), mirroring the
      empty-relationship branch; OPTIONAL MATCH still pushes its null row (verified).
- [x] 3.6 Make the §1 tests pass — all pass.

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation:
  `docs/specs/cypher-subset.md` (DELETE relationship-existence guard) and
  `docs/specs/storage-format.md` (first_rel_ptr = outgoing only; no live record
  may reference a deleted node); add a CHANGELOG entry
      Done: both specs updated; CHANGELOG [3.0.0]
      `### Fixed — phase0_fix-delete-node-dangling-relationships`.
- [x] 4.2 Write tests covering the new behavior: incoming-only DELETE errors
  (C-2a); non-DETACH delete via REST/RPC errors (C-2b); Expand skips a dangling
  endpoint (C-2c); DETACH DELETE still works
      Done: 4 nexus-core executor tests + REST + RPC server tests, all pass and
      discriminate (code-reviewed). RESP3 inherits the guard (no separate test;
      confirmed by review that it routes through the same `delete_node`).
- [x] 4.3 Run tests and confirm they pass (`cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test -p nexus-core` / `-p nexus-server` — all green)
      Done (scoped per host-resource limits): nexus-core suite green (20 groups,
      0 failed), nexus-server green (17 groups, 0 failed, incl. new REST/RPC tests);
      fmt `--check` clean; clippy exit 0. Code-reviewed: no correctness defects
      (one perf follow-up filed: `node_has_live_relationship` full-scan cost).

## Related
- `phase0_fix-update-node-index-divergence`,
  `phase0_fix-property-store-shrink-corruption` — other write-path/
  index-corruption defects from the same audit
- `phase0_fix-delete-path-index-cleanup` — separate follow-on covering
  composite/typed-index and property-chain residue left by delete (not the
  dangling-edge defect itself)
