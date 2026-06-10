# Proposal: phase5_split-oversized-files-wave2

## Why
Wave 1 (phase5_split-oversized-files) used a non-blank-line count (PowerShell Measure-Object) to build its 17-file target list. By raw line count (`wc -l`) — the more conventional metric — 10 additional files exceed 1500 lines (1520–1797 range): they were invisible to the wave-1 scan because blank lines pushed them over. Same maintainability rationale applies: smaller, cohesive modules review faster, conflict less, and fit AI/tooling context windows.

## What Changes
Split the remaining >1500-raw-line files into directory modules with facade mod.rs re-exports (zero logic changes), same recipe as wave 1:

1. `crates/nexus-core/src/index/mod.rs` (1797)
2. `crates/nexus-core/src/graph/algorithms/traversal.rs` (1715)
3. `crates/nexus-core/src/graph/clustering.rs` (1669)
4. `crates/nexus-core/src/graph/core.rs` (1641)
5. `crates/nexus-core/src/graph/correlation/data_flow/mod.rs` (1632)
6. `crates/nexus-core/src/graph/procedures.rs` (1594)
7. `crates/nexus-core/tests/neo4j_result_comparison_test.rs` (1591)
8. `crates/nexus-core/src/executor/mod.rs` (1588)
9. `crates/nexus-core/src/graph/correlation/component.rs` (1542)
10. `crates/nexus-core/src/storage/adjacency_list.rs` (1520)

(`tests/integration_test.rs` at 2090 raw lines is handled by phase5_wire-or-remove-dead-integration-test.)

## Impact
- Affected specs: core (source-file size limit)
- Affected code: nexus-core index/graph/executor/storage modules + one integration test
- Breaking change: NO (facade re-exports preserve paths)
- User benefit: completes the size-limit enforcement under the raw-line metric
