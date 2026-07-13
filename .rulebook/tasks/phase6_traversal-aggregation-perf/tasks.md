## 1. COUNT(*) metadata shortcut (smallest, ship first)
- [x] 1.1 Bench baseline captured via `tests/benchmark_aggregation_performance.rs` (#[ignore]d, run on demand with `--release -- --ignored --nocapture`)
- [x] 1.2 `try_short_circuit_count_cross_product` (executor/dispatch.rs) widened: additional plan shapes (all-nodes scan + count, alias forms previously rejected) now answered from the label bitmap; predicate/grouped cases fall back to scan
- [x] 1.3 Correctness: `tests/test_metadata_count_optimization.rs` — 10/10 green, incl. counts after deletes (bitmap iteration excludes deleted records) and shortcut-vs-scan equality

## 2. Relationship-type pre-filter
- [x] 2.1 Bench baseline via `tests/benchmark_relationship_traversal.rs` (#[ignore]d, on demand)
- [x] 2.2 `type_id` check hoisted before record/property materialization in the traversal walk (`executor/operators/path.rs`) — non-matching rels are rejected at record-header level without materialization
- [x] 2.3 Traversal entry paths verified; typed-traversal results identical (correctness assertions in the benchmark test)

## 3. GROUP BY sizing
- [x] 3.1 Aggregation HashMap pre-sizing improved (`operators/aggregate/core.rs`) using upstream cardinality where available, rows/10 fallback retained

## 4. Statistics-driven planning
- [x] 4.1 Real label cardinalities from the catalog wired into `planner/queries/cost.rs` (NodeByLabel/Expand costs scale with actual counts, treated as upper bounds since the per-label counter is increment-only); conservative fallback to the previous constants on cold catalogs (no behavior change when stats are missing)
- [x] 4.2 Plan-quality tests: `tests/statistics_driven_join_ordering_test.rs` — 2/2 green (low-cardinality side driven first on a 10-vs-10000 label join; cold-stats fallback safe)

## 5. Gate
- [x] 5.1 Release benchmarks (#[ignore]d, reproducible on demand) recorded; zero regressions across the full suites: nexus-core lib 2414 passed / 0 failed, nexus-server lib 504 passed / 0 failed

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Update or create documentation covering the implementation — CHANGELOG `[Unreleased — 2.5.0]` Performance entries; benchmark tests carry the reproduction commands
- [x] 6.2 Write tests covering the new behavior — 10 metadata-count tests, 2 plan-quality tests, correctness assertions inside both benchmark suites
- [x] 6.3 Run tests and confirm they pass — nexus-core 2414/0, nexus-server 504/0, clippy `--all-targets -D warnings` clean, fmt clean
