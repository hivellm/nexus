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
- [ ] 3.1 Split engine/mod.rs (5391) into engine/ submodules with facade mod.rs
- [ ] 3.2 Split engine/tests.rs (3053) by feature area
- [ ] 3.3 Split storage/mod.rs (2232) into storage/ submodules with facade mod.rs
- [ ] 3.4 Split wal/mod.rs (1824) into wal/ submodules with facade mod.rs
- [ ] 3.5 Split catalog/mod.rs (1641) into catalog/ submodules with facade mod.rs
- [ ] 3.6 Split graph/correlation/mod.rs (2030) into correlation/ submodules with facade mod.rs
- [ ] 3.7 Split graph/correlation/pattern_recognition.rs (1734) into pattern_recognition/ submodules

## 4. Split nexus-server & workspace tests
- [ ] 4.1 Split nexus-server/src/api/cypher/execute.rs (1563) into execute/ submodules
- [ ] 4.2 Split nexus-server/src/api/streaming.rs (1535) into streaming/ submodules
- [ ] 4.3 Split tests/integration_test.rs (1892) by feature area

## 5. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 5.1 Update or create documentation covering the implementation (CHANGELOG refactor entry)
- [ ] 5.2 Write tests covering the new behavior (pure move: existing 2310 workspace tests cover; verify no count regression)
- [ ] 5.3 Run tests and confirm they pass (cargo +nightly test --workspace, plus fmt + clippy -D warnings, no file >1500 lines remains in split set)
