# Proposal: phase7_fix-query-to-string-debug-reconstruction

## Why
`Engine::query_to_string` (engine/query_pipeline.rs) "serializes" a parsed
AST by `format!("{:?}", clause)` — Rust Debug output, not Cypher. Every
consumer that re-parses its output fails with a confusing
`CypherSyntax("Expected identifier ...")`. Found during the
phase6_call-in-tx-result-cap work: the legacy CALL-subquery engine path
(`execute_call_subquery_commands` → `execute_cypher_ast` read fallback)
cannot execute any inner read subquery because the executor re-parses the
Debug dump. Related symptom: `PROFILE CALL { ... } IN TRANSACTIONS` fails
earlier with `Query must contain at least one clause` (the PROFILE clause
does not capture/serialize the inner query usably).

Impact today is LOW: the legacy CALL path is only reachable for internally
dispatched ASTs (top-level client queries use the executor operator), and
EXPLAIN/PROFILE normally carry the original `query_string`. But the broken
serializer is a trap for any future caller and blocks PROFILE over CALL
subqueries.

## What Changes
- Implement a real AST → Cypher serializer for the clause set the engine
  re-parses (MATCH / WHERE / RETURN / WITH / UNWIND / ORDER BY / LIMIT /
  SKIP at minimum), or — better — thread the original query string /
  pre-parsed AST through the legacy paths so no re-serialization happens
  (the executor already supports `install_preparsed_ast_override`).
- Fix `PROFILE CALL { ... }` parsing to retain the inner query string.
- Add tests: PROFILE of a CALL subquery returns a plan + result; the legacy
  `execute_call_subquery_commands` executes an inner `MATCH ... RETURN`.

## Impact
- Affected specs: cypher / EXPLAIN / PROFILE
- Affected code: `crates/nexus-core/src/engine/query_pipeline.rs`
  (`query_to_string`, `execute_cypher_ast` fallbacks), parser PROFILE
  clause capture
- Breaking change: NO
- User benefit: PROFILE works over CALL subqueries; the legacy engine path
  stops failing on re-parsed Debug output; future callers do not fall into
  the trap.
