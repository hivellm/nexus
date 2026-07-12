## 1. Implementation
- [ ] 1.1 RPC: `protocol/rpc/dispatch/cypher.rs` — replace `engine.execute_cypher(&query)` with `execute_cypher_with_params(&query, params)` (params already decoded in the request envelope)
- [ ] 1.2 RESP3: `protocol/resp3/command/cypher.rs` — `run_cypher` uses its currently-ignored `_params` via `execute_cypher_with_params`

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation (CHANGELOG "Fixed" entry)
- [ ] 2.2 Write tests: parameterized write over RPC persists and round-trips; same for RESP3 (`CREATE (n {x:$v})` then re-read == value, not null)
- [ ] 2.3 Run tests and confirm they pass (`cargo +nightly test -p nexus-server`, clippy zero warnings)
