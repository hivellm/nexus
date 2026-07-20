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
- [ ] 1.1 Write a failing test: create `(a)-[:KNOWS]->(b)`, `DELETE b`
  (non-DETACH, via Cypher `MATCH...DELETE`). Confirm it succeeds today
  (should error — C-2a)
- [ ] 1.2 Write a failing test: same setup, delete `b` via
  `Engine::delete_node` directly (or REST `DELETE /data/nodes`, RPC
  `DELETE_NODE` with `detach=false`, RESP3 `NODE.DELETE` without `DETACH`).
  Confirm it succeeds today with no error (C-2b)
- [ ] 1.3 Write a failing test: after either delete above,
  `MATCH (a)-[r:KNOWS]->(b) RETURN a,r,b`. Confirm it returns a row with
  `b = null` today instead of zero rows (C-2c), and that
  `MATCH (a)-[r:KNOWS]->(b) RETURN count(r)` still counts the dangling edge

## 2. Confirm the mechanism per defect
- [ ] 2.1 Confirm `first_rel_ptr` is never set on a relationship's
  destination node by reading `create_relationship` end to end
  (`storage/record_store_ops.rs:790-805`); confirm this is a deliberate
  design choice (the comment at :792-796), not a bug in itself — the bug is
  `match_exec.rs:138-144` treating it as a complete liveness check
- [ ] 2.2 Confirm `Engine::delete_node` (`crud/nodes.rs:364-402`) truly has
  no relationship check by reading the full function body; confirm each of
  REST/RPC/RESP3 (`nexus-server/src/api/data.rs:620`,
  `protocol/rpc/dispatch/graph.rs:153`,
  `protocol/resp3/command/graph.rs:132`) calls it directly with no upstream
  check when `detach`/DETACH is false
- [ ] 2.3 Confirm `read_node_as_value_with_store`
  (`executor/operators/path.rs:1421-1430`) returns `Value::Null` for a
  deleted node, and that `Expand` (`expand.rs:437-497`) has a working skip
  pattern already for the empty-relationship-list branch earlier in the
  same function — locate it as the template for the C-2c fix

## 3. Implement the fix
- [ ] 3.1 Add a real relationship-existence check to `Engine::delete_node`
  (`crud/nodes.rs:364-402`) that finds BOTH outgoing (via `first_rel_ptr`/
  `next_src_ptr`) and incoming (via a scan using `next_dst_ptr` or an
  exact-edge index) live relationships — not `first_rel_ptr != 0` alone
- [ ] 3.2 Make `Engine::delete_node` return an error (or a typed "has
  relationships" result) when the check in 3.1 finds any live relationship
  and the caller has not requested DETACH semantics; thread a
  `detach: bool` (or equivalent) parameter/variant through so DETACH
  callers (which already call `delete_node_relationships` first) are
  unaffected
- [ ] 3.3 Reduce `engine/match_exec.rs:138-144`'s standalone
  `first_rel_ptr != 0` check to a redundant fast-path in front of the
  engine-level check from 3.1 (or remove it), so Cypher DELETE inherits the
  complete check instead of relying on its own incomplete one
- [ ] 3.4 Confirm REST `DELETE /data/nodes` (`api/data.rs:602-643`), RPC
  `DELETE_NODE` (`rpc/dispatch/graph.rs:130-154`), and RESP3 `NODE.DELETE`
  (`resp3/command/graph.rs:107-133`) now correctly refuse non-DETACH delete
  of a node with relationships purely by virtue of calling the fixed
  `Engine::delete_node` — no code change should be needed in these three
  files unless their error surfacing needs updating for the new error
  variant
- [ ] 3.5 In `executor/operators/expand.rs:437-497`, skip the row (do not
  `insert`/`push_with_row_cap`) when `target_node` (or the source, for
  symmetry) resolves to `Value::Null` on a non-optional pattern, mirroring
  the existing empty-relationship skip branch
- [ ] 3.6 Make the §1 tests pass

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` (DELETE semantics) and
  `docs/specs/storage-format.md` (relationship linked-list invariant: no
  live record may reference a deleted node) to document the uniform guard;
  add a CHANGELOG entry
- [ ] 4.2 Tests: non-DETACH DELETE of an incoming-only node errors (C-2a);
  non-DETACH delete via REST/RPC/RESP3 errors (C-2b, one test per
  protocol); Expand skips a row whose endpoint is dangling instead of
  returning `null` (C-2c); DETACH DELETE still works for all four entry
  points
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-update-node-index-divergence`,
  `phase0_fix-property-store-shrink-corruption` — other write-path/
  index-corruption defects from the same audit
- `phase0_fix-delete-path-index-cleanup` — separate follow-on covering
  composite/typed-index and property-chain residue left by delete (not the
  dangling-edge defect itself)
