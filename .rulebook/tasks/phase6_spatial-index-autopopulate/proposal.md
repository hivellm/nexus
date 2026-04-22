# Proposal: phase6_spatial-index-autopopulate

## Why

`phase6_opencypher-geospatial-predicates` slice A shipped
`spatial.*` procedures that read from
`ExecutorShared::spatial_indexes`, but NO write path keeps the
index in lockstep with the authoritative node store:

- Running `CREATE (n:Place {loc: point({x:1, y:2})})` updates
  the node store but does NOT touch the spatial index.
- `SET n.loc = point({...})` does not refresh the index.
- `REMOVE n.loc` and `DELETE n` do not evict the index entry.

Slice A's workaround is the Cypher-level `spatial.addPoint(
label, property, nodeId, point)` bulk-loader. Users can script
around it for one-shot imports, but every real workload that
goes through normal Cypher CRUD ends up with a stale index.
The symptom is subtle: `CREATE SPATIAL INDEX` succeeds,
`db.indexes()` reports the index, `spatial.nearest` returns
zero rows, and nothing in the error taxonomy tells the user
why.

The FTS subsystem already ships the matching hooks
(`fts_autopopulate_node` / `fts_refresh_node` at
`crates/nexus-core/src/engine/crud.rs:92 / 457 / 477`) and the
MVCC tracking proves the pattern integrates cleanly with the
transaction manager. This task wires the equivalent hooks for
spatial.

Depends on `phase6_rtree-index-core` because the CRUD hook
needs to reach the spatial index registry through
`IndexManager::rtree` — slice A's `ExecutorShared::spatial_
indexes` is the wrong place to hook since the engine crate
cannot reach the executor crate's internal state.

## What Changes

1. **Relocate the spatial registry**: move the spatial-index
   map from `ExecutorShared::spatial_indexes: Arc<RwLock<
   HashMap<String, SpatialIndex>>>` to a new
   `IndexManager::rtree: RTreeRegistry` so the engine crate's
   `engine::crud` module can reach it the same way it reaches
   `indexes.fulltext`.
2. **`Engine::spatial_autopopulate_node`** hook called from
   every `create_node` path (standalone `CREATE`, `MATCH +
   CREATE`, relationship-target creation, programmatic
   `Engine::create_node`). For every registered spatial
   index, match `(label, property)`, extract a `Point` from
   the written properties, insert into the index, emit a WAL
   `RTreeInsert`. Mirrors `fts_autopopulate_node` shape.
3. **`Engine::spatial_refresh_node`** hook called from
   `persist_node_state` on SET / REMOVE. Delete-then-
   conditional-add: drop the old entry from every index the
   node belonged to, then re-add if the new property value is
   still a valid Point.
4. **`Engine::spatial_evict_node`** hook called from
   `delete_node` — remove from every index the node appeared
   in; emit `RTreeDelete` for each.
5. **Type check §6.3 from the parent task**: reject `CREATE
   SPATIAL INDEX ON :Label(prop)` when a sample of existing
   `Label` nodes carries `prop` as a non-Point value. Error
   code `ERR_RTREE_BUILD` with a line-numbered diagnostic
   pointing at the first offending node.
6. **Membership tracking**: per-index `HashSet<u64>` mirroring
   the FTS `NamedFullTextIndex::members` set so the refresh
   and evict paths do not need to re-scan every property on
   every write.
7. **Crash-recovery integration test** at
   `crates/nexus-core/tests/spatial_crash_recovery.rs`
   mirroring the FTS one: fork-style simulation that
   journals N `RTreeInsert` entries, drops the engine
   without flushing, reopens, and asserts every WAL-committed
   point is visible through `spatial.nearest`.
8. **Deprecation of `spatial.addPoint`**: the procedure keeps
   working (idempotent with the auto-populate hook) but the
   CHANGELOG notes it as a legacy path. Removed in a future
   major version.

## Impact

- Affected specs: MODIFIED `docs/specs/rtree-index.md` (CRUD
  hook section), MODIFIED `docs/guides/GEOSPATIAL.md`
  (auto-populate behaviour).
- Affected code: MODIFIED `crates/nexus-core/src/index/mod.rs`
  (add `IndexManager::rtree`), MODIFIED
  `crates/nexus-core/src/executor/shared.rs` (remove
  `spatial_indexes` field, re-source through `IndexManager`),
  MODIFIED `crates/nexus-core/src/engine/crud.rs` (new
  `spatial_autopopulate_node` / `spatial_refresh_node` /
  `spatial_evict_node` hooks), MODIFIED
  `crates/nexus-core/src/executor/operators/admin.rs` (CREATE
  SPATIAL INDEX samples existing nodes for type check),
  MODIFIED `crates/nexus-core/src/executor/operators/
  procedures.rs::execute_spatial_nearest` /
  `execute_spatial_add_point` (re-source from `IndexManager`).
- New tests: `crates/nexus-core/tests/
  spatial_crash_recovery.rs`, auto-populate coverage in
  `geospatial_predicates_test.rs`.
- Breaking change: NO — `spatial.addPoint` stays callable for
  scripts that used it under slice A; ordinary Cypher CRUD
  starts populating the index automatically.
- User benefit: the spatial index finally reflects live data
  without a manual bulk-load step; `spatial.nearest` starts
  returning sensible results for datasets written through
  `CREATE`; crash recovery covers spatial writes end-to-end.
- Dependencies: requires `phase6_rtree-index-core` (the CRUD
  hook needs the `IndexManager::rtree` handle). Unblocks the
  TCK-level integration in `phase6_spatial-planner-seek`
  because the planner rewrite only delivers its p95 numbers
  when the index is populated automatically.
- Timeline: 1-2 weeks. Complexity low — pattern is a
  line-for-line mirror of `fts_autopopulate_node`. Risk low
  — the MVCC guarantees come from the R-tree task, not from
  this one.
