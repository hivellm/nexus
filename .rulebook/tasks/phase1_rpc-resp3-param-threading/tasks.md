## 1. Implementation
- [x] 1.1 RPC: `protocol/rpc/dispatch/cypher.rs` — write queries now route through `execute_cypher_with_params(&query, params)` (line ~270); module docs updated
- [x] 1.2 RESP3: `protocol/resp3/command/cypher.rs` — `run_cypher` threads its params via `execute_cypher_with_params` (line ~104); plan-only EXPLAIN path intentionally unchanged

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation — CHANGELOG `[Unreleased — 2.5.0]` Fixed entry added
- [x] 2.2 Write tests — `cypher_parameterized_write_persists_value` (RPC) and `cypher_with_parameterized_write_persists_value` (RESP3): parameterized CREATE then re-read == value
- [x] 2.3 Run tests and confirm they pass — 2/2 green (`cargo +nightly test -p nexus-server --lib parameterized_write_persists`); clippy `--all-targets -D warnings` clean; fmt clean
