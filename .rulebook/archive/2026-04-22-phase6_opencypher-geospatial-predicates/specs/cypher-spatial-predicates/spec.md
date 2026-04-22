# Cypher Spatial Predicates Spec

## ADDED Requirements

### Requirement: `point.withinBBox`

The system SHALL expose `point.withinBBox(p: POINT, bbox: MAP) -> BOOLEAN`.
`bbox` MUST be a map with keys `bottomLeft` and `topRight`, each a POINT
in the same CRS as `p`.

#### Scenario: Point inside bounding box
Given `p = point({x:1, y:1})` and `bbox = {bottomLeft: point({x:0, y:0}), topRight: point({x:2, y:2})}`
When `RETURN point.withinBBox(p, bbox)` is executed
Then the result SHALL be `true`

#### Scenario: Point outside
Given `p = point({x:3, y:3})` and the same bbox as above
When the query is executed
Then the result SHALL be `false`

#### Scenario: CRS mismatch raises error
Given `p` in `Cartesian-2D` and `bbox` using `WGS-84-2D` points
When the query is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_CRS_MISMATCH`

### Requirement: `point.withinDistance`

The system SHALL expose `point.withinDistance(a: POINT, b: POINT, distMeters: FLOAT) -> BOOLEAN`.
For WGS-84 the distance SHALL be computed with haversine in meters.
For Cartesian CRS the distance SHALL be Euclidean in the point's unit.

#### Scenario: Close points match
Given WGS-84 points for Paris and Rue de Rivoli (≈ 500 m apart)
When `RETURN point.withinDistance(paris, rivoli, 1000)` is executed
Then the result SHALL be `true`

#### Scenario: Far points do not match
Given Paris and Berlin (≈ 880 km apart)
When `RETURN point.withinDistance(paris, berlin, 100000)` is executed
Then the result SHALL be `false`

### Requirement: `point.nearest` Returns K Nearest Neighbours

`point.nearest(p: POINT, k: INTEGER) -> LIST<NODE>` SHALL return the
k nodes with a point property closest to `p`, ordered ascending by
distance. Ties SHALL be broken by node id ascending.

#### Scenario: K nearest among labelled nodes
Given 10 nodes of label `Store` at distances 1, 2, 3, …, 10 m from `p`
When `MATCH (s:Store) WITH s, point.nearest(p, 3) AS near RETURN near`
Then the result's first row SHALL contain the 3 nodes at distances 1, 2, 3

### Requirement: `point.azimuth` Returns Bearing

`point.azimuth(a: POINT, b: POINT) -> FLOAT` SHALL return the bearing
from `a` to `b` in degrees (0 = north, 90 = east) for WGS-84, or
the Cartesian angle for Cartesian CRS.

#### Scenario: Due east bearing
Given WGS-84 points at equator, `a = (0,0)`, `b = (1,0)` (1° longitude east)
When `RETURN point.azimuth(a, b)` is executed
Then the result SHALL be `90.0` ± 0.1 degrees

### Requirement: R-tree-Backed Seeks

When a `WHERE point.withinBBox(...)` or `WHERE point.withinDistance(...)`
predicate targets an indexed property, the planner SHALL choose a
`SpatialSeek` operator over a label scan. When no R-tree index exists,
the planner SHALL fall back to label scan + filter.

#### Scenario: Planner uses R-tree when available
Given an R-tree index on `(:Place).loc`
When `EXPLAIN MATCH (p:Place) WHERE point.withinDistance(p.loc, $c, 1000) RETURN p`
Then the plan SHALL contain `SpatialSeek`
And SHALL NOT contain `LabelScan(:Place)` as the driving operator

#### Scenario: Planner falls back without index
Given no R-tree index on `(:Place).loc`
When the same query is explained
Then the plan SHALL use `LabelScan(:Place)` followed by a filter
