# Cypher Spatial Predicates — Planner Integration Spec

## ADDED Requirements

### Requirement: SpatialSeek operator

The system SHALL provide a new executor operator
`Operator::SpatialSeek { index_id, mode, limit }` where
`mode` is one of `Bbox(BBox)`, `WithinDistance { center,
meters }`, or `Nearest { point, k }`. The operator SHALL read
directly from `IndexManager::rtree` and emit rows without
going through `NodeByLabel`.

#### Scenario: Seek operator emits only matching rows
Given an R-tree index `:Place(loc)` populated with 1 000
  points and 100 matching a query bbox
When `EXPLAIN MATCH (p:Place) WHERE
  point.withinBBox(p.loc, $bbox) RETURN p` is executed
Then the plan SHALL contain `SpatialSeek::Bbox`
And SHALL NOT contain `NodeByLabel(:Place)` as the driving
  operator
And the emitted row count SHALL be exactly 100

### Requirement: Planner rewrites three seekable shapes

The planner SHALL rewrite each of the following shapes into a
`SpatialSeek` when an R-tree index exists for the property:

| Cypher fragment                                        | Seek mode                        |
|--------------------------------------------------------|----------------------------------|
| `WHERE point.withinBBox(n.prop, $bbox)`                | `Bbox(bbox)`                     |
| `WHERE point.withinDistance(n.prop, $p, $d)`           | `WithinDistance { p, d }`        |
| `ORDER BY distance(n.prop, $p) ASC LIMIT $k`           | `Nearest { p, k }`               |
| `RETURN point.nearest(n.prop, $k)` (function-style)    | `Nearest { p, k }` + collect     |

#### Scenario: Planner uses R-tree when available
Given an R-tree index on `:Place(loc)`
When `EXPLAIN MATCH (p:Place) WHERE
  point.withinDistance(p.loc, $c, 1000) RETURN p` is run
Then the plan SHALL contain `SpatialSeek::WithinDistance`
And SHALL NOT contain `NodeByLabel(:Place)` as the driving
  operator

#### Scenario: Planner falls back without index
Given no R-tree index on `:Place(loc)`
When the same query is explained
Then the plan SHALL use `NodeByLabel(:Place) -> Filter` and
  SHALL produce identical results to the seek plan

### Requirement: Cost-based seek vs scan decision

The planner SHALL cost `SpatialSeek` as `log_b(N) +
matching_entries` with `b = 127`, where `matching_entries` is
derived from the index's entry count multiplied by a
selectivity estimate for the bbox / radius. The planner SHALL
pick the seek only when its estimated cost is below the label
scan + filter alternative.

#### Scenario: Planner keeps the scan for degenerate queries
Given an R-tree index on `:Place(loc)` with 100 points
And a bbox that covers the entire populated extent
When the planner compares a `SpatialSeek::Bbox` against a
  label scan + filter
Then it SHALL prefer the label scan (lower cost when every
  row matches)

### Requirement: `point.nearest` as a real function

`point.nearest(p, k)` SHALL be callable in `RETURN` /
`WITH` / `WHERE` expression position, returning a `LIST<NODE>`
ordered ascending by distance. When an R-tree index exists
for the property, the planner SHALL rewrite the call into a
`SpatialSeek::Nearest`; otherwise the evaluator SHALL fall
back to a scan + sort + truncate.

#### Scenario: Function-style call with index uses seek
Given an R-tree index on `:Store(loc)`
When `MATCH (s:Store) RETURN point.nearest(s.loc, 3)` is
  explained
Then the plan SHALL contain `SpatialSeek::Nearest { k: 3 }`

### Requirement: `db.indexes()` reports RTREE rows

`CALL db.indexes()` SHALL list every registered R-tree index
with `type = "RTREE"` and `state` drawn from `{"ONLINE",
"BUILDING", "FAILED"}` reflecting the registry's current
state.

#### Scenario: New R-tree index surfaces as ONLINE
Given a freshly registered `:Place(loc)` R-tree index
When `CALL db.indexes()` is executed
Then the row for `:Place(loc)` SHALL carry `type = "RTREE"`
  and `state = "ONLINE"`

### Requirement: openCypher TCK + Neo4j diff coverage

The Neo4j compatibility diff harness SHALL gain at least 25
new spatial scenarios covering each combination of `Bbox /
WithinDistance / Nearest` x `Cartesian / WGS-84` x `2D / 3D`.
The existing 300 diff scenarios SHALL remain green.

#### Scenario: Diff harness passes 325 / 325
When `scripts/compatibility/test-neo4j-nexus-compatibility-
  200.ps1` is executed against the current HEAD
Then every scenario SHALL produce the same result set the
  Neo4j reference produces
And the summary SHALL report `325/325 passing`
