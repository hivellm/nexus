# 13. UNWIND-driven writes iterate rows in the engine write path with a per-row context

**Status**: proposed
**Date**: 2026-06-08
**Related Tasks**: phase6_fix-unwind-write-persists, phase6_fix-sustained-write-busyloop

## Context

A write ranging over an UNWIND row list (UNWIND [...] AS row MERGE/CREATE/SET ...) silently persisted nothing (HTTP 200, count 0). Two causes: (1) the engine write path execute_write_query explicitly rejected Clause::Unwind; (2) the REST handler only flagged writes that start with CREATE/MERGE, so an UNWIND-prefixed write fell through to the read executor, which ran UNWIND but dropped the embedded MERGE/SET. This capped write throughput at ~1-2 writes/sec (one request per row), the likely load source behind the #12 busy-loop.

## Decision

Add an UNWIND-iteration path to the engine write path. (1) execute_write_query detects a leading/embedded UNWIND and delegates to execute_unwind_write_query, which evaluates the list once and runs the post-UNWIND write clauses (MERGE/CREATE-node/SET/REMOVE/FOREACH + relationship MERGE) once per row. (2) A per-row `value lane` — `Engine.unwind_bindings: HashMap<String, serde_json::Value>` (mirrors current_params) — binds the loop variable each iteration; expression_to_json_value and evaluate_set_expression resolve `row`/`row.id` against it. (3) Each row runs against a fresh per-row context cloned from the leading-MATCH bindings, so SET/REMOVE touch only that row's node; node ids are accumulated for the trailing RETURN/count. (4) Engine dispatch routes UNWIND+CREATE (and UNWIND+MERGE/SET/REMOVE) to the write path; the REST handler routes any parsed UNWIND+write AST to engine.execute_cypher_with_params. Relationship CREATE inside UNWIND errors clearly rather than dropping. The write tail (flush/refresh/notifications) is shared via finalize_write_result.

## Alternatives Considered

- Make the read executor persist MERGE/SET embedded after UNWIND (rejected: the read executor operates on a cloned store snapshot and conflates read/write execution; writes would not reliably reach the durable engine store)
- Bind the UNWIND row by threading a value_context parameter through every write-clause function signature (rejected: large signature churn; an Engine field mirrors the existing current_params mechanism with far less churn)
- Reuse FOREACH iteration directly (rejected: FOREACH binds a Vec<u64> node-id loop variable, not a serde_json::Value map, so it cannot carry UNWIND row maps)

## Consequences

Batched UNWIND writes persist every row in one statement/request (fast single-pass backfill) instead of ~1-2 writes/sec. Per-row SET correctness verified (unw1->A, unw2->B). MERGE idempotent across rows. count(n) reflects all rows. Verified end-to-end over REST. nexus-core lib 2368 passed. Reduces the one-statement-per-write churn implicated in #12. Limitation: relationship CREATE patterns inside UNWIND are rejected (use MERGE); row-driven DELETE-by-UNWIND not in scope.
