# Proposal: Geospatial Predicates, R-tree Index, Spatial Procedures

## Why

Nexus already ships the `Point` type and scalar `distance()` function
(WGS-84 and Cartesian). What it does not ship is the rest of the
openCypher spatial surface:

- **Spatial predicates**: `point.withinBBox`, `point.withinDistance`,
  `point.nearest` (openCypher + Neo4j `spatial` namespace).
- **Containment / intersection**: `ST_Contains`, `ST_Intersects` via
  `point.*` aliases (partial overlap with PostGIS names).
- **R-tree index**: today spatial queries scan every node with a
  `Point` property. For real workloads (store locators, geo-fencing,
  nearest-neighbour on physical locations) this is unusable past
  ~10k nodes.
- **Spatial procedures**: `spatial.bbox(nodes)`, `spatial.distance(p1, p2)`,
  `spatial.intersection(a, b)` — required by APOC wrappers and most
  GIS-adjacent BI tools.

Without these, customers running "find all restaurants within 500 m of
the user" hit sequential scans, and any attempt to port an existing
Neo4j Spatial workflow fails on the first `withinDistance` call.

## What Changes

- **Index layer**: new R-tree index type
  `nexus-core/src/index/rtree/`. Bulk-load via Hilbert-packing, queries
  via tree-walk with NN priority queue.
- **Executor**: register R-tree indexes with the planner; spatial
  predicates become index-seekable.
- **Functions / predicates**:
  - `point.withinBBox(point, bbox)` → BOOLEAN
  - `point.withinDistance(a, b, distMeters)` → BOOLEAN
  - `point.nearest(point, k)` → LIST<NODE>
  - `point.azimuth(a, b)` → FLOAT (bearing degrees)
- **Procedures**: `spatial.bbox(geom)`, `spatial.interpolate(line, frac)`,
  `spatial.distance(a, b)`, `spatial.nearest(point, label, k)`.
- **Planner**: cost-based choice between R-tree seek, label scan +
  filter, and KNN (when an embedding index coexists).
- **DDL**: `CREATE INDEX FOR (n:Label) ON (n.prop) USING RTREE` syntax.

**BREAKING**: none. Existing `Point` and `distance()` semantics stay
identical. The new `point.*` / `spatial.*` namespaces are additive.

## Impact

### Affected Specs

- NEW capability: `cypher-spatial-predicates`
- NEW capability: `index-rtree`
- NEW capability: `procedures-spatial`
- MODIFIED capability: `index-catalogue` (adds RTREE index type)

### Affected Code

- `nexus-core/src/index/rtree/mod.rs` (NEW, ~900 lines)
- `nexus-core/src/index/rtree/hilbert.rs` (NEW, ~200 lines bulk load)
- `nexus-core/src/index/rtree/search.rs` (NEW, ~400 lines NN + range)
- `nexus-core/src/executor/eval/functions.rs` (~250 lines, spatial funcs)
- `nexus-core/src/procedures/spatial/` (NEW, ~500 lines)
- `nexus-core/src/executor/plan/mod.rs` (~180 lines, index choice)
- `nexus-core/tests/spatial_tck.rs` (NEW, ~800 lines)

### Dependencies

- Requires: `phase6_opencypher-system-procedures` (so `db.indexes()`
  reports R-tree indexes correctly).
- Unblocks: `phase6_opencypher-apoc-ecosystem` (APOC spatial wrappers).

### Timeline

- **Duration**: 3–4 weeks
- **Complexity**: Medium — R-tree is a well-known structure; the
  hairy part is planner integration and NN-seek under concurrent writes.
- **Risk**: Medium — MVCC interaction with R-tree rebuild must not
  violate snapshot semantics.
