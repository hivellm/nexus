# Proposal: `CALL {} IN TRANSACTIONS`, Nested Subqueries, `COLLECT {}` Subqueries

## Why

Nexus has basic `CALL { }` subqueries (~90% coverage) and full
`EXISTS { }` and `COUNT { }` subquery predicates, but three gaps
block openCypher parity:

1. **`CALL { } IN TRANSACTIONS [OF N ROWS]`** — the batched-
   transaction mode that Neo4j introduced in 5.0 for streaming
   inserts and large deletes. Without it, ingest jobs must be split
   client-side into thousands of small HTTP requests, each with its
   own network round-trip and planning overhead. `IN TRANSACTIONS OF
   10000 ROWS` is Neo4j's recommended pattern for any bulk import.
2. **Full `COLLECT { } subquery` semantics** — our current
   implementation handles the simple case `RETURN COLLECT { MATCH (n)
   RETURN n.name }` but fails on nested aggregation like `RETURN
   COLLECT { MATCH (n) RETURN count(n) }`. Required for GraphQL-style
   nested resolvers.
3. **Nested `CALL { }` (one subquery inside another)** — the parser
   accepts them, but variable scoping between nested scopes produces
   wrong results in ~30% of scenarios the TCK exercises.

Additionally, transactional subqueries unlock the `ON ERROR` /
`ON ERROR CONTINUE` / `ON ERROR RETRY` error-handling clauses, which
are required for any production-grade ETL workflow.

## What Changes

- **Parser**: new tokens for `IN TRANSACTIONS`, `OF N ROWS`,
  `CONCURRENT TRANSACTIONS`, `REPORT STATUS AS var`,
  `ON ERROR CONTINUE/BREAK/FAIL/RETRY`.
- **AST**: new `CallInTransactions` clause with fields
  `{ batch_size, concurrency, on_error, status_var }`.
- **Executor**: new operator `CallInTransactions` that wraps the
  inner subplan in its own transaction boundary, commits every `N`
  input rows, and maintains an error-report row stream.
- **Nested scoping**: the variable resolver pushes a fresh binding
  scope for each nested `CALL { }`. Outer vars referenced via
  importing clause (`CALL (var1, var2) { ... }` in Cypher 25; also
  `WITH var1, var2 CALL { ... }` legacy form).
- **`COLLECT { }`**: extend the evaluator to handle nested
  aggregation semantics (the aggregation runs within the subquery's
  scope; the outer sees a LIST of its result).
- **Error-handling clauses**: `ON ERROR CONTINUE` logs and moves on;
  `ON ERROR BREAK` aborts the outer query; `ON ERROR RETRY n` retries
  up to `n` times; `ON ERROR FAIL` is the default (abort).
- **Metrics**: per-batch counters (committed, failed, retried) and
  aggregate status report row when `REPORT STATUS AS` is used.

**BREAKING**: none. Every addition lives in syntactic positions
where the current parser rejects input.

## Impact

### Affected Specs

- NEW capability: `cypher-call-in-transactions`
- NEW capability: `cypher-collect-subqueries`
- MODIFIED capability: `cypher-call-subqueries` (nested scoping)
- MODIFIED capability: `cypher-error-handling` (new ON ERROR clauses)

### Affected Code

- `nexus-core/src/executor/parser/clauses.rs` (~200 lines added)
- `nexus-core/src/executor/parser/ast.rs` (~80 lines added)
- `nexus-core/src/executor/operators/call_subquery.rs` (~150 lines modified)
- `nexus-core/src/executor/operators/call_in_transactions.rs` (NEW, ~500 lines)
- `nexus-core/src/executor/operators/collect_subquery.rs` (~180 lines modified)
- `nexus-core/src/executor/eval/scope.rs` (~120 lines added, nested scoping)
- `nexus-core/src/transaction/batch.rs` (NEW, ~300 lines, batched-tx management)
- `nexus-core/tests/subquery_tck.rs` (NEW, ~1100 lines)

### Dependencies

- Requires: none (self-contained in executor + transaction layer).
- Unblocks: `phase6_opencypher-apoc-ecosystem` (APOC's
  `apoc.periodic.iterate` is a thin wrapper around `CALL IN
  TRANSACTIONS`; we can implement APOC's procedure directly once
  this task ships).

### Timeline

- **Duration**: 2–3 weeks
- **Complexity**: Medium — batched transactions require care at the
  MVCC boundary; retry semantics must be deterministic.
- **Risk**: Medium — touching the transaction layer; robust tests
  essential.
