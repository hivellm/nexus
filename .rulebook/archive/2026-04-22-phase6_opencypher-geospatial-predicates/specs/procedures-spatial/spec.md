# Spatial Procedures Spec

## ADDED Requirements

### Requirement: `spatial.bbox(points)`

The system SHALL expose `CALL spatial.bbox(points: LIST<POINT>)` that
returns one row with column
`bbox: {bottomLeft: POINT, topRight: POINT}`.

#### Scenario: BBox of three points
Given `points = [point({x:1,y:1}), point({x:5,y:2}), point({x:3,y:7})]`
When `CALL spatial.bbox(points)` is executed
Then the returned `bbox.bottomLeft` SHALL equal `point({x:1, y:1})`
And `bbox.topRight` SHALL equal `point({x:5, y:7})`

#### Scenario: Empty list yields NULL bbox
Given `points = []`
When `CALL spatial.bbox(points)` is executed
Then the returned column SHALL be NULL

### Requirement: `spatial.distance(a, b)`

The procedure `CALL spatial.distance(a: POINT, b: POINT)` SHALL return
`meters: FLOAT`. For WGS-84 the distance SHALL be haversine.

#### Scenario: Paris to Berlin
Given `paris = point({latitude:48.8566, longitude:2.3522})`
And `berlin = point({latitude:52.5200, longitude:13.4050})`
When `CALL spatial.distance(paris, berlin)` is executed
Then the returned `meters` SHALL be `878_000` ± `10_000`

### Requirement: `spatial.nearest(point, label, k)`

The procedure `CALL spatial.nearest(p: POINT, label: STRING, k: INTEGER)`
SHALL stream rows `(node: NODE, dist: FLOAT)` ordered by `dist`
ascending. When an R-tree index exists on `(label).<point-property>`,
it SHALL be used; otherwise the procedure SHALL fall back to a label
scan + priority queue.

#### Scenario: Nearest three stores
Given ten `Store` nodes at known distances from `p`
When `CALL spatial.nearest(p, "Store", 3)` is executed
Then exactly three rows SHALL be returned
And `dist` SHALL increase monotonically row by row

### Requirement: `spatial.interpolate(line, frac)`

The procedure `CALL spatial.interpolate(line: LIST<POINT>, frac: FLOAT)`
SHALL return a point on the piecewise-linear path `line` at fractional
arc length `frac ∈ [0, 1]`.

#### Scenario: Midpoint of a two-point line
Given `line = [point({x:0,y:0}), point({x:10,y:0})]`
When `CALL spatial.interpolate(line, 0.5)` is executed
Then the result SHALL be `point({x:5, y:0})`

#### Scenario: Out-of-range frac
Given any `line` and `frac = 1.5`
When the procedure is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_ARG_VALUE`
