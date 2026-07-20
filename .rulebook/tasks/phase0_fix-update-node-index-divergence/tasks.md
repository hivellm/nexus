# Tasks: phase0_fix-update-node-index-divergence

`Engine::update_node` (`crates/nexus-core/src/engine/crud/nodes.rs:303-361`)
writes the node record and property blob directly and calls none of the
index-refresh helpers the Cypher SET path (`persist_node_state`,
`crud/lookup.rs:42-107`) calls. Any typed B-tree / FTS / spatial /
label-bitmap index that covers the updated node goes stale the moment this
path runs:

```
CREATE INDEX person_email FOR (n:Person) ON (n.email);
CREATE (n:Person {email:'old@x.com'});
PUT /data/nodes {"node_id":<id>,"properties":{"email":"new@x.com"}}
MATCH (n:Person {email:'new@x.com'}) RETURN n;   -- 0 rows
```

Order matters: prove the divergence with a failing test (§1) before touching
`update_node`, so the fix is verified against the actual symptom, not just
against `persist_node_state`'s call list; then confirm exactly what
`persist_node_state` does that `update_node` skips (§2) before changing code
(§3), since the fix must reproduce that refresh sequence exactly — a partial
refresh (e.g. typed index but not the label bitmap) leaves a narrower but
still-real divergence.

## 1. Reproduce the divergence first
- [ ] 1.1 Write a failing integration test: `CREATE INDEX ... FOR (n:Person)
  ON (n.email)`, create a node with `email:'old@x.com'`, call
  `Engine::update_node` (or the REST `PUT /data/nodes` handler) to set
  `email:'new@x.com'`, then `MATCH (n:Person {email:'new@x.com'}) RETURN n`.
  Confirm it fails today (0 rows)
- [ ] 1.2 Add a second failing case for the stale-old-value read: the same
  node, `MATCH (n:Person {email:'old@x.com'}) RETURN n` still (falsely)
  returns the node after the update. Confirm it fails today (1 row, should
  be 0)
- [ ] 1.3 Add a third failing case for label drift: `update_node` changing a
  node's labels, then `MATCH (n:NewLabel) RETURN n` returns 0 rows despite
  `n.label_bits` carrying the new label. Confirm it fails today

## 2. Confirm the refresh gap
- [ ] 2.1 Diff `update_node` (`crud/nodes.rs:303-361`) against
  `persist_node_state` (`crud/lookup.rs:42-107`) line by line; list every
  refresh call `persist_node_state` makes that `update_node` omits
  (`update_node_labels_with_ids` :69, `fts_refresh_node` :91,
  `spatial_refresh_node` :94, `typed_index_refresh_node` :99-105) and
  confirm none of them, nor `index_composite_tuples`, appear anywhere in
  `crud/nodes.rs:303-361`
- [ ] 2.2 Confirm `persist_node_state` needs the PRE-write property/label
  snapshot (`old_properties`/`old_label_ids`, lookup.rs:52-55) to evict
  stale typed-index entries; `update_node` currently has no equivalent
  capture — the fix must read the old state via `self.get_node(id)` /
  `load_node_properties_map` before overwriting the record

## 3. Implement the fix
- [ ] 3.1 Capture `old_properties` and `old_label_ids` for the target node
  before `update_node` overwrites `label_bits`/`prop_ptr`
  (`crud/nodes.rs:333-346`)
- [ ] 3.2 After `write_node`/`commit` (`crud/nodes.rs:349-351`), call the
  same refresh sequence as `persist_node_state`: `update_node_labels_with_ids`,
  `fts_refresh_node`, `spatial_refresh_node`, `typed_index_refresh_node` —
  either by delegating to `persist_node_state` directly (translating
  `Vec<String>`/`serde_json::Value` into `NodeWriteState`) or by calling each
  helper explicitly with the same old/new arguments it expects
- [ ] 3.3 Confirm composite-index (`NODE KEY`/composite B-tree) tuples are
  re-indexed too if `update_node` can change a property covered by a
  composite index — apply `index_composite_tuples` or the equivalent
  removal+reinsert if so
- [ ] 3.4 Make the §1 tests pass

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 4.1 Update `docs/specs/cypher-subset.md` to state that all
  node-property/label write paths (Cypher SET, REST `PUT /data/nodes`, RPC
  `UPDATE_NODE`, RESP3 `NODE.UPDATE`) refresh the same index set; add a
  CHANGELOG entry
- [ ] 4.2 Tests: typed-index seek finds the new value and not the old value
  after `update_node`; label-bitmap `MATCH` finds the node after a label
  change via `update_node`; FTS/spatial refresh covered if reachable
  through this path
- [ ] 4.3 Run `cargo +nightly fmt --all`,
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`,
  `cargo +nightly test --workspace` — all green

## Related
- `phase0_fix-delete-node-dangling-relationships`,
  `phase0_fix-property-store-shrink-corruption` — other write-path/
  index-corruption defects from the same audit
