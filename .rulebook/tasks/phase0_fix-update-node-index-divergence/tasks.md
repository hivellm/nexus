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
- [x] 1.1 Failing test written (`update_node_refreshes_typed_index_new_and_old_value`
  in `tests/executor/update_node_index_divergence_test.rs`): CREATE INDEX,
  create node `email:'old@x.com'`, `update_node` to `'new@x.com'`, assert the
  typed index (raw `find_exact`) + `MATCH ...{email:'new@x.com'}` find it.
  Confirmed RED (`find_exact(new) == []`, 0 rows).
- [x] 1.2 Same test also asserts the stale-old-value read: `find_exact(old)`
  empty and `MATCH ...{email:'old@x.com'}` returns 0. Confirmed RED (old value
  still matched before the fix).
- [x] 1.3 Label-drift case (`update_node_refreshes_label_index_on_label_change`):
  `update_node` relabels to `Employee`, `MATCH (n:Employee)` must find it.
  Confirmed RED (0 rows before the fix).

## 2. Confirm the refresh gap
- [x] 2.1 Diffed `update_node` against `persist_node_state`: `update_node`
  called NONE of `update_node_labels_with_ids`, `fts_refresh_node`,
  `spatial_refresh_node`, `typed_index_refresh_node`, nor `index_composite_tuples`
  — it wrote the record/blob directly and only (wrongly) re-incremented per-label
  counts.
- [x] 2.2 Confirmed `persist_node_state` captures `old_properties`
  (`load_node_properties_map`) + `old_label_ids` before writing, to evict stale
  typed entries. Rather than duplicate that capture in `update_node`, the fix
  delegates to `persist_node_state`, which owns it.

## 3. Implement the fix
- [x] 3.1/3.2 `update_node` now delegates to `persist_node_state` (after keeping
  its existence + constraint checks): builds a `NodeWriteState` from the input
  labels/properties and calls `persist_node_state`, which captures old state,
  writes new properties (preserving `first_rel_ptr`) + labels, and refreshes the
  label / typed-property / FTS / spatial indexes. The erroneous
  `increment_node_count`-on-update was dropped (matches the SET path).
- [x] 3.3 Composite / NODE KEY refresh: `persist_node_state` did NOT refresh the
  composite B-tree either (a gap SHARED with the Cypher SET path), so the fix was
  placed there (evict old tuple via `unindex_composite_tuples`, insert new via
  `index_composite_tuples`) — closing it for BOTH `update_node` and SET at once,
  rather than only in `update_node` (which would re-create the very divergence
  this task removes). Covered by `update_node_refreshes_composite_node_key_index`.
- [x] 3.4 §1 tests pass (3/3 green).

## 4. Tail (docs + tests — check or waive with tailWaiver)
- [x] 4.1 Update or create documentation covering the implementation — DONE:
  CHANGELOG entry added under `[3.0.0]`
  (`### Fixed — phase0_fix-update-node-index-divergence`) stating all node
  property/label write paths now refresh the same index set (label, typed, FTS,
  spatial, composite). `docs/specs/cypher-subset.md` edit WAIVED: this restores
  the already-intended index-consistency contract; the CHANGELOG note suffices.
- [x] 4.2 Write tests covering the new behavior — DONE: typed-index seek finds
  the new value and not the old value; label `MATCH` finds the node after a label
  change; composite NODE KEY tuple refreshed on update. 3/3 green. (FTS/spatial
  refresh now run on this path via `persist_node_state` — the same helpers the
  SET path's existing FTS/spatial suites already cover.)
- [x] 4.3 Run tests and confirm they pass — DONE (green): `cargo +nightly fmt
  --all`, `cargo clippy -p nexus-core --all-targets --all-features -- -D warnings`
  (0 warnings), full `cargo +nightly test -p nexus-core` and
  `cargo +nightly test --workspace` — 0 failed.

## Related
- `phase0_fix-delete-node-dangling-relationships`,
  `phase0_fix-property-store-shrink-corruption` — other write-path/
  index-corruption defects from the same audit
