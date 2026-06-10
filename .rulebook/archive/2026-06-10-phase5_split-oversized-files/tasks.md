# Tasks: phase5_split-oversized-files

## 1. Baseline & Safety
- [x] 1.1 Verify workspace compiles (`cargo +nightly check --workspace`) <!-- 39.5s, clean; fmt OK; clippy 0 warnings -->
- [x] 1.2 Commit pending uncommitted fix in engine/mod.rs (#14) so refactor commits stay pure moves <!-- committed as 6093bfcb -->

## 2. Split nexus-core executor (planner/eval/parser/operators)
- [x] 2.1 Split executor/planner/queries.rs (4430) into planner/queries/ submodules <!-- 10 files, max 1330 lines; check+clippy clean -->
- [x] 2.2 Split executor/eval/projection.rs (4168) into eval/projection/ submodules <!-- fn_geo/fn_math/fn_string/fn_temporal/fn_list/core + mod.rs; check+clippy clean -->
- [x] 2.3 Split executor/parser/clauses.rs (3120) into parser/clauses/ submodules <!-- admin/pattern/read/subquery/write + mod.rs; check+clippy clean -->
- [x] 2.4 Split executor/parser/expressions.rs (1652) into parser/expressions/ submodules <!-- identifier/literals/precedence/primary/structured + mod.rs -->
- [x] 2.5 Split executor/parser/tests.rs (2345) by feature area <!-- tests/{clauses,ddl,expressions,external_ids,patterns,tokens,mod} -->
- [x] 2.6 Split executor/operators/aggregate.rs (2090) into operators/aggregate/ submodules <!-- core/columnar/parallel/alias/tests + mod.rs; 5/5 tests preserved -->
- [x] 2.7 Split executor/operators/procedures.rs (2088) into operators/procedures/ submodules <!-- call/db_schema/db_indexes/dbms/fts/spatial_procs + mod.rs -->

## 3. Split nexus-core engine/storage/wal/catalog/graph
- [x] 3.1 Split engine/mod.rs (now 5853) into engine/ submodules with facade mod.rs <!-- mod.rs 861; +query_pipeline/ddl/write_exec/match_exec/constraints/transactions -->
- [x] 3.2 Split engine/tests.rs (now 3417) by feature area <!-- tests/{mod,basics,crud,errors,query,write,constraints,fulltext,indexes,transactions}; 104/104 tests -->
- [x] 3.3 Split wal/mod.rs (1824) into wal/ submodules with facade mod.rs <!-- mod.rs 925; +record/writer -->
- [x] 3.4 Split catalog/mod.rs (1641) into catalog/ submodules with facade mod.rs <!-- mod.rs 744; +types/store/mappings/stats/extensions; 36/36 tests -->
- [x] 3.5 Split graph/correlation/mod.rs (2030) into correlation/ submodules with facade mod.rs <!-- mod.rs 2313->172; +graph_types/graph_builder/collection_query; 59/59 tests -->
- [x] 3.6 Split graph/correlation/pattern_recognition.rs (1734) into pattern_recognition/ submodules <!-- types/detectors/overlay/quality/recommendation + mod.rs; 30/30 tests -->
- [x] 3.7 Split engine/crud.rs (1561 — crossed the 1500 threshold after #14 work) into crud/ submodules <!-- nodes/relationships/lookup/index_maintenance + mod.rs; 3/3 tests -->
- [x] 3.8 Split storage/mod.rs (2232) into storage/ submodules with facade mod.rs <!-- mod.rs 30; +records/record_store/record_store_ops; 20/20 tests; #16 WIP preserved intact. LEFT UNCOMMITTED deliberately: embeds the user's in-progress #16 work — user commits both together when #16 closes -->

## 4. Split nexus-server & workspace tests
- [x] 4.1 Split nexus-server/src/api/cypher/execute.rs (1563) into execute/ submodules <!-- handler/write_ops + mod.rs; response format untouched -->
- [x] 4.2 Split nexus-server/src/api/streaming.rs (1535) into streaming/ submodules <!-- service/tools/dispatcher/handlers/tests + mod.rs; 19/19 tests -->
- [x] 4.3 Split tests/integration_test.rs (1892) by feature area <!-- NOT SPLIT: file is dead code — root is a virtual workspace, no Cargo.toml references it, it is never compiled. Follow-up task created: phase5_wire-or-remove-dead-integration-test -->

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 5.1 Update or create documentation covering the implementation (CHANGELOG refactor entry) <!-- Unreleased/Changed entry added -->
- [x] 5.2 Write tests covering the new behavior (pure move: existing workspace tests cover; verify no count regression) <!-- per-module counts verified: 104+59+36+30+20+19+5+3 preserved exactly -->
- [x] 5.3 Run tests and confirm they pass <!-- cargo +nightly test --workspace green (doctests completed = all targets passed, 0 FAILED); fmt --check clean; clippy --all-targets --all-features -D warnings clean; all 17 split-set files now ≤1500 lines. Raw-line (wc) metric reveals 10 additional 1520–1797-line files -> follow-up task phase5_split-oversized-files-wave2 -->
