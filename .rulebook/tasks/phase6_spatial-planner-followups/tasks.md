# Implementation Tasks — Spatial Planner Follow-ups

## 1. Function-style `point.nearest(<var>.<prop>, <pt>, <k>)`

- [ ] 1.1 Register `point.nearest` in `crates/nexus-core/src/executor/eval/projection.rs` under the namespaced-call dispatch. When the planner already rewrote the call into `SpatialSeek::Nearest`, the function arm becomes unreachable; when no R-tree index exists, the evaluator falls back to: scan label, compute distances, sort, truncate to `k`, return `Vec<Value::Object(node)>`.
- [ ] 1.2 Extend `try_rewrite_spatial_seek` in `crates/nexus-core/src/executor/planner/queries.rs` to match `RETURN point.nearest(<var>.<prop>, <pt-literal>, <k-literal>)` and emit `SpatialSeek::Nearest` + Collect.
- [ ] 1.3 Integration test: same query against `Engine::execute_cypher` with and without `CREATE SPATIAL INDEX` returns identical `LIST<NODE>` ordered by distance.

## 2. openCypher TCK import

- [ ] 2.1 Vendor `spatial.feature` files from the openCypher reference distribution into `crates/nexus-core/tests/tck/spatial/`. Tag the upstream commit hash in a `VENDOR.md` for reproducibility.
- [ ] 2.2 Wire the new feature files into the existing TCK harness so `cargo test -p nexus-core --test tck_runner` runs them alongside the existing scenarios.
- [ ] 2.3 Fix every failing scenario before archiving (target: 0 failures at ship time).

## 3. Neo4j compat-diff +25 spatial scenarios

- [ ] 3.1 Add 25 scenarios to `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` covering the cross-product `Bbox / WithinDistance / Nearest` × `Cartesian / WGS-84` × `2D / 3D`. Capture golden output from a running Neo4j 2025.09.0 reference instance.
- [ ] 3.2 Confirm the existing 300/300 diff scenarios remain green.
- [ ] 3.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` with the new 325/325 total.

## 4. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 4.1 Update or create documentation covering the implementation — `docs/specs/cypher-spatial-predicates.md` (function-style `point.nearest` semantics + projection fallback); `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` (+25 rows); CHANGELOG `[Unreleased]` entry covering all three sections.
- [ ] 4.2 Write tests covering the new behavior — function-style integration test (§1.3); the vendored TCK suite stands as its own coverage (§2.2); the diff harness (§3.1) covers Neo4j parity.
- [ ] 4.3 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --test tck_runner --test spatial_planner_test --all-features -- --test-threads=1`; `pwsh ./scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` reports 325/325.
