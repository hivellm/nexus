# Bug Inventory

> **Date**: 2026-07-11 · **Analyzed version**: 2.4.0 · **Method**: code greps
> for silent-degradation patterns, live Docker repro, GitHub issues, ignored
> tests, routing-heuristic review.
>
> Part of the [Nexus 2.5.0 competitive analysis](README.md).

## Confirmed bugs (repro known)

| # | Symptom | Trigger | Root cause | Severity |
|---|---|---|---|---|
| B1 | `MERGE (a)-[r:T]->(b)` creates **0 relationships** over HTTP | `CREATE (a:QQ {id:1}) CREATE (b:QQ {id:2}) MERGE (a)-[r:REL2]->(b)` → `count(r)` = 0 | `write_ops.rs` MERGE loop processes only node patterns; relationship elements never examined | **data-loss** |
| B2 | `SET` on relationship variable silently dropped over HTTP | `MATCH (a)-[r:T]->(b) SET r.k = v` → logs `WARN Variable r not found in context`, HTTP 200, nothing persisted | `variable_context: HashMap<String, Vec<u64>>` tracks node IDs only — structurally cannot hold relationships (write_ops.rs:597, 697, 908, 955) | **silent wrong-result** |
| B3 | `CREATE ... RETURN r.prop` projects `null` in-statement (property IS stored) | `CREATE (a)-[r:E {w:7}]->(b) RETURN r.w` → null; separate MATCH reads 7 | write_ops.rs RETURN projector has no relationship bindings; property access falls through to `Value::Null` | wrong-result |
| B4 | `$param` property values stored as null over HTTP (**fixed in 2.4.0**, d1b97e62) | `CREATE (n {x:$v})` stored null | `expression_to_json_value` never received the params map | **data-loss** (fixed) |
| B5 | GraphQL mutations: MERGE degrades to MATCH-only; SET/REMOVE/FOREACH silently ignored | any GraphQL mutation using MERGE or SET | GraphQL calls `executor.execute` directly; planner stubs `Clause::Merge` as a read pattern (planner_core.rs:415) and has **no** Set/Remove/Foreach operators | **data-loss** |
| B6 | RPC (port 15475) and RESP3 drop `$params` on write queries | parameterized write via SDK default transport | `rpc/dispatch/cypher.rs:273` calls `engine.execute_cypher(&query)` (params-dropping variant); RESP3 `run_cypher` ignores its `_params` | **data-loss** |
| B7 | Streaming MCP handler: hand-rolled CREATE supports literals only | CREATE with `$param` or non-literal expression via streaming API | `api/streaming/handlers.rs` (~159–228) is a 5th mini write-fork | data-loss |
| B8 | `storage.delete_rel().unwrap()` in HTTP write path | delete of a relationship hitting a storage error | panic instead of error response (write_ops.rs) | crash |

**The same query gives different results depending on the port/transport** —
HTTP (15474), RPC (15475), RESP3, GraphQL and embedded each route through
different write implementations. This is the defining bug class of 2.4.0.

## Latent bugs (verified code smell, unconfirmed blast radius)

| # | Pattern | Location | Risk |
|---|---|---|---|
| L1 | String-prefix query routing: `query_upper.starts_with("CREATE")` etc. | `handler.rs:342–387` | Leading comments, lowercase, `WITH ... CREATE`, EXPLAIN-prefixed writes can misroute to the read executor → HTTP 200 with nothing persisted |
| L2 | Silent `Value::Null` fallbacks on variable/property lookup failure in RETURN projection | write_ops.rs:1020, 1046–1058 | Cannot distinguish "property absent" (correct null) from "lookup bug" (should error) |
| L3 | `let _ = audit_logger.log_write_operation(...)` (50+ sites) | write_ops.rs throughout | Audit failures invisible; compliance gap |
| L4 | Engine-internal dispatch duplication: `execute_cypher_dispatch` vs `execute_cypher_ast` (~200 near-identical lines) | `engine/query_pipeline.rs` | PROFILE path can drift from normal path |
| L5 | `Engine::execute_cypher(&str)` silently drops params (footgun API) | `engine/query_pipeline.rs` | Any new caller repeats bug B6 |

## Open GitHub issues

None open (`gh issue list` returns empty). All previously-filed issues
(#3, #7, #14, #19, #20, #24, #25) are closed; #25's real-world HTTP
manifestation is tracked by task `phase1_http-merge-rel-and-set-rel-parity`.

## Ignored tests (67 workspace-wide; relevant subset)

| Location | Tests | Reason |
|---|---|---|
| `nexus-server/src/config.rs` | 3 env-config tests | env-var race under parallel runner |
| `nexus-server/tests/vectorizer_integration_test.rs` | 1 | health returns `Healthy` vs expected `ok` (stale assertion — fix the test) |
| `nexus-bench/tests/live_rpc.rs`, `live_compare.rs` | 8+ | require live server / Neo4j endpoints (by design) |

## Structural bug factories (root causes, not instances)

1. **Five divergent write implementations** — engine `write_exec.rs`
   (correct, tested), HTTP `write_ops.rs` (1,109-line fork), GraphQL→raw
   executor, streaming MCP mini-fork, RPC/RESP3 params-dropping wiring.
   Every engine fix must be manually ported four times; nobody does.
   → Fix: [04-write-path-unification.md](04-write-path-unification.md).
2. **String-based routing** instead of AST-predicate routing (RPC already
   has `needs_engine_interception(&ast)` — HTTP should share it).
3. **Silent-null error philosophy** in projection paths: lookup failures
   must be errors, not nulls.
4. **Missing transport-parity testing**: the 300-test compat suite runs a
   single path; no test asserts HTTP ≡ RPC ≡ GraphQL ≡ embedded for the same
   query battery.

## Elimination plan (maps to 2.5.0 tasks)

| Bug | Task |
|---|---|
| B1, B2, B3, B8, L1, L2, L3 | `phase2_http-write-path-unification` (+ parity harness in `phase1_write-path-parity-harness`) |
| B5 | `phase3_graphql-and-streaming-write-unification` |
| B6 | `phase1_rpc-resp3-param-threading` |
| B7 | `phase3_graphql-and-streaming-write-unification` |
| L4, L5 | `phase4_engine-dispatch-consolidation` |
| Ignored vectorizer test | `phase4_engine-dispatch-consolidation` (cleanup item) |
