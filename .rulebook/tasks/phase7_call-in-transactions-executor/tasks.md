## 1. Implementation
- [ ] 1.1 Read `phase6_opencypher-subquery-transactions` task and existing AST/grammar
- [ ] 1.2 Wire the AST `CallInTransactions` node into a new physical operator that opens fresh transactions per batch
- [ ] 1.3 Implement chunking: split UNWIND input into batches of `n ROWS` (default 1000)
- [ ] 1.4 Implement commit-between-chunks with proper visibility for subsequent chunks
- [ ] 1.5 Implement `ON ERROR CONTINUE` (collect errors, proceed)
- [ ] 1.6 Implement `ON ERROR BREAK` (stop cleanly, return partial result)
- [ ] 1.7 Implement `ON ERROR FAIL` (abort whole call; default)
- [ ] 1.8 Implement `REPORT STATUS AS <var>` row emission with `{ committed, transactionId, errorMessage, started, finished }`
- [ ] 1.9 Reject nested `CALL ... IN TRANSACTIONS` with clear error
- [ ] 1.10 Add executor tests for each shape
- [ ] 1.11 Run Neo4j diff-suite — confirm 300/300 still passes
- [ ] 1.12 Update `docs/specs/cypher-subset.md` to mark feature as supported

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
