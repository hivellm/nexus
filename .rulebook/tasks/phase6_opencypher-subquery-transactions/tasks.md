# Implementation Tasks — Subquery Transactions + Collect + Nesting

## Status — slice-1 (read-only inner) + slice-5 (COLLECT { } subquery)

- Operator `Operator::CallSubquery` wired through planner + dispatch.
- Inner subquery executes per outer row; outer × inner join produced.
- IN TRANSACTIONS / ON ERROR / REPORT STATUS / write-bearing inner all
  surface typed errors (`ERR_CALL_IN_TX_PENDING_SLICE2`,
  `ERR_CALL_SUBQUERY_WRITE_INNER_UNSUPPORTED`) until the re-entrant
  executor lands in the IN TRANSACTIONS slice.
- `Expression::CollectSubquery` AST + parser disambiguation against
  `collect(expr)` aggregation + projection-evaluator emit `LIST<T>` /
  `LIST<MAP>` / aggregating-inner / empty-list semantics.
- 7 CALL executor tests + 7 COLLECT evaluator tests.
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

- [ ] 4.1 Create `operators/call_in_transactions.rs`
- [ ] 4.2 Stream input rows into batch buffer
- [ ] 4.3 Flush-on-batch-full, flush-on-stream-end
- [ ] 4.4 Record per-row status for REPORT STATUS
- [ ] 4.5 Tests: small batch, large batch, exact-multiple, remainder

## 5. `ON ERROR` Semantics

- [ ] 5.1 `ON ERROR FAIL` (default): abort immediately
- [ ] 5.2 `ON ERROR CONTINUE`: log, mark row failed, keep going
- [ ] 5.3 `ON ERROR BREAK`: commit current batch, stop cleanly
- [ ] 5.4 `ON ERROR RETRY n`: retry the failing batch up to n times
- [ ] 5.5 Tests for each path

## 6. Concurrent Transactions

- [ ] 6.1 Parse `IN CONCURRENT TRANSACTIONS OF N ROWS`
- [ ] 6.2 Spawn up to `nexus.cypher.concurrency` parallel workers
- [ ] 6.3 Input rows sharded round-robin across workers
- [ ] 6.4 Per-worker isolation; no shared mutable state
- [ ] 6.5 Tests asserting no lost writes under concurrency

## 7. REPORT STATUS

- [ ] 7.1 When REPORT STATUS is set, emit one row per batch
- [ ] 7.2 Columns: `started:DATETIME, committed:BOOLEAN, rowsProcessed:INT, err:STRING?`
- [ ] 7.3 Allow caller to consume the stream for monitoring
- [ ] 7.4 Tests asserting report-row shape

## 8. Nested `CALL {}`

- [ ] 8.1 Variable-resolver pushes a new scope for every nested CALL
- [ ] 8.2 Support `CALL (var1, var2) { ... }` Cypher 25 import-list form
- [ ] 8.3 Reject shadowed-variable conflicts with clear errors
- [ ] 8.4 Tests covering at least 3-deep nesting

## 9. `COLLECT {}` Subquery Full Semantics

- [x] 9.1 Support aggregating return: `COLLECT { MATCH (n) RETURN count(n) }`
- [x] 9.2 Support structured row returns: `COLLECT { ... RETURN {a, b} }`
- [x] 9.3 Tests for empty, single, many rows

## 10. openCypher TCK + Diff

- [ ] 10.1 Import TCK subquery scenarios
- [ ] 10.2 Extend Neo4j diff harness with IN TRANSACTIONS tests
- [ ] 10.3 Confirm 300/300 existing diff tests green

## 11. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 11.1 Update `docs/specs/cypher-subset.md` with the new grammar
- [ ] 11.2 Add `docs/guides/BULK_INGEST.md` (CALL IN TRANSACTIONS best practices)
- [ ] 11.3 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 11.4 Add CHANGELOG entry "Added CALL IN TRANSACTIONS and nested subqueries"
- [ ] 11.5 Update or create documentation covering the implementation
- [ ] 11.6 Write tests covering the new behavior
- [ ] 11.7 Run tests and confirm they pass
- [ ] 11.8 Quality pipeline: fmt + clippy + ≥95% coverage
