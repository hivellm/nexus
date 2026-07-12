## 1. GraphQL mutations
- [x] 1.1 Red tests written and CONFIRMED RED against the buggy code (agent reverted fixes to prove it), in `tests/graphql_integration_test.rs`: MERGE persists, SET persists — previously silent no-ops
- [x] 1.2 Mutating execution routed through a shared `execute_write` helper calling `engine.execute_cypher_with_params` (`api/graphql/mutation.rs`); read resolvers stay on the lock-free executor
- [x] 1.3 `resolver.rs` swept — no other raw-executor write-clause call sites remained

## 2. Streaming MCP handler
- [x] 2.1 Red test (new `tests/streaming_mcp_write_test.rs`): streaming CREATE with `$param` persists — was null via the literal-only fork
- [x] 2.2 Literal-only CREATE loop deleted from `handle_execute_cypher` (−110 lines net across both files); writes route via the shared AST predicate to `execute_cypher_with_params`, reads stay lock-free

## 3. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 3.1 Update or create documentation covering the implementation — CHANGELOG `[Unreleased — 2.5.0]` Fixed entry; docs/nexus/04 status already lists Steps 5-6 under this task
- [x] 3.2 Write tests covering the new behavior — graphql_integration_test 17/17 (incl. new MERGE/SET persistence cases), streaming_mcp_write_test 1/1
- [x] 3.3 Run tests and confirm they pass — integration targets green; full server lib 478 passed / 0 failed; clippy `--all-targets -D warnings` clean; fmt clean
