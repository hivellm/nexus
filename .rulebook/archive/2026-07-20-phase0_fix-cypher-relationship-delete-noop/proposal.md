# Proposal: phase0_fix-cypher-relationship-delete-noop

**Priority: HIGH (correctness / silent data non-deletion).** Found while
testing `phase0_perf-delete-node-relationship-check-full-scan`.

## Why

Cypher relationship delete (`MATCH (a)-[r:KNOWS]->(b) DELETE r`) is a **no-op**.
The query returns `Ok`, but the relationship is never removed:

- Empirical probe on an isolated engine after
  `CREATE (a)-[:KNOWS]->(b)` then `MATCH (a)-[r:KNOWS]->(b) DELETE r`:
  - `side_effects.relationships_deleted == 0`
  - the `RelationshipRecord` is byte-identical (`is_deleted() == false`)
  - a follow-up `MATCH (a)-[r:KNOWS]->(b) RETURN count(r)` still returns `1`

Root cause: the executor `Delete` operator's `execute_delete`
(`crates/nexus-core/src/executor/operators/expand.rs`, ~lines 606-621) is a
**stub** — it clears the result set but never calls
`storage::record_store_ops::delete_rel` (which exists, ~lines 348-353, and
correctly does `mark_deleted()` + `write_rel()`). `dispatch.rs` (~756-761) calls
the stub. A comment claims deletion is "handled at Engine level (lib.rs)", but
no such wiring exists for a bare relationship variable.

This is a genuine correctness bug independent of the perf work: users believe
they deleted a relationship (no error, and it disappears from *some* views) while
it stays live in the store, corrupting counts, traversals, and the
`node_has_live_relationship` guard (a node keeps refusing non-DETACH delete
forever because its "deleted" edge is still live).

DETACH DELETE of a node works (`delete_node_relationships` soft-deletes edges);
only the standalone relationship `DELETE r` path is broken.

## What Changes

- Wire the executor `Delete` operator to actually soft-delete matched
  relationship bindings through the storage layer (`delete_rel`), including
  updating the relationship index and reporting `relationships_deleted`
  accurately in `side_effects`.
- Ensure idempotency (deleting an already-deleted / non-matching edge is a
  clean no-op with `relationships_deleted == 0`).
- Confirm parity across all protocols that reach the same operator
  (Cypher/REST/RPC/RESP3).

## Impact

- Affected specs: none known (verify against `docs/specs/cypher-subset.md`)
- Affected code: `crates/nexus-core/src/executor/operators/expand.rs`
  (`execute_delete`), `crates/nexus-core/src/executor/dispatch.rs`, possibly
  `engine/crud` relationship-delete glue
- Breaking change: NO (fixes a silent no-op; existing correct callers unaffected)
- User benefit: `DELETE r` actually deletes the relationship, `side_effects`
  becomes truthful, counts/traversals stay consistent
- Related: `phase0_fix-delete-node-dangling-relationships`,
  `phase0_perf-delete-node-relationship-check-full-scan` (whose test had to
  soft-delete edges at the storage layer to work around this bug)
