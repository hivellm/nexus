# Implementation Tasks — Spatial Planner Seek

## 1. Operator + executor integration

- [x] 1.1 `Operator::SpatialSeek { index_id, variable, mode }`
            added in `crates/nexus-core/src/executor/types.rs`.
            `index_id` is the `{Label}.{property}` registry
            key; `variable` is the pattern variable so
            downstream operators reference `n.prop`. `limit`
            folds into `SeekMode::Nearest::k`.
- [x] 1.2 `SeekMode` enum with `Bbox { min_x, min_y, max_x,
            max_y }`, `WithinDistance { center_x, center_y,
            meters }`, `Nearest { center_x, center_y, k }`.
            Plain f64 fields keep the surface free of
            geometry-crate types so tests build seeks without
            going through Cypher.
- [x] 1.3 `crates/nexus-core/src/executor/operators/spatial.rs`
            implements `execute_spatial_seek`. Probes the new
            `ExecutorShared::rtree_registry` (`Arc<
            RTreeRegistry>`) via `snapshot(name)`, walks per
            seek mode (`query_bbox` / `within_distance` /
            `nearest`), reads each matching node through
            `read_node_as_value`, emits rows. `Nearest` adds
            a `distance` column. Tombstoned ids are dropped
            silently so a stale registry entry does not fail
            the query. Unknown index surfaces typed
            `ERR_SPATIAL_INDEX_NOT_FOUND`. Dispatch arms in
            `dispatch.rs` and `mod.rs` route here;
            `Engine::rtree_registry()` accessor exposes the
            registry to engine callers and tests.
- [x] 1.4 Five `executor::operators::spatial::tests` cases:
            bbox emits only matching rows (4 inputs → 3 hits),
            within-distance filters by radius (4 inputs → 2
            hits at radius 1.5), nearest emits a `distance`
            column with the right values, unknown index errors
            typed, empty index returns no rows.

## 2. Predicate recogniser

- [ ] 2.1 Extend `crates/nexus-core/src/executor/planner/queries.rs` to match `WHERE point.withinBBox(<var>.<prop>, <bbox>)` and rewrite it into `SpatialSeek::Bbox`.
- [ ] 2.2 Match `WHERE point.withinDistance(<var>.<prop>, <point>, <d>)` -> `SpatialSeek::WithinDistance`.
- [ ] 2.3 Match `ORDER BY distance(<var>.<prop>, <point>) ASC LIMIT <k>` and the equivalent `point.distance` form -> `SpatialSeek::Nearest`.
- [ ] 2.4 Match function-style `RETURN point.nearest(<var>.<prop>, <k>)` -> `SpatialSeek::Nearest` wrapped in a collect projection.
- [ ] 2.5 When the recogniser matches but no R-tree index exists for `(label, prop)`, emit the existing `NodeByLabel + Filter` plan so behaviour stays correct without an index.

## 3. Cost model

- [ ] 3.1 Add a `SpatialSeek` arm to `estimate_cost()` (`queries.rs` around line 2895): `log_b(n) + matching_entries` with `b = 127` and `matching_entries` derived from the index's `entries()` count multiplied by a selectivity estimate for the bbox / radius.
- [ ] 3.2 Prefer the seek when its cost is below the label scan + filter alternative; fall back otherwise.
- [ ] 3.3 Planner regression test: synthesise two plans for the same query (index / no-index) and assert the cost-based pick.

## 4. Function-style `point.nearest`

- [ ] 4.1 Register `point.nearest` in `crates/nexus-core/src/executor/eval/projection.rs` — when the planner rewrote the call into a `SpatialSeek::Nearest`, the function arm becomes unreachable; when no index exists, the evaluator falls back to a scan + sort + truncate over the label scan.
- [ ] 4.2 Integration test: same query with and without an R-tree index returns the same LIST<NODE> ordered by distance.

## 5. `db.indexes()` reporting

- [ ] 5.1 Surface `type = "RTREE"` and `state = "ONLINE" / "BUILDING" / "FAILED"` columns for every R-tree index in `execute_db_indexes_procedure`.
- [ ] 5.2 Integration test: `CALL db.indexes()` row for a registered R-tree index reports the expected shape.

## 6. openCypher TCK import

- [ ] 6.1 Vendor `spatial.feature` files from the openCypher reference distribution into `crates/nexus-core/tests/tck/spatial/`.
- [ ] 6.2 Wire them into the existing TCK harness so they run with `cargo test -p nexus-core --test tck_runner`.
- [ ] 6.3 Fix every failing scenario (expected: 0 at ship time).

## 7. Neo4j diff harness — 25 new spatial tests

- [ ] 7.1 Add 25 scenarios to `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` covering Bbox / WithinDistance / Nearest x Cartesian / WGS-84 x 2D / 3D.
- [ ] 7.2 Confirm the existing 300/300 diff tests remain green.
- [ ] 7.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with the new 325/325 total.

## 8. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 8.1 Update or create documentation covering the implementation
- [ ] 8.2 Write tests covering the new behavior
- [ ] 8.3 Run tests and confirm they pass
- [ ] 8.4 Quality pipeline: `cargo +nightly fmt --all` + `cargo +nightly clippy -p nexus-core --all-targets --all-features -- -D warnings` clean.
- [ ] 8.5 CHANGELOG entry "Added SpatialSeek planner operator + 25 new Neo4j compatibility tests".
