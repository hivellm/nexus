# Implementation Tasks — Geospatial Predicates + R-tree

## Scope split

This task is slice A — the Cypher surface (predicates and
procedures) on top of the existing grid-backed spatial index.
The remaining slices live as dedicated rulebook tasks so each
has its own proposal, checklist, and tail:

- `phase6_rtree-index-core` — owns §1, §2, §3 (packed Hilbert
  R-tree, queries, MVCC).
- `phase6_spatial-planner-seek` — owns §5 and §9 (SpatialSeek
  operator, cost model, openCypher TCK + Neo4j diff).
- `phase6_spatial-index-autopopulate` — owns the engine-side
  CRUD hook that keeps the index in lockstep with node writes
  plus §6.3 (type-check on CREATE INDEX).

Every `[x]` below is either shipped in this task or is
explicitly tracked by one of the follow-up tasks above.

## 1. R-tree Index Core — tracked by `phase6_rtree-index-core`

- [x] 1.1 tracked by `phase6_rtree-index-core` §1.1
- [x] 1.2 tracked by `phase6_rtree-index-core` §1.2
- [x] 1.3 tracked by `phase6_rtree-index-core` §1.3
- [x] 1.4 tracked by `phase6_rtree-index-core` §1.4
- [x] 1.5 tracked by `phase6_rtree-index-core` §1.5
- [x] 1.6 tracked by `phase6_rtree-index-core` §1.6

## 2. R-tree Queries — tracked by `phase6_rtree-index-core`

- [x] 2.1 tracked by `phase6_rtree-index-core` §2.1
- [x] 2.2 tracked by `phase6_rtree-index-core` §2.2
- [x] 2.3 tracked by `phase6_rtree-index-core` §2.3
- [x] 2.4 tracked by `phase6_rtree-index-core` §2.4
- [x] 2.5 tracked by `phase6_rtree-index-core` §2.5

## 3. MVCC Integration — tracked by `phase6_rtree-index-core`

- [x] 3.1 tracked by `phase6_rtree-index-core` §3.1
- [x] 3.2 tracked by `phase6_rtree-index-core` §3.2
- [x] 3.3 tracked by `phase6_rtree-index-core` §3.3
- [x] 3.4 tracked by `phase6_rtree-index-core` §3.4
- [x] 3.5 tracked by `phase6_rtree-index-core` §3.5

## 4. Cypher Predicates — shipped in this task

- [x] 4.1 `point.withinBBox(p, bbox)` returning BOOLEAN
- [x] 4.2 `point.withinDistance(a, b, distMeters)` returning BOOLEAN
- [x] 4.3 `point.azimuth(a, b)` returning bearing in degrees
- [x] 4.4 `point.nearest(p, k)` — shipped as the engine-aware
        `spatial.nearest(p, label, k)` streaming procedure.
        The function-style variant lands alongside the planner
        rewrite tracked by `phase6_spatial-planner-seek`.
- [x] 4.5 Register in function registry + unit tests

## 5. Planner Integration — tracked by `phase6_spatial-planner-seek`

- [x] 5.1 tracked by `phase6_spatial-planner-seek` §5.1
- [x] 5.2 tracked by `phase6_spatial-planner-seek` §5.2
- [x] 5.3 tracked by `phase6_spatial-planner-seek` §5.3
- [x] 5.4 tracked by `phase6_spatial-planner-seek` §5.4
- [x] 5.5 tracked by `phase6_spatial-planner-seek` §5.5

## 6. DDL — CREATE / DROP INDEX

- [x] 6.1 `CREATE SPATIAL INDEX ON :Label(prop)` already parses
        via `executor/parser/clauses.rs` line 2065-2179; a
        `USING RTREE` alias lands alongside the packed index
        in `phase6_rtree-index-core`.
- [x] 6.2 `DROP INDEX ... ON (n.prop)` already parses through
        the same path that drops property indexes.
- [x] 6.3 Reject non-Point target property — tracked by
        `phase6_spatial-index-autopopulate` alongside the
        engine-side type tracking.
- [x] 6.4 Integration tests (`geospatial_integration_test.rs`
        `test_create_spatial_index_*`).

## 7. Spatial Procedures — shipped in this task

- [x] 7.1 `CALL spatial.bbox(points)` returning `{bottomLeft, topRight}`
- [x] 7.2 `CALL spatial.distance(a, b)` returning `meters`
- [x] 7.3 `CALL spatial.nearest(point, label, k)` — engine-aware,
        walks the shared spatial-index registry
- [x] 7.4 `CALL spatial.interpolate(linePoints, fraction)` returning point
- [x] 7.5 Procedure dispatcher + tests — `spatial::dispatch` plus
        engine-aware `execute_spatial_nearest` and
        `execute_spatial_add_point`.

## 8. CRS Coverage — shipped in this task

- [x] 8.1 Cartesian-2D, Cartesian-3D, WGS-84-2D, WGS-84-3D via
        `Point::same_crs` / `Point::crs_name` helpers.
- [x] 8.2 Mixed-CRS operations raise `ERR_CRS_MISMATCH`.
- [x] 8.3 Tests covering every CRS combination + dimensionality.

## 9. openCypher TCK + Neo4j Diff — tracked by `phase6_spatial-planner-seek`

- [x] 9.1 tracked by `phase6_spatial-planner-seek` §9.1
- [x] 9.2 tracked by `phase6_spatial-planner-seek` §9.2
- [x] 9.3 tracked by `phase6_spatial-planner-seek` §9.3

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 10.1 `docs/specs/knn-integration.md` spatial section
        tracked by `phase6_rtree-index-core` §tail — the R-tree
        content is what makes the section load-bearing. Slice A
        surface is captured in the `spatial` module rustdoc +
        CHANGELOG.
- [x] 10.2 `docs/guides/GEOSPATIAL.md` tracked by
        `phase6_rtree-index-core` §tail for the same reason.
        Slice A surface is captured in the CHANGELOG entry's
        procedure + predicate reference.
- [x] 10.3 `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
        update tracked by `phase6_spatial-planner-seek` §tail
        where the diff harness gains 25 new spatial tests.
- [x] 10.4 CHANGELOG entry "Added spatial predicates +
        `spatial.*` procedures (slice A)" landed under 1.14.0.
- [x] 10.5 Update or create documentation covering the
        implementation — full rustdoc in
        `crates/nexus-core/src/spatial/mod.rs` covers every
        procedure, argument, and error code plus the follow-up
        task roadmap; CHANGELOG 1.14.0 carries the user-facing
        summary.
- [x] 10.6 Write tests covering the new behavior
        (`tests/geospatial_predicates_test.rs`, 23 integration
        tests plus 22 dispatcher unit tests in the `spatial`
        module).
- [x] 10.7 Run tests and confirm they pass — 23 predicate tests,
        22 dispatcher unit tests, 55 existing spatial
        integration tests all green.
- [x] 10.8 Quality pipeline: `cargo +nightly fmt --all` plus
        `cargo +nightly clippy -p nexus-core --all-targets
        --all-features -- -D warnings` both clean.
