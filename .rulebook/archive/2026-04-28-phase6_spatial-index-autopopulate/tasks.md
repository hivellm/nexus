# Implementation Tasks — Spatial Index Auto-populate

## 1. Registry relocation

- [x] 1.1 `IndexManager::rtree: Arc<RTreeRegistry>` already shipped by `phase6_rtree-index-core`; this task now shares that handle with the executor via `ExecutorShared::rtree_registry` (set by `Engine::refresh_executor` through `Executor::install_rtree`).
- [x] 1.2 `ExecutorShared::spatial_indexes` removed from `crates/nexus-core/src/executor/shared.rs`; `execute_create_index`, `execute_spatial_nearest`, `execute_spatial_add_point` re-source through `shared.rtree_registry`.
- [x] 1.3 `RTreeRegistry` tracks per-index membership via `IndexSlot::members: HashSet<u64>` plus `definitions()` / `indexes_containing(node_id)` / `insert_point` / `delete_point` helpers; refresh and evict paths short-circuit on already-absent nodes.

## 2. CREATE auto-populate

- [x] 2.1 `Executor::spatial_autopopulate_node(node_id, label_ids, properties)` (executor-level, mirrors `Executor::fts_autopopulate_node`) wired into all three CREATE call sites in `crates/nexus-core/src/executor/operators/create.rs` (lines 207, 308, 909). The matching `Engine::spatial_autopopulate_node` lives in `crates/nexus-core/src/engine/crud.rs:568` and fires from the engine's direct `Engine::create_node` API path.
- [x] 2.2 Match rule: node carries one of the index's label ids AND `Point::from_json_value` succeeds on the indexed property. Both hooks share this rule.
- [x] 2.3 R-tree insert via `RTreeRegistry::insert_point`; engine-side hook emits `WalEntry::RTreeInsert` so crash recovery replays the write. The executor-level hook is in-memory only — same layering as FTS, where WAL emission lives on the engine side and the executor crate does not own the WAL handle.
- [x] 2.4 Integration test `spatial_index_autopopulate_on_create_node` in `crates/nexus-core/tests/geospatial_predicates_test.rs` — `CREATE (n:Place {loc: point({x: 3.0, y: 4.0})})` followed by `spatial.nearest` returns the node with no `spatial.addPoint`.

## 3. SET / REMOVE auto-refresh

- [x] 3.1 `Executor::spatial_refresh_node(node_id, label_ids, new_props)` (mirrors `Executor::spatial_autopopulate_node`); the parallel `Engine::spatial_refresh_node` is wired into `Engine::persist_node_state` at `engine/crud.rs:95`.
- [x] 3.2 Delete-then-conditional-add: phase 1 evicts every index the node currently belongs to via `RTreeRegistry::indexes_containing`; phase 2 re-inserts where the new property value is still a Point and the labels still match.
- [x] 3.3 `REMOVE n.loc` clears the last indexed property; the node stays evicted (covered by `spatial_index_evicts_on_remove_property` integration test which asserts zero rows after REMOVE).
- [x] 3.4 Integration test `spatial_index_autorefresh_on_set_property` — `SET n.loc = point({x: 10.0, y: 10.0})` moves the indexed position; subsequent `spatial.nearest(point({x: 10, y: 10}), ...)` returns dist=0.

## 4. DELETE evict

- [x] 4.1 `Executor::spatial_evict_node(node_id)` is the executor-side mirror; `Engine::spatial_evict_node` at `engine/crud.rs:686` is wired into `Engine::delete_node` at line 904. Cypher DELETE flows through `Engine::execute_match_delete_query` which calls `engine.delete_node`.
- [x] 4.2 Iterates `RTreeRegistry::indexes_containing(node_id)` so every index the node belongs to is touched exactly once. Engine-side hook emits `WalEntry::RTreeDelete` per index for replay.
- [x] 4.3 Integration test `spatial_index_evicts_on_delete_node` — `MATCH (n:Place {name: 'temp'}) DELETE n` removes the node from `spatial.nearest` results.

## 5. Type check on CREATE INDEX

- [x] 5.1 `executor/operators/admin.rs::execute_create_index` samples up to 1 000 existing `Label` nodes and verifies `prop` is a Point on each before registering the R-tree.
- [x] 5.2 First non-Point sample raises `Error::CypherExecution("ERR_RTREE_BUILD: node {node_id} has a non-Point value for property ...")`.
- [x] 5.3 Three integration tests cover the spec'd shapes: `create_spatial_index_rejects_string_property` (STRING), `create_spatial_index_rejects_integer_property` (INTEGER), `create_spatial_index_rejects_malformed_map_property` (object that is not a recognisable Point — seeded via `Engine::create_node` because the CREATE-property evaluator only admits literals).

## 6. Crash recovery

- [x] 6.1 `crates/nexus-core/tests/spatial_crash_recovery.rs` mirrors `fulltext_crash_recovery.rs` line-for-line.
- [x] 6.2 `wal_replay_restores_every_committed_point_after_registry_drop` — journals 20 `RTreeInsert` entries, drops the registry, reopens, asserts every committed point is reachable through `RTreeRegistry::nearest_with_filter`.
- [x] 6.3 `unflushed_entries_stay_absent_after_crash` — proves that points that never reached the WAL stay absent after replay; plus a third test `wal_replay_honours_insert_then_delete_ordering` that converges to the post-delete state for an insert/delete pair.

## 7. Deprecation of `spatial.addPoint`

- [x] 7.1 `tracing::info!("spatial.addPoint called — superseded by Cypher CRUD auto-populate ... scheduled for removal in v2.0.0")` at `executor/operators/procedures.rs::execute_spatial_add_point`.
- [x] 7.2 `CHANGELOG.md` `[Unreleased]` block now carries a `### Deprecated` entry for `spatial.addPoint`.
- [x] 7.3 `docs/guides/GEOSPATIAL.md` `spatial.addPoint` section now opens with a deprecation banner, points at plain CREATE + index ordering, and shows the recommended ingest snippet.

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 8.1 Update or create documentation covering the implementation — `docs/specs/rtree-index.md` gained a `## CRUD hooks` section listing the three engine-side hooks, the WAL entries each emits, the membership-tracking shape, and the `ERR_RTREE_BUILD` sample-validate behaviour. `docs/guides/GEOSPATIAL.md` updated. CHANGELOG `[Unreleased]` block now has `### Added — phase6_spatial-index-autopopulate` and `### Deprecated`.
- [x] 8.2 Write tests covering the new behavior — 4 integration tests for auto-populate / refresh / evict (`spatial_index_*` in `geospatial_predicates_test.rs`), 3 type-check tests (`create_spatial_index_rejects_*` covering STRING / INTEGER / malformed-map), 1 engine programmatic-API test (`engine_create_node_fires_spatial_autopopulate_hook`), 3 crash-recovery tests (`spatial_crash_recovery.rs`).
- [x] 8.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --test spatial_crash_recovery --test geospatial_predicates_test --test geospatial_integration_test --all-features -- --test-threads=1` reports `55 + 31 + 3 = 89 passed; 0 failed; 0 ignored`.
- [x] 8.4 Quality pipeline: `cargo +nightly fmt --all -- --check` clean; `cargo +nightly clippy -p nexus-core --all-targets --all-features -- -D warnings` clean.
- [x] 8.5 CHANGELOG `[Unreleased]` `### Added — phase6_spatial-index-autopopulate` entry covers "auto-populate spatial indexes on CREATE / SET / REMOVE / DELETE", membership tracking, registry relocation, the `ERR_RTREE_BUILD` type-check, and the crash-recovery harness.
