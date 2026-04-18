## 1. Tier 1 — Critical blockers (~32k LOC)
- [ ] 1.1 Split `nexus-core/src/executor/mod.rs` (15,260 LOC) into `executor/operators/`, `executor/functions/`, `executor/{eval,context,result}.rs`
- [ ] 1.2 Split `nexus-core/src/executor/parser.rs` (6,882 LOC) into `parser/clauses/`, `parser/{tokens,literals,expressions}.rs`
- [ ] 1.3 Split `nexus-core/src/lib.rs` (5,531 LOC) — extract `types.rs`, `error.rs`, `config.rs`
- [ ] 1.4 Split `nexus-core/src/graph/correlation/mod.rs` (4,638 LOC) into `correlator.rs`, `scoring.rs`, `analyzer.rs`, `reporter.rs`

## 2. Tier 2 — High priority (~12k LOC)
- [ ] 2.1 Split `nexus-core/src/executor/planner.rs` (4,254 LOC) into `planner/{cost,rewrite,logical,physical}.rs`
- [ ] 2.2 Split `nexus-core/src/graph/correlation/data_flow.rs` (3,004 LOC) into `data_flow/{taint,reachability,propagation}.rs`
- [ ] 2.3 Split `nexus-server/src/api/cypher.rs` (2,965 LOC) into `cypher/{handlers,response,validation,streaming}.rs`
- [ ] 2.4 Split `nexus-core/src/graph/algorithms.rs` (2,560 LOC) into `algorithms/{shortest_path,centrality,traversal,community}.rs`

## 3. Tier 3 — Medium priority (~11k LOC)
- [ ] 3.1 Split `nexus-core/tests/regression_extended.rs` (2,184 LOC) by feature area
- [ ] 3.2 Split `nexus-core/tests/neo4j_compatibility_test.rs` (2,103 LOC) by Neo4j section
- [ ] 3.3 Split `tests/integration_test.rs` (2,090 LOC) into `integration/{crud,transactions,indexes}.rs`
- [ ] 3.4 Split `nexus-core/src/graph/correlation/pattern_recognition.rs` (2,008 LOC) into `patterns/{detector,rules,matcher}.rs`
- [ ] 3.5 Split `nexus-server/src/api/streaming.rs` (1,726 LOC) into `streaming/{sse,websocket,chunking}.rs`
- [ ] 3.6 Split `nexus-core/src/storage/mod.rs` (1,692 LOC) into `storage/{nodes,relationships,properties}.rs`
- [ ] 3.7 Split `nexus-core/src/graph/clustering.rs` (1,669 LOC) into `clustering/{louvain,label_prop,connected_components}.rs`
- [ ] 3.8 Split `nexus-core/src/index/mod.rs` (1,614 LOC) into `index/{label,btree,fulltext,knn}.rs`

## 4. Tier 4 — Low priority (~9k LOC)
- [ ] 4.1 Split `nexus-core/src/graph/procedures.rs` (1,594 LOC) into `procedures/{db,graph,util}.rs`
- [ ] 4.2 Split `nexus-core/tests/neo4j_result_comparison_test.rs` (1,592 LOC) by result category
- [ ] 4.3 Split `nexus-core/src/catalog/mod.rs` (1,581 LOC) into `catalog/{labels,types,keys}.rs`
- [ ] 4.4 Split `nexus-core/src/graph/core.rs` (1,578 LOC) into `core/{node,edge,graph_view}.rs`
- [ ] 4.5 Split `nexus-core/src/graph/correlation/component.rs` (1,542 LOC) into `component/{detector,builder}.rs`
- [ ] 4.6 Split `nexus-core/src/storage/adjacency_list.rs` (1,520 LOC) into `adjacency/{list,iterator,mutation}.rs`

## 5. Quality gates (run after EACH tier)
- [ ] 5.1 `cargo +nightly fmt --all` — no diff
- [ ] 5.2 `cargo clippy --workspace --all-targets --all-features -- -D warnings` — zero warnings
- [ ] 5.3 `cargo test --workspace --verbose` — 100% pass
- [ ] 5.4 `cargo llvm-cov --workspace --ignore-filename-regex 'examples'` — ≥95% coverage
- [ ] 5.5 Verify no public API regression — `cargo doc --workspace --no-deps` succeeds, external SDK tests still pass
- [ ] 5.6 Run Neo4j compatibility suite — `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` — 300/300 pass

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 6.1 Update or create documentation covering the implementation
- [ ] 6.2 Write tests covering the new behavior
- [ ] 6.3 Run tests and confirm they pass
