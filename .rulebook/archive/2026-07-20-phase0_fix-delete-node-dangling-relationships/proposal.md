# Proposal: phase0_fix-delete-node-dangling-relationships

**Priority: CRITICAL — deleting a node that still has live relationships
leaves those relationships pointing at a freed node, corrupting every
subsequent traversal through the dangling edge.** Found during a
write-path/index corruption audit; not previously reported. Three related
defects form one chain: a guard that's blind to incoming-only nodes, a
delete path that has no guard at all, and an expand operator that surfaces
the corruption as a null row instead of refusing it.

## Why

### C-2a — the plain-DELETE guard only sees OUTGOING relationships

The Cypher `MATCH ... DELETE` path (non-DETACH) refuses to delete a node
with relationships by checking `first_rel_ptr`:

```rust
// engine/match_exec.rs:138-144
let node_record = self.storage.read_node(node_id)?;
if node_record.first_rel_ptr != 0 {
    return Err(Error::CypherExecution(
        "Cannot DELETE node with existing relationships; use DETACH DELETE".to_string(),
    ));
}
```

But `create_relationship` never sets `first_rel_ptr` on the destination
node — by design, `first_rel_ptr` only tracks a node's OUTGOING
relationships:

```rust
// storage/record_store_ops.rs:792-805
// CRITICAL FIX: Don't update first_rel_ptr on target nodes for incoming relationships
// first_rel_ptr should only point to OUTGOING relationships from a node
...
// Don't update first_rel_ptr for incoming relationships
// Just preserve prop_ptr
target_node.prop_ptr = preserved_target_prop_ptr;
```

A node that is only ever a relationship TARGET therefore keeps
`first_rel_ptr == 0` forever, the guard passes, and `DELETE` hard-marks it
deleted while a live edge still points at it.

### C-2b — `Engine::delete_node` has no relationship check at all

The guard above lives ONLY in the Cypher `MATCH...DELETE` code path
(`match_exec.rs`). `Engine::delete_node` itself (`crud/nodes.rs:364-402`)
has no relationship check whatsoever:

```rust
pub fn delete_node(&mut self, id: u64) -> Result<bool> {
    if let Ok(Some(node_record)) = self.get_node(id) {
        self.indexes.label_index.remove_node(id)?;
        self.fts_evict_node(id);
        self.spatial_evict_node(id);
        let mut deleted_record = node_record;
        deleted_record.mark_deleted();
        ...
```

Every caller that goes through `Engine::delete_node` directly — not through
Cypher — inherits NO protection: REST `DELETE /data/nodes`
(`nexus-server/src/api/data.rs:602-643`, calls `engine.delete_node(request.node_id)`
at :620 with no prior relationship check), RPC `DELETE_NODE detach=false`
(`nexus-server/src/protocol/rpc/dispatch/graph.rs:130-154`, calls
`guard.delete_node(uid)` at :153 unconditionally when `detach` is false),
and RESP3 `NODE.DELETE` (`nexus-server/src/protocol/resp3/command/graph.rs:107-133`)
whose own doc comment claims Neo4j semantics it doesn't enforce:

```rust
/// `DETACH` first clears every relationship attached to the node and then
/// deletes the node itself. Without `DETACH`, the delete will fail if the
/// node still has relationships — matching Neo4j semantics.
```

but the body (:127-133) just calls `guard.delete_node(id)` when `detach` is
false — no check, no failure.

### C-2c — `Expand` turns the dangling edge into a silently wrong row instead of refusing it

Once a node is deleted out from under a live edge, `Expand` reads the
endpoint and gets `Value::Null` (`read_node_as_value_with_store`,
`executor/operators/path.rs:1421-1430`, returns `Ok(Value::Null)` when
`node_record.is_deleted()`), but inserts it unconditionally:

```rust
// executor/operators/expand.rs:437-497
let target_node = self.read_node_as_value_with_store(&expand_store, target_id)?;
...
new_row.insert(target_var.to_string(), target_node);   // :480 — Null inserted, not skipped
...
push_with_row_cap(&mut expanded_rows, new_row, "Expand")?;  // :497 — pushed unconditionally
```

unlike the empty-relationship branch above it in the same function, which
does skip. `count(r)` over such a pattern counts the dangling edge forever,
and any query on the endpoint's properties dereferences `Null` silently
instead of surfacing the corruption.

### Trigger (reproduces all three)

```
CREATE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})
MATCH (b:Person{name:'Bob'}) DELETE b        -- guard passes (b.first_rel_ptr==0), b hard-deleted
MATCH (a)-[r:KNOWS]->(b) RETURN a,r,b         -- returns a, live r, b=null; count(r) still counts it forever
```

## What Changes

- C-2a/C-2b: give `Engine::delete_node` a real relationship-existence
  check — not `first_rel_ptr` (which only sees outgoing edges) but a scan
  that also finds incoming edges (via `next_dst_ptr` traversal or an
  exact-edge index), so every caller (Cypher, REST, RPC, RESP3) inherits
  the same guard from one place. Reduce `match_exec.rs`'s local, incomplete
  check to a redundant fast-path in front of the engine-level check (or
  remove it), so it is never the ONLY check in effect.
- C-2c: make `Expand` skip the row (not insert `Null` and push it) when a
  non-optional endpoint resolves to `Value::Null`, mirroring the existing
  skip behavior for the empty-relationship branch.
- Deletion stays soft (`mark_deleted`, ids never recycled), so this is
  purely about refusing/skipping the corruption, not id-reuse.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (DELETE semantics —
  relationship-existence guard applies uniformly), `docs/specs/storage-format.md`
  (relationship linked-list invariant: no live record may reference a
  deleted node)
- Affected code: `crates/nexus-core/src/engine/crud/nodes.rs` (`delete_node`,
  add the check), `crates/nexus-core/src/engine/match_exec.rs:138-144`
  (existing incomplete guard), `crates/nexus-core/src/executor/operators/expand.rs:437-497`
  (skip null endpoint), `crates/nexus-core/src/storage/record_store_ops.rs`
  (relationship scan primitives, `create_relationship:790-805` context),
  `crates/nexus-server/src/api/data.rs:602-643`,
  `crates/nexus-server/src/protocol/rpc/dispatch/graph.rs:130-154`,
  `crates/nexus-server/src/protocol/resp3/command/graph.rs:107-133` (all
  inherit the fixed guard, no code change needed once C-2b is fixed at the
  engine level)
- Breaking change: NO for correct callers — a caller relying on the (buggy)
  success of deleting a node with live incoming edges now correctly gets an
  error / must use DETACH
- User benefit: `DELETE`/`DELETE_NODE`/`NODE.DELETE` can no longer corrupt
  the graph by leaving edges pointing at a freed node, across every
  protocol (Cypher, REST, RPC, RESP3)
- Related: `phase0_fix-update-node-index-divergence`,
  `phase0_fix-property-store-shrink-corruption` (sibling write-path/
  index-corruption defects from the same audit); `phase0_fix-delete-path-index-cleanup`
  (separate follow-on covering composite/typed-index and property-chain
  residue left by delete, not the dangling-edge defect itself)
