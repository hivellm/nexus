# Proposal: phase6_spatial-planner-seek

## Why

`phase6_opencypher-geospatial-predicates` slice A shipped
`point.withinBBox`, `point.withinDistance`, and the implicit
k-NN shape (`ORDER BY distance(n.loc, $p) LIMIT $k`) as scalar
functions that the projection evaluator runs per-row AFTER a
`NodeByLabel(:Label)` driving operator has materialised every
row. That works end-to-end — slice A's 23 integration tests
green — but wastes every row that does not match:

- A 1 M-node graph with 100 matching points still evaluates
  `point.withinDistance` 1 000 000 times.
- The scalar evaluator has no notion of "seek from R-tree" —
  the planner emits the same `NodeByLabel + Filter` plan
  regardless of whether an R-tree index exists.
- `EXPLAIN MATCH (p:Place) WHERE point.withinDistance(p.loc,
  $c, 1000) RETURN p` currently produces `NodeByLabel(:Place)
  -> Filter(expr)` with no `SpatialSeek`.

This mismatch is visible at the openCypher TCK level: every
`spatial.feature` scenario the official TCK ships assumes the
planner picks the index-seek plan, and the Neo4j diff harness
cannot grow the 25 new spatial scenarios the parent task
promised without a real plan-level rewrite.

Depends on `phase6_rtree-index-core` so the seek operator has
a proper index to probe — measuring a planner rewrite against
the slice-A grid backend would be benchmarking a linear scan
against itself.

## What Changes

1. **New operator** `Operator::SpatialSeek { index_id, mode,
   limit }` in `crates/nexus-core/src/executor/types.rs` where
   `mode: SeekMode` is `Bbox(BBox)`, `WithinDistance { center:
   Point, meters: f64 }`, or `Nearest { point: Point, k:
   usize }`.
2. **Seek operator implementation** at
   `crates/nexus-core/src/executor/operators/spatial.rs` that
   probes `IndexManager::rtree` and emits rows without
   materialising the full label scan.
3. **Predicate recogniser** in
   `crates/nexus-core/src/executor/planner/queries.rs`:
   - `WHERE point.withinBBox(n.prop, $bbox)` -> `SpatialSeek::
     Bbox`
   - `WHERE point.withinDistance(n.prop, $p, $d)` ->
     `SpatialSeek::WithinDistance`
   - `ORDER BY distance(n.prop, $p) ASC LIMIT $k` ->
     `SpatialSeek::Nearest`
   - `RETURN point.nearest(n.prop, $k)` (function-style)
     rewrites into the seek as well, landing the §4.4 item
     slice A deferred.
4. **Cost model** in the existing `estimate_cost()` arm
   (`queries.rs:2838-2910`): `SpatialSeek` costs
   `log_b(n) + matching_entries` with `b = 127`. Planner
   prefers the seek when the estimated matching-entries count
   is smaller than the full label cardinality; otherwise it
   keeps the slice-A `NodeByLabel + Filter` plan.
5. **Planner regression tests** comparing plan shape with and
   without the R-tree index:
   - With index: plan contains `SpatialSeek`, does NOT
     contain `NodeByLabel(:Place)` as the driving operator.
   - Without index: plan uses `NodeByLabel + Filter`.
   Assert both produce identical result sets on the same
   data.
6. **openCypher TCK import**: vendor `spatial.feature` files
   from the official openCypher distribution into
   `crates/nexus-core/tests/tck/` and run them through the
   existing TCK harness.
7. **Neo4j diff harness**: add 25 spatial diff tests to
   `scripts/compatibility/test-neo4j-nexus-compatibility-
   200.ps1` covering all three SeekMode branches + CRS
   permutations. Keep the existing 300 diff tests green.
8. **Function-style `point.nearest(p, k)`** finally lands as a
   real Cypher function (returns `LIST<NODE>`) — the planner
   rewrites it into the seek before the evaluator ever sees a
   row, so the O(N) fallback never fires.
9. **`db.indexes()` integration**: the column `type` reads
   `RTREE` for every R-tree index; `state` reports `ONLINE /
   BUILDING / FAILED` based on the registry.

## Impact

- Affected specs: MODIFIED `docs/specs/cypher-spatial-
  predicates.md` (planner behaviour), MODIFIED
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` (+25
  spatial diff rows), MODIFIED `docs/guides/GEOSPATIAL.md`
  (planner section).
- Affected code: NEW
  `crates/nexus-core/src/executor/operators/spatial.rs`,
  MODIFIED `crates/nexus-core/src/executor/types.rs`
  (SpatialSeek variant),
  `crates/nexus-core/src/executor/planner/queries.rs`
  (predicate recogniser + cost arm),
  `crates/nexus-core/src/executor/eval/projection.rs`
  (`point.nearest` function arm that routes through the
  planner), `crates/nexus-core/src/executor/operators/
  procedures.rs::execute_db_indexes_procedure` (RTREE type +
  state columns).
- New tests: planner regression suite
  (`crates/nexus-core/tests/spatial_planner_test.rs`), TCK
  import (`crates/nexus-core/tests/tck/spatial/`), Neo4j diff
  additions.
- Breaking change: NO — slice A queries keep working; the
  planner just picks a cheaper plan when the R-tree index is
  available.
- User benefit: p95 latency for `WHERE
  point.withinDistance(...)` drops from `O(N)` per row to
  `O(log N + matching)`; the Neo4j compatibility score moves
  from 300/300 to 325/325; the function-style `point.nearest`
  surface promised by the parent task finally ships.
- Dependencies: requires `phase6_rtree-index-core` (needs a
  real index to seek against); blocks
  `phase6_spatial-index-autopopulate` only indirectly (the
  planner rewrite does not depend on auto-populate, but the
  end-to-end perf numbers do — tests should write through the
  CRUD hook once it lands).
- Timeline: 2 weeks. Complexity medium — the planner already
  has cost-based arms for B-tree / composite / KNN; adding a
  fourth follows the same shape. Risk medium — TCK import has
  historically surfaced edge cases (empty-index plans,
  correlated subquery contexts) that need extra fixture work.
