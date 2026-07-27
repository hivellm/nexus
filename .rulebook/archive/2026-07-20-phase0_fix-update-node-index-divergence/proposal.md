# Proposal: phase0_fix-update-node-index-divergence

**Priority: CRITICAL — a node updated through `Engine::update_node` becomes
permanently unfindable by its new property values, and its label bitmap
diverges from the label index.** Found during a write-path/index corruption
audit; not previously reported.

## Why

`Engine::update_node` (`crates/nexus-core/src/engine/crud/nodes.rs:303-361`)
is a second, independent property-write path alongside the Cypher SET path.
It writes the record and the property blob directly:

```rust
node_record.label_bits = label_bits;
node_record.prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty() {
    self.storage.property_store.write().unwrap()
        .store_properties(id, storage::property_store::EntityType::Node, properties)?
} else { 0 };
...
self.storage.write_node(id, &node_record)?;
```

and then only updates catalog per-label counters (354-358). It calls none of
`typed_index_refresh_node`, `fts_refresh_node`, `spatial_refresh_node`,
`index_composite_tuples`, or `label_index.set_node_labels` — the refresh
operations the Cypher SET path performs. Contrast `persist_node_state`
(`crud/lookup.rs:42-107`), the correct reference implementation: it captures
`old_properties`/`old_label_ids` (52-55), writes the new state, then calls
`update_node_labels_with_ids` (69), `fts_refresh_node` (91),
`spatial_refresh_node` (94), and `typed_index_refresh_node` (99-105) —
evicting stale `(label,key,value)` entries and inserting the new ones.

`update_node` even rewrites `label_bits` on the record (line 334) with no
corresponding `label_index` update, so a node whose labels change through
this path also drifts out of the label bitmap index used by
`MATCH (n:Label)`.

### Consequence (confirmed by code inspection)

Any typed B-tree index built on a property that `update_node` touches now
points at the node's OLD value forever; a MATCH/seek on the new value
returns nothing, and a seek on the stale old value still (falsely) returns
the node. The label index, full-text index, and spatial index diverge the
same way whenever this path is used to change labels or geo/text
properties.

### Trigger (reachable via REST `PUT /data/nodes` → `nexus-server/src/api/data.rs:583`, RPC `UPDATE_NODE`, RESP3 `NODE.UPDATE`)

```
CREATE INDEX person_email FOR (n:Person) ON (n.email);
CREATE (n:Person {email:'old@x.com'});
PUT /data/nodes {"node_id":<id>,"properties":{"email":"new@x.com"}}
MATCH (n:Person {email:'new@x.com'}) RETURN n;   -- 0 rows; the live node is unfindable by its new value
```

## What Changes

- Route `Engine::update_node` through the same refresh suite as
  `persist_node_state` — either call `persist_node_state` directly
  (translating `Vec<String>` labels / `serde_json::Value` properties into
  `NodeWriteState`) or call `update_node_labels_with_ids`,
  `fts_refresh_node`, `spatial_refresh_node`, and `typed_index_refresh_node`
  explicitly with the pre-write (`old_properties`, `old_label_ids`) and
  post-write state, mirroring `lookup.rs:42-107`.
- Ensure the refresh runs inside/after the same write transaction as
  `write_node` so a reader can never observe the new record with stale
  indexes.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (index consistency contract
  for node property/label updates)
- Affected code: `crates/nexus-core/src/engine/crud/nodes.rs:303-361`
  (`update_node`), `crates/nexus-core/src/engine/crud/lookup.rs:42-107`
  (`persist_node_state`, reference implementation),
  `crates/nexus-core/src/engine/crud/index_maintenance.rs`
  (`typed_index_refresh_node`, `fts_refresh_node`, `spatial_refresh_node`)
- Breaking change: NO — output/API unchanged, only correctness of
  subsequent seeks
- User benefit: `PUT /data/nodes`, RPC `UPDATE_NODE`, RESP3 `NODE.UPDATE` no
  longer silently break every typed/FTS/spatial/label index that references
  the updated node
- Related: `phase0_fix-delete-node-dangling-relationships`,
  `phase0_fix-property-store-shrink-corruption` (sibling
  write-path/index-corruption defects from the same audit)
