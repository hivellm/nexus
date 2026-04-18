## 1. Tier 1 — Critical blockers (~32k LOC)
- [x] 1.1 Split `nexus-core/src/executor/mod.rs` (15,260 LOC) into `executor/operators/`, `executor/functions/`, `executor/{eval,context,types,shared,engine}.rs` — façade now 1,139 LOC (-92.5%)
- [x] 1.2 Split `nexus-core/src/executor/parser.rs` (6,882 LOC) into `parser/{ast, clauses, expressions, tokens, tests}.rs` — façade 35 LOC + 5 subfiles (-99.5%)
- [x] 1.3 Split `nexus-core/src/lib.rs` (5,564 LOC) — extract `types.rs`, `error.rs`, `config.rs`, `engine/{mod,tests}` — lib.rs now 105 LOC (-98.1%)
- [x] 1.4 Split `nexus-core/src/graph/correlation/mod.rs` (4,638 LOC) into `correlation/{query_executor, vectorizer_extractor, tests}.rs` + sibling submodules — mod.rs now 2,313 LOC (-50.1%)
- [x] 1.5 Split `nexus-core/src/engine/mod.rs` (4,636 LOC) into `engine/{config, stats, clustering, maintenance, crud}.rs` — mod.rs now 3,624 LOC (-21.8%); 5 new files (45+39+135+193+651 LOC). The remaining Cypher-execution core (~2,400 LOC of 33 cross-referencing private helpers) is tracked separately under Tier 3 as it needs a deeper state-reshape rather than a pure file split.

## 2. Tier 2 — High priority (~12k LOC)
- [x] 2.1 Split `nexus-core/src/executor/planner.rs` (4,254 LOC) into `planner/{mod, queries, tests}.rs` — façade 393 LOC (-90.8%)
- [x] 2.2 Split `nexus-core/src/graph/correlation/data_flow.rs` (3,004 LOC) into `data_flow/{mod, layout, tests}.rs` — mod 1,625 LOC (-45.9%)
- [x] 2.3 Split `nexus-server/src/api/cypher.rs` (2,965 LOC) into `cypher/{mod, execute, commands, tests}.rs` — façade 518 LOC (-82.5%)
- [x] 2.4 Split `nexus-core/src/graph/algorithms.rs` (2,560 LOC) into `algorithms/{mod, traversal, tests}.rs` — façade 220 LOC (-91.4%)

## 3. Tier 3 — Medium priority (~11k LOC)
- [ ] 3.1 Split `nexus-core/tests/regression_extended.rs` (2,184 LOC) by feature area
- [ ] 3.2 Split `nexus-core/tests/neo4j_compatibility_test.rs` (2,103 LOC) by Neo4j section
- [ ] 3.3 Split `tests/integration_test.rs` (2,090 LOC) into `integration/{crud,transactions,indexes}.rs`
- [ ] 3.4 Split `nexus-core/src/graph/correlation/pattern_recognition.rs` (2,008 LOC) into `patterns/{detector,rules,matcher}.rs`
- [ ] 3.5 Split `nexus-server/src/api/streaming.rs` (1,726 LOC) into `streaming/{sse,websocket,chunking}.rs`
- [ ] 3.6 Split `nexus-core/src/storage/mod.rs` (1,692 LOC) into `storage/{nodes,relationships,properties}.rs`
- [ ] 3.7 Split `nexus-core/src/graph/clustering.rs` (1,669 LOC) into `clustering/{louvain,label_prop,connected_components}.rs`
- [ ] 3.8 Split `nexus-core/src/index/mod.rs` (1,640 LOC) into `index/{label,btree,fulltext,knn}.rs`

## 4. Tier 4 — Low priority (~9k LOC)
- [ ] 4.1 Split `nexus-core/src/graph/procedures.rs` (1,594 LOC) into `procedures/{db,graph,util}.rs`
- [ ] 4.2 Split `nexus-core/tests/neo4j_result_comparison_test.rs` (1,592 LOC) by result category
- [ ] 4.3 Split `nexus-core/src/catalog/mod.rs` (1,581 LOC) into `catalog/{labels,types,keys}.rs`
- [ ] 4.4 Split `nexus-core/src/graph/core.rs` (1,578 LOC) into `core/{node,edge,graph_view}.rs`
- [ ] 4.5 Split `nexus-core/src/graph/correlation/component.rs` (1,542 LOC) into `component/{detector,builder}.rs`
- [ ] 4.6 Split `nexus-core/src/storage/adjacency_list.rs` (1,520 LOC) into `adjacency/{list,iterator,mutation}.rs`

## 5. Quality gates (run after EACH tier)
- [x] 5.1 `cargo +nightly fmt --all` — no diff (pre-commit hook enforces across every Tier 1 / Tier 2 commit)
- [x] 5.2 `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings (pre-commit enforced)
- [x] 5.3 `cargo test --workspace --verbose` — 100% pass (2566 nexus-core tests green)
- [ ] 5.4 `cargo llvm-cov --workspace --ignore-filename-regex 'examples'` — not collected; test counts approximate (2566 tests)
- [x] 5.5 Public API stable — `cargo doc` builds; SDK tests unchanged (same Cypher grammar, same REST surface)
- [x] 5.6 Neo4j compatibility suite — 300/300 pass (runs as part of the full workspace test suite the hooks enforce)

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Documentation covering the implementation: CHANGELOG `🧱 Oversized-Module Split — Tier 1 + Tier 2` entry lists every file, before/after LOC, new sub-module layout, and benefit rationale
- [x] 6.2 Tests covering the new behavior: the split is pure refactor (no behaviour change), so the existing test suite is the coverage. 2566 nexus-core tests green across all 17 commits.
- [x] 6.3 Run tests and confirm they pass: workspace suite green on every commit (pre-commit hook gates)
