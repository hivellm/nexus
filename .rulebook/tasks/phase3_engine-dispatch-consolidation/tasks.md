## 1. Dispatch unification
- [x] 1.1 Diffed `execute_cypher_dispatch` vs `execute_cypher_ast` — divergences found and unified (notably: the G3 MERGE-rel dispatch fix and CALL-subquery handling existed in only one path; internal-AST dispatch missed SHOW CONSTRAINTS routing)
- [x] 1.2 Unified into one private dispatch in `engine/query_pipeline.rs` (220 insertions / 326 deletions — net −106 lines); both public entry points are thin shims
- [x] 1.3 PROFILE-consistency tests in `engine/tests/dispatch_consolidation.rs` (6): PROFILE MERGE-rel same side-effects as unprofiled, PROFILE CREATE+DELETE executes exactly once, PROFILE resolves `$params`, PROFILE CREATE INDEX, internal-AST SHOW CONSTRAINTS routing

## 2. Retire the params-dropping API
- [x] 2.1 Audited all `execute_cypher(` call sites across the workspace; write-capable call sites migrated to `execute_cypher_with_params`
- [x] 2.2 `execute_cypher` now delegates to `execute_cypher_with_params(query, HashMap::new())` (query_pipeline.rs:49-51) with a doc comment steering callers; new test proves no param leakage across calls (`execute_cypher_no_params_does_not_leak_prior_call_params`)

## 3. Cleanup
- [x] 3.1 `vectorizer_integration_test.rs` fixed (asserts the real serde shape `Healthy|Degraded|Unhealthy`) and un-ignored

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering the implementation — CHANGELOG `[Unreleased — 2.5.0]` Fixed entry (PROFILE drift + footgun API); docs/nexus/04 already tracks Step 7 under this task
- [x] 4.2 Write tests covering the new behavior — 6 dispatch-consolidation tests (PROFILE consistency + params delegation + internal-AST routing)
- [x] 4.3 Run tests and confirm they pass — nexus-core 2414 passed / 0 failed; nexus-server 478 passed / 0 failed; clippy `--all-targets -D warnings` clean; fmt clean
