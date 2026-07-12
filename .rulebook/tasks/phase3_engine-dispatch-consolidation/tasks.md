## 1. Dispatch unification
- [ ] 1.1 Diff `execute_cypher_dispatch` vs `execute_cypher_ast` in `engine/query_pipeline.rs`; list every divergence (these are latent PROFILE-drift bugs)
- [ ] 1.2 Extract one private `dispatch(ast, query, opts)`; EXPLAIN/PROFILE callers pass a flag; both public entry points become thin shims
- [ ] 1.3 Add a PROFILE-consistency test: PROFILE of a write query performs the same side-effects as the un-profiled query

## 2. Retire the params-dropping API
- [ ] 2.1 Audit all `execute_cypher(&str)` call sites across the workspace; migrate writes to `execute_cypher_with_params`
- [ ] 2.2 Make `execute_cypher` delegate to `execute_cypher_with_params(query, HashMap::new())` with a doc comment steering callers to the params variant

## 3. Cleanup
- [ ] 3.1 Fix the stale ignored test `nexus-server/tests/vectorizer_integration_test.rs` (asserts `ok`, server returns `Healthy`) and un-ignore it

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering the implementation
- [ ] 4.2 Write tests covering the new behavior (dispatch consistency + params delegation)
- [ ] 4.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-core -p nexus-server`, clippy zero warnings)
