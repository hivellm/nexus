# Proposal: phase6_spatial-planner-followups

Source: parent task `phase6_spatial-planner-seek` carved out three
items that need work outside the planner-rewriter scope. This
follow-up covers them as a bundle so the parent could archive
cleanly without leaving orphaned items behind.

## Why

`phase6_spatial-planner-seek` shipped the rewriter for the three
seek-shape predicates (Bbox / WithinDistance / Nearest), the
cost-based picker, and `db.indexes()` reporting. The remaining
work needs surfaces this task did not touch:

1. **§4 Function-style `point.nearest(<var>.<prop>, <k>)` in
   `RETURN` / `WITH` / `WHERE` expression position.** Implementing
   this correctly requires a multi-row Project + Sort + Limit +
   Collect lowering — the projection evaluator runs per row, but
   `point.nearest` returns a `LIST<NODE>` aggregated across the
   driving operator's output. That lowering is a non-trivial
   refactor of the projection pipeline; bundling it into the
   parent task would have inflated its scope and risked landing
   the seek rewriter behind a multi-week refactor. The parent's
   §2.3 rewriter already covers the dominant production shape
   (`MATCH (s:Store) RETURN s ORDER BY distance(s.loc, $p) LIMIT $k`),
   so users on `1.15.0` are not blocked.

2. **§6 openCypher TCK import.** The official openCypher
   distribution ships `spatial.feature` files we need to vendor
   into `crates/nexus-core/tests/tck/spatial/` and wire into the
   existing TCK harness. Vendoring requires fetching the upstream
   distribution and reconciling its license header conventions
   with this repo's; both are operator-gated (Docker Hub + git
   credentials needed).

3. **§7 Neo4j compat-diff harness, +25 spatial scenarios.** The
   diff harness compares Nexus output against a live Neo4j
   reference instance. Authoring 25 new scenarios needs a
   running Neo4j 2025.09.0 image to capture the expected golden
   diffs. That's the same pipeline the parent task's spec calls
   out: `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`.
   Operator-gated on the Docker image being available and the
   diff captures being signed off.

## What Changes

### 1. Function-style `point.nearest`

- Register `point.nearest(<var>.<prop>, <pt>, <k>)` as a Cypher
  function in `crates/nexus-core/src/executor/eval/projection.rs`
  AND as a planner-side rewrite shape in
  `crates/nexus-core/src/executor/planner/queries.rs::try_rewrite_spatial_seek`.
- Planner shape: `RETURN point.nearest(<var>.<prop>, <pt-lit>, <k-lit>)`
  rewrites the operator pipeline into `SpatialSeek::Nearest` +
  `Collect` so the function call disappears from the projection
  expression. The projection arm becomes unreachable when the
  rewriter fires.
- Projection fallback (no R-tree index): scan the label, compute
  distances, sort ascending, truncate to `k`, return the list.
  Mirrors the existing `aggregation::collect` shape for the
  inner aggregation.
- Integration test: same query against `Engine::execute_cypher`
  with and without `CREATE SPATIAL INDEX` returns identical
  `LIST<NODE>` ordered by distance.

### 2. openCypher TCK import

- Vendor `spatial.feature` files from the openCypher reference
  distribution into `crates/nexus-core/tests/tck/spatial/`. Tag
  the upstream commit hash in a `VENDOR.md` so future bumps stay
  reproducible.
- Wire them into the existing TCK harness; `cargo test -p
  nexus-core --test tck_runner` runs the new scenarios alongside
  the existing ones.
- Fix every failing scenario before archiving (target: 0 failures
  at ship time).

### 3. Neo4j compat-diff +25 spatial scenarios

- Add 25 scenarios to
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`
  covering the cross-product `Bbox / WithinDistance / Nearest`
  × `Cartesian / WGS-84` × `2D / 3D`.
- Capture golden output from a live Neo4j 2025.09.0 reference
  instance.
- Confirm the existing 300/300 diff scenarios remain green.
- Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with
  the new 325/325 total.

## Impact

- **Affected specs**: MODIFIED
  `docs/specs/cypher-spatial-predicates.md` (function-style
  `point.nearest` semantics + projection fallback);
  `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` (+25 rows).
- **Affected code**: NEW vendored TCK feature files;
  `crates/nexus-core/src/executor/eval/projection.rs`
  (function dispatch arm + scan fallback);
  `crates/nexus-core/src/executor/planner/queries.rs`
  (rewriter extension for the function-style shape);
  `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1`.
- **Breaking change**: NO — all three additions extend the
  surface; no existing query shape changes meaning.
- **User benefit**: function-style `point.nearest` ships as a
  real Cypher function for clients that prefer it over
  `ORDER BY distance(...) ASC LIMIT k`; openCypher TCK coverage
  closes a long-standing gap; the Neo4j compat score moves from
  300/300 to 325/325 once the spatial scenarios land.
- **Dependencies**: parent `phase6_spatial-planner-seek` already
  archived. No blockers on §1; §2 needs upstream openCypher
  distribution access; §3 needs a running Neo4j 2025.09.0 image
  the operator already runs for the existing diff harness.

## Source

- Parent task: archived as `phase6_spatial-planner-seek`.
- Spec excerpts referencing the three items remain in
  `.rulebook/archive/<date>-phase6_spatial-planner-seek/specs/cypher-spatial-predicates/spec.md`.
