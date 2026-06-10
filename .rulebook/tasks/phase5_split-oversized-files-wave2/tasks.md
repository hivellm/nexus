# Tasks: phase5_split-oversized-files-wave2

## 1. Implementation (same recipe as wave 1: directory module + facade mod.rs, zero logic changes, check --tests clean per file)
- [x] 1.1 Split crates/nexus-core/src/index/mod.rs (1797) <!-- mod.rs 179; +dist/label_index/knn_index/property_index; 49/49 tests -->
- [x] 1.2 Split crates/nexus-core/src/graph/algorithms/traversal.rs (1715) <!-- traversal/{mod,bfs_dfs,shortest_path,components,centrality,similarity,mst} -->
- [x] 1.3 Split crates/nexus-core/src/graph/clustering.rs (1669) <!-- clustering/ dir; 24/24 tests -->
- [x] 1.4 Split crates/nexus-core/src/graph/core.rs (1641) <!-- core/{mod,graph,node,edge,ids,property_store,stats}; 19/19 tests -->
- [x] 1.5 Split crates/nexus-core/src/graph/correlation/data_flow/mod.rs (1632) <!-- mod.rs 39; +tracker/types/analyzer/optimization/statistics; 33/33 tests -->
- [x] 1.6 Split crates/nexus-core/src/graph/procedures.rs (1594) <!-- procedures/{mod,types,shortest_path,centrality,community,similarity,topology,custom,registry}; 12/12 tests -->
- [ ] 1.7 Split crates/nexus-core/tests/neo4j_result_comparison_test.rs (1591)
- [ ] 1.8 Split crates/nexus-core/src/executor/mod.rs (1588)
- [ ] 1.9 Split crates/nexus-core/src/graph/correlation/component.rs (1542)
- [ ] 1.10 Split crates/nexus-core/src/storage/adjacency_list.rs (1520)

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation (CHANGELOG entry)
- [ ] 2.2 Write tests covering the new behavior (pure move: existing tests cover; verify no test-count regression)
- [ ] 2.3 Run tests and confirm they pass (fmt + clippy -D warnings + cargo +nightly test --workspace)
