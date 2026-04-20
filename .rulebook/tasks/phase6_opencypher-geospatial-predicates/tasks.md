# Implementation Tasks — Geospatial Predicates + R-tree

## 1. R-tree Index Core

- [ ] 1.1 Create `nexus-core/src/index/rtree/mod.rs` with node/leaf structs
- [ ] 1.2 Implement bulk-load via Hilbert curve ordering
- [ ] 1.3 Implement point insert with split heuristic (quadratic split)
- [ ] 1.4 Implement point delete with underflow handling
- [ ] 1.5 Memory-mapped node pages (8 KB), same page cache as other indexes
- [ ] 1.6 Unit tests: insert, delete, bulk-load determinism

## 2. R-tree Queries

- [ ] 2.1 Range search by bounding box
- [ ] 2.2 Nearest-neighbour with priority queue (incremental k-NN)
- [ ] 2.3 Within-distance (great-circle for WGS-84, Euclidean for Cartesian)
- [ ] 2.4 Contains / intersects for bbox geometry
- [ ] 2.5 Benchmark: 1M random points, NN p95 < 2 ms

## 3. MVCC Integration

- [ ] 3.1 Insert/delete ops journal in WAL as existing index does
- [ ] 3.2 Snapshot-aware reads use shadow bbox from undo log
- [ ] 3.3 Rebuild under concurrent writes preserves MVCC consistency
- [ ] 3.4 Crash-recovery: R-tree rebuilt from WAL replay
- [ ] 3.5 Recovery tests

## 4. Cypher Predicates

- [ ] 4.1 `point.withinBBox(p, bbox)` returning BOOLEAN
- [ ] 4.2 `point.withinDistance(a, b, distMeters)` returning BOOLEAN
- [ ] 4.3 `point.azimuth(a, b)` returning bearing in degrees
- [ ] 4.4 `point.nearest(p, k)` returning LIST<NODE>
- [ ] 4.5 Register in function registry + unit tests

## 5. Planner Integration

- [ ] 5.1 Recognise `WHERE point.withinDistance(n.loc, $p, $d)` as seekable
- [ ] 5.2 Cost-based pick between R-tree, label scan, KNN
- [ ] 5.3 Push bbox predicate down into the R-tree seek operator
- [ ] 5.4 New operator `SpatialSeek`
- [ ] 5.5 Planner tests comparing plans with and without the index

## 6. DDL — CREATE / DROP INDEX

- [ ] 6.1 Parse `CREATE INDEX ... FOR (n:Label) ON (n.prop) USING RTREE`
- [ ] 6.2 Parse `DROP INDEX ... ON (n.prop)` for R-tree variant
- [ ] 6.3 Reject if target property is not declared `Point` at samples
- [ ] 6.4 Integration tests

## 7. Spatial Procedures

- [ ] 7.1 `CALL spatial.bbox(geom)` returning bounding box
- [ ] 7.2 `CALL spatial.distance(a, b)` returning meters (WGS-84)
- [ ] 7.3 `CALL spatial.nearest(point, label, k)` — convenience wrapper
- [ ] 7.4 `CALL spatial.interpolate(linePoints, fraction)` returning point
- [ ] 7.5 Procedure registry + tests

## 8. CRS Coverage

- [ ] 8.1 Support `Cartesian-2D`, `Cartesian-3D`, `WGS-84-2D`, `WGS-84-3D`
- [ ] 8.2 Reject mixed-CRS operations with `ERR_CRS_MISMATCH`
- [ ] 8.3 Tests for each CRS combination

## 9. openCypher TCK + Neo4j Diff

- [ ] 9.1 Import TCK `spatial.feature` files
- [ ] 9.2 Extend the Neo4j diff harness with 25 new spatial tests
- [ ] 9.3 Confirm 300/300 existing diff tests remain green

## 10. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 10.1 Update `docs/specs/knn-integration.md` (now covers spatial too)
- [ ] 10.2 Add `docs/guides/GEOSPATIAL.md` user guide
- [ ] 10.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 10.4 Add CHANGELOG entry "Added R-tree index + spatial predicates"
- [ ] 10.5 Update or create documentation covering the implementation
- [ ] 10.6 Write tests covering the new behavior
- [ ] 10.7 Run tests and confirm they pass
- [ ] 10.8 Quality pipeline: fmt + clippy + ≥95% coverage
