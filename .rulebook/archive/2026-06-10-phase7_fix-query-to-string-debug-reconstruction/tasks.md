## 1. Implementation
- [x] 1.1 Thread the original query string / pre-parsed AST through the legacy `execute_cypher_ast` read fallbacks (or implement a real AST → Cypher serializer for the re-parsed clause set) so no consumer re-parses Debug output — both fallback sites now install the one-shot `preparsed_ast_override` (the cluster-mode mechanism) and pass an empty cypher string; the executor plans from the AST directly, no re-serialization
- [x] 1.2 Fix `PROFILE CALL { ... }` parsing to retain the inner query string ("Query must contain at least one clause" today) — TWO parser gaps fixed: (1) `EXPLAIN`/`PROFILE` were missing from `is_clause_boundary`, so ANY top-level EXPLAIN/PROFILE parsed to an empty AST (the real source of the error — existing tests were soft and hid it); (2) the EXPLAIN/PROFILE inner parse loops had no `CALL` arm — added with the same subquery/procedure dual shape as the top-level parser

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG [Unreleased] Fixed entries
- [x] 2.2 Write tests covering the new behavior (PROFILE over a CALL subquery; legacy path executes an inner MATCH ... RETURN) — `profile_over_call_subquery_parses_and_executes` (profile column + rows_returned=3) and `legacy_call_subquery_path_executes_inner_read` (3 rows through the legacy engine path)
- [x] 2.3 Run tests and confirm they pass — both new tests green; full lib 2388/2388; query_analysis_test 10/10; call_subquery_test 20/20; full workspace 4716 passed / 0 failed; clippy 0 warnings; fmt applied
