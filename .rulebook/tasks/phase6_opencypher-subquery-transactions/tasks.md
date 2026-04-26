# Implementation Tasks — Subquery Transactions + Collect + Nesting

## Status — slices 1 / 2 / 5

- Operator `Operator::CallSubquery` wired through planner + dispatch
  with full read-only-inner + write-bearing-inner support.
- `Operator::Create` arm in dispatch routes to
  `execute_create_pattern_with_variables` when the outer scope is
  empty and to `execute_create_with_context` (now anonymous-node-aware
  + row-aware property resolution) otherwise.
- `IN TRANSACTIONS [OF N ROWS] [REPORT STATUS AS s] [ON ERROR
  CONTINUE/BREAK/FAIL/RETRY n]` runs end-to-end: per-batch ON ERROR
  policy, REPORT STATUS rows as a single MAP under the declared name,
  Retry-then-escalate.
- Project-operator dedup loop now preserves rows carrying synthetic
  MAPs (status rows, list-of-maps).
- `Expression::CollectSubquery` AST + parser disambiguation against
  `collect(expr)` aggregation + projection-evaluator emits `LIST<T>` /
  `LIST<MAP>` / aggregating-inner / empty-list semantics.
- 13 CALL executor tests + 7 COLLECT evaluator tests.
- Multi-worker `IN CONCURRENT TRANSACTIONS` returns
  `ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`; savepoint-bracketed atomic
  rollback (§3) is the next follow-up.
- Parser §1 / §2 already complete (16 unit tests pass).

## 1. Grammar — `CALL {} IN TRANSACTIONS`

- [x] 1.1 Tokenise `IN TRANSACTIONS`, `OF N ROWS`, `CONCURRENT TRANSACTIONS`
- [x] 1.2 Tokenise `REPORT STATUS AS ident`
- [x] 1.3 Tokenise `ON ERROR CONTINUE|BREAK|FAIL|RETRY`
- [x] 1.4 Parser rule for the full clause
- [x] 1.5 Unit tests for every syntactic variant

## 2. AST & Clause Validation

- [x] 2.1 Add `CallInTransactions` AST node with all fields
- [x] 2.2 Validate that the inner subquery is non-empty
- [x] 2.3 Reject `RETURN` in the inner when REPORT STATUS is set (conflicts)
- [x] 2.4 Reject `OF 0 ROWS`, negative values, non-integer literals
- [x] 2.5 Tests for each validation rule

## 3. Batched Transaction Manager

- [ ] 3.1 Create `transaction/batch.rs` with `BatchTx` wrapper
- [ ] 3.2 Commit at every `N` input rows (default 1000)
- [ ] 3.3 Per-batch WAL segment boundary
- [ ] 3.4 Rollback on error, retry according to clause
- [ ] 3.5 Tests including crash mid-batch

## 4. Executor Operator: CallInTransactions

- [x] 4.1 Create `operators/call_subquery.rs` (subsumes call_in_transactions)
- [x] 4.2 Stream input rows into batch buffer
- [x] 4.3 Flush-on-batch-full, flush-on-stream-end
- [x] 4.4 Record per-row status for REPORT STATUS
- [x] 4.5 Tests: small batch, large batch, exact-multiple, remainder

## 5. `ON ERROR` Semantics

- [x] 5.1 `ON ERROR FAIL` (default): abort immediately
- [x] 5.2 `ON ERROR CONTINUE`: log, mark row failed, keep going
- [x] 5.3 `ON ERROR BREAK`: commit current batch, stop cleanly
- [x] 5.4 `ON ERROR RETRY n`: retry the failing batch up to n times
- [x] 5.5 Tests for each path

## 6. Concurrent Transactions

- [ ] 6.1 Parse `IN CONCURRENT TRANSACTIONS OF N ROWS`
- [ ] 6.2 Spawn up to `nexus.cypher.concurrency` parallel workers
- [ ] 6.3 Input rows sharded round-robin across workers
- [ ] 6.4 Per-worker isolation; no shared mutable state
- [ ] 6.5 Tests asserting no lost writes under concurrency

## 7. REPORT STATUS

- [x] 7.1 When REPORT STATUS is set, emit one row per batch
- [x] 7.2 Columns: `started:DATETIME, committed:BOOLEAN, rowsProcessed:INT, err:STRING?`
- [x] 7.3 Allow caller to consume the stream for monitoring
- [x] 7.4 Tests asserting report-row shape

## 8. Nested `CALL {}`

- [x] 8.1 Variable-resolver pushes a new scope for every nested CALL
- [x] 8.2 Support `CALL (var1, var2) { ... }` Cypher 25 import-list form
- [x] 8.3 Reject shadowed-variable conflicts with clear errors
- [x] 8.4 Tests covering at least 3-deep nesting

## 9. `COLLECT {}` Subquery Full Semantics

- [x] 9.1 Support aggregating return: `COLLECT { MATCH (n) RETURN count(n) }`
- [x] 9.2 Support structured row returns: `COLLECT { ... RETURN {a, b} }`
- [x] 9.3 Tests for empty, single, many rows

## 10. openCypher TCK + Diff

- [x] 10.1 Import TCK subquery scenarios (SUB-1..SUB-8 in
            `scripts/compatibility/compatibility-test-queries.cypher`)
- [x] 10.2 Extend Neo4j diff harness with IN TRANSACTIONS tests
- [x] 10.3 Confirm existing diff tests green (workspace lib + integration)

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [x] 11.1 Update `docs/specs/cypher-subset.md` with the new grammar
- [x] 11.2 Add `docs/guides/BULK_INGEST.md` (CALL IN TRANSACTIONS best practices)
- [x] 11.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` (covered via the diff harness scenarios above)
- [x] 11.4 Add CHANGELOG entry "Added CALL IN TRANSACTIONS and nested subqueries"
- [x] 11.5 Update or create documentation covering the implementation
- [x] 11.6 Write tests covering the new behavior
- [x] 11.7 Run tests and confirm they pass
- [x] 11.8 Quality pipeline: fmt + clippy clean (workspace lib 2062 + integration suite green)
