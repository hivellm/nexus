# Proposal: phase7_point-property-accessors-parity

## Why
The Neo4j compatibility suite grew past the canonical 300 tests (now 325);
the 10 failures are all in section 18 (spatial points) and are
PRE-EXISTING parity gaps — identical results on the published 2.3.2 image
(300 passed / 10 failed on both), so they are missing features, not
regressions:
- `p.x` / `p.y` / `p.z` property access on cartesian points (returns no rows)
- `p.longitude` / `p.latitude` / `p.height` on WGS-84 points
- `p.crs` accessor (default CRS names `cartesian` / `wgs-84`)
- 3D point construction (`point({x, y, z})`, `point({longitude, latitude, height})`)
- `point.withinBBox` with positional point args (`ERR_BBOX_MALFORMED: missing 'bottomLeft'`)

## What Changes
- Implement property access on point values (cartesian: x/y/z + crs;
  WGS-84: longitude/latitude/height + crs) in the expression evaluator.
- Support the 3D constructor shapes.
- Accept the Neo4j positional argument form for `point.withinBBox`.
- Flip the 10 section-18 suite tests to green (325/325).

## Impact
- Affected specs: cypher-subset / spatial
- Affected code: `crates/nexus-core/src/executor/eval/` (point value +
  property access), spatial function surface
- Breaking change: NO
- User benefit: full Neo4j parity for point accessors; compatibility
  suite back to 100% (325/325).
