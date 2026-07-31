## 1. Implementation
- [ ] 1.1 Move the `semantic_validation::validate` call from `execute_cypher_with_context` (query_pipeline.rs:186) into the shared body `execute_cypher_ast_with_context`, so `execute_cypher_ast_with_params` (the RPC/Thunder write path) is covered — and confirm the HTTP path does not validate twice
- [ ] 1.2 Audit the RPC read-only autocommit fast path that calls the lock-free executor directly, bypassing `Engine`: either route it through validation or document why it cannot, without reintroducing a re-parse
- [ ] 1.3 Transport-parity test: the same semantically invalid query (one per check — undefined variable, variable type conflict, already-bound variable, misplaced aggregation, negative SKIP/LIMIT, duplicate alias) is rejected with the same error kind over HTTP `/cypher` and over the RPC `CYPHER` command

## 2. Tail (docs + tests — check or waive with tailWaiver)
- [ ] 2.1 Update or create documentation covering the implementation
- [ ] 2.2 Write tests covering the new behavior
- [ ] 2.3 Run tests and confirm they pass
