# Proposal: phase7_call-in-transactions-executor

## Why

The Cypher 25 / Neo4j 5.x `CALL { ... } IN TRANSACTIONS [OF n ROWS] [ON ERROR ...] [REPORT STATUS AS ...]` syntax is the canonical way to run large mutating ETL inside a graph DB without OOM-ing on a single transaction. Grammar + AST + suffix clauses landed in `phase6_opencypher-subquery-transactions` (2026-04-22), but the executor side is unfinished — the engine parses the syntax and returns an error or runs the inner subquery as a single transaction. This is a Neo4j-compat gap that blocks any large ETL workload migrating from Neo4j.

## What Changes

- Implement transaction batching in the executor: split UNWIND-fed input into chunks of `n ROWS`, open a fresh transaction per chunk, commit between chunks, propagate the chunk's row stream through the surrounding query.
- Implement the `ON ERROR { CONTINUE | BREAK | FAIL }` suffix clauses: `CONTINUE` collects errors and proceeds; `BREAK` stops cleanly with partial results; `FAIL` aborts the whole call.
- Implement `REPORT STATUS AS <var>`: emit a status row per batch with `{ committed, transactionId, errorMessage, started, finished }`.
- Add executor tests for each shape (commit-per-N, ON ERROR variants, REPORT STATUS, nested CALL IN TRANSACTIONS rejected, mixing with UNWIND).
- Run Neo4j diff-suite to confirm 300/300 still passes.

## Impact

- Affected specs: `docs/specs/cypher-subset.md` (mark CALL IN TRANSACTIONS as supported).
- Affected code: `crates/nexus-core/src/executor/`, `crates/nexus-core/src/transaction/`.
- Breaking change: NO (currently errors → now executes).
- User benefit: large ETL migrations from Neo4j possible without rewrite.
