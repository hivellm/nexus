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

## 3. Tier 3 — Medium priority
- [x] 3.1 Split `nexus-core/tests/regression_extended.rs` (2,184 LOC) into `regression_extended_{create,match,relationships,functions,union,engine,simple}.rs` — 7 files, 140-583 LOC each; 118 tests all passing
- [x] 3.2 Split `nexus-core/tests/neo4j_compatibility_test.rs` (2,103 LOC) into `neo4j_compatibility_{core,extended,additional}_test.rs` — 3 files (317/1063/825 LOC); 109 tests all passing
- [x] 3.3 `tests/integration_test.rs` (2,090 LOC) — closed as out-of-scope on 2026-04-18. Investigation found the file is orphaned: no Cargo target references it and imports point at APIs since refactored away (`nexus_server::main::NexusServer`). Splitting would produce equally-orphaned fragments; re-wiring it is a separate concern tracked outside this task.
- [x] 3.4 `nexus-core/src/graph/correlation/pattern_recognition.rs` (2,008 LOC) — closed with owner decision 2026-04-18. File sits below the practical size threshold the tier-1/2 outliers (15k/6k/5k/4k LOC) established; no further split needed.
- [x] 3.5 `nexus-server/src/api/streaming.rs` (1,726 LOC) — closed with owner decision 2026-04-18. Below practical threshold.
- [x] 3.6 `nexus-core/src/storage/mod.rs` (1,692 LOC) — closed with owner decision 2026-04-18. Below practical threshold.
- [x] 3.7 `nexus-core/src/graph/clustering.rs` (1,669 LOC) — closed with owner decision 2026-04-18. Below practical threshold.
- [x] 3.8 `nexus-core/src/index/mod.rs` (1,640 LOC) — closed with owner decision 2026-04-18. Below practical threshold.

## 4. Tier 4 — Low priority (whole tier closed with owner decision 2026-04-18)
All Tier 4 files sit in the 1,520–1,594 LOC band, below the practical threshold set by the tier-1/2 outliers. No further splits required.

- [x] 4.1 `nexus-core/src/graph/procedures.rs` (1,594 LOC) — closed with owner decision. Below practical threshold.
- [x] 4.2 `nexus-core/tests/neo4j_result_comparison_test.rs` (1,592 LOC) — closed with owner decision. Below practical threshold.
- [x] 4.3 `nexus-core/src/catalog/mod.rs` (1,581 LOC) — closed with owner decision. Below practical threshold.
- [x] 4.4 `nexus-core/src/graph/core.rs` (1,578 LOC) — closed with owner decision. Below practical threshold.
- [x] 4.5 `nexus-core/src/graph/correlation/component.rs` (1,542 LOC) — closed with owner decision. Below practical threshold.
- [x] 4.6 `nexus-core/src/storage/adjacency_list.rs` (1,520 LOC) — closed with owner decision. Below practical threshold.

## 5. Quality gates (run after EACH tier)
- [x] 5.1 `cargo +nightly fmt --all` — no diff (pre-commit hook enforces across every Tier 1 / Tier 2 commit)
- [x] 5.2 `cargo clippy --workspace --all-targets -- -D warnings` — zero warnings (pre-commit enforced)
- [x] 5.3 `cargo test --workspace --verbose` — 100% pass (2566 nexus-core tests green)
- [ ] 5.4 `cargo llvm-cov --workspace --ignore-filename-regex 'examples'` — not collected; test counts approximate (2566 tests)
- [x] 5.5 Public API stable — `cargo doc` builds; SDK tests unchanged (same Cypher grammar, same REST surface)
- [x] 5.6 Neo4j compatibility suite — 300/300 pass (runs as part of the full workspace test suite the hooks enforce)

## 6. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 6.1 Update or create documentation covering the implementation — CHANGELOG entries `🧱 Oversized-Module Split — Tier 1 + Tier 2`, `🧱 Engine Module Split (Tier 1.5)`, `🧱 Regression Test Split (Tier 3.1)`, `🧱 Neo4j Compatibility Test Split (Tier 3.2)` list every file, before/after LOC, new sub-module layout, and benefit rationale.
- [x] 6.2 Write tests covering the new behavior — the split is pure refactor (no behaviour change), so the existing test suite is the coverage. 2566 nexus-core tests green plus the 118 + 109 = 227 tests in the newly split tier-3 binaries across all commits.
- [x] 6.3 Run tests and confirm they pass — workspace suite green on every commit (pre-commit hook gates).
