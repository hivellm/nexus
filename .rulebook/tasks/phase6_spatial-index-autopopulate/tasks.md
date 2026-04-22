# Implementation Tasks — Spatial Index Auto-populate

## 1. Registry relocation

- [ ] 1.1 Add `IndexManager::rtree: RTreeRegistry` paralleling `IndexManager::fulltext` in `crates/nexus-core/src/index/mod.rs`.
- [ ] 1.2 Remove `ExecutorShared::spatial_indexes` from `crates/nexus-core/src/executor/shared.rs`; re-source the two call sites (`execute_create_index`, `execute_spatial_nearest`, `execute_spatial_add_point`) through `IndexManager::rtree`.
- [ ] 1.3 `RTreeRegistry` tracks per-index membership as `HashSet<u64>` mirroring `NamedFullTextIndex::members` so refresh / evict paths short-circuit on already-absent nodes.

## 2. CREATE auto-populate

- [ ] 2.1 New `Engine::spatial_autopopulate_node(node_id, label_ids, properties)` called from every `create_node` path.
- [ ] 2.2 Match rule: node carries >= 1 of the index's labels AND has a Point value for >= 1 of the indexed properties.
- [ ] 2.3 Insert into the R-tree; emit a matching `WalEntry::RTreeInsert` so crash recovery replays the write.
- [ ] 2.4 Integration test: `CREATE (p:Place {loc: point({x:1,y:2})})` -> `spatial.nearest` finds it without a manual `spatial.addPoint` call.

## 3. SET / REMOVE auto-refresh

- [ ] 3.1 New `Engine::spatial_refresh_node(node_id, old_props, new_props)` called from `persist_node_state`.
- [ ] 3.2 Delete-then-conditional-add: evict stale entries from every index the node was in, then re-add if the new property value is still a Point.
- [ ] 3.3 When `REMOVE n.loc` clears the last indexed property, the node stays evicted (no phantom re-add).
- [ ] 3.4 Integration test: `SET p.loc = point({x:10,y:10})` moves the node in the index; subsequent `withinDistance` queries see the new position.

## 4. DELETE evict

- [ ] 4.1 New `Engine::spatial_evict_node(node_id)` called from the `delete_node` path.
- [ ] 4.2 Iterate every index the node was in (via membership set); emit `WalEntry::RTreeDelete` for each.
- [ ] 4.3 Integration test: `DELETE p` removes the node from every spatial index that contained it.

## 5. Type check on CREATE INDEX

- [ ] 5.1 When `CREATE SPATIAL INDEX ON :Label(prop)` runs, sample up to 1 000 existing `Label` nodes and verify `prop` is a Point on each.
- [ ] 5.2 Reject with `ERR_RTREE_BUILD` when a sample carries a non-Point value; error message names the first offending `node_id`.
- [ ] 5.3 Integration test covering each non-Point JSON shape (STRING, INTEGER, malformed map).

## 6. Crash recovery

- [ ] 6.1 New `crates/nexus-core/tests/spatial_crash_recovery.rs` mirroring `fulltext_crash_recovery.rs`.
- [ ] 6.2 Scenario A — mid-ingest kill: journal N `RTreeInsert` entries, drop the engine without flushing, reopen, assert every WAL-committed point appears in `spatial.nearest`.
- [ ] 6.3 Scenario B — phantom absence: entries that never hit the WAL remain absent after replay.

## 7. Deprecation of `spatial.addPoint`

- [ ] 7.1 Procedure keeps working (idempotent with auto-populate); add a `tracing::info!` note on each call so telemetry can spot stragglers.
- [ ] 7.2 CHANGELOG note: "DEPRECATED: `spatial.addPoint` is no longer required; Cypher CRUD auto-populates spatial indexes. Scheduled for removal in v2.0.0".
- [ ] 7.3 Update `docs/guides/GEOSPATIAL.md` bulk-load section to point at plain CREATE + index ordering.

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 8.1 Update or create documentation covering the implementation
- [ ] 8.2 Write tests covering the new behavior
- [ ] 8.3 Run tests and confirm they pass
- [ ] 8.4 Quality pipeline: `cargo +nightly fmt --all` + `cargo +nightly clippy -p nexus-core --all-targets --all-features -- -D warnings` clean.
- [ ] 8.5 CHANGELOG entry "Added auto-populate for spatial indexes on CREATE / SET / REMOVE / DELETE".
