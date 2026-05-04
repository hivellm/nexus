## 1. Active-query registry

- [ ] 1.1 Create `crates/nexus-core/src/executor/active_queries.rs` with `ActiveQueryRegistry { entries: Mutex<HashMap<QueryId, ActiveQueryEntry>> }` and a `RegisteredQuery` RAII guard whose `Drop` impl removes the entry
- [ ] 1.2 `ActiveQueryEntry { query_id, query_text, parameters_redacted, started_at: Instant, client_addr: Option<SocketAddr>, database: String }`
- [ ] 1.3 Hook `register_query(...) -> RegisteredQuery` into the execution path in `crates/nexus-core/src/executor/mod.rs` (single call site at the top of the per-query handler)
- [ ] 1.4 Unit test: registry populates on register, clears on guard drop, clears on panic during query execution

## 2. Slow-query log tick

- [ ] 2.1 In `crates/nexus-server/src/main.rs`, spawn a tokio task that ticks every `NEXUS_SLOW_QUERY_TICK_MS` (default 1000)
- [ ] 2.2 On each tick, iterate the registry and emit a WARN log for any entry whose `elapsed >= NEXUS_SLOW_QUERY_THRESHOLD_MS` (default 1000); include `query_id`, `elapsed_ms`, query text truncated to 512 chars
- [ ] 2.3 Track per-query "last logged tick" so the same query is not re-logged at every tick — log on first crossing of threshold, then once every `NEXUS_SLOW_QUERY_REPEAT_SECS` (default 30) for as long as it is still running
- [ ] 2.4 Disable the tick entirely when `NEXUS_SLOW_QUERY_THRESHOLD_MS=0`

## 3. HTTP admin endpoint

- [ ] 3.1 Add `GET /admin/queries` in a new `crates/nexus-server/src/api/admin/queries.rs`
- [ ] 3.2 Response shape: `{"queries": [{"query_id", "query_text", "elapsed_ms", "started_at", "client_addr", "database"}]}`
- [ ] 3.3 Mount under existing admin auth middleware (same gate as `/admin/*` if present, otherwise the standard auth middleware)
- [ ] 3.4 Integration test: spawn a deliberately slow query in one tokio task, hit `/admin/queries`, assert the entry is present with `elapsed_ms > 0`

## 4. Cypher procedure

- [ ] 4.1 Register `nexus.queries.list` in the procedures registry (`crates/nexus-core/src/executor/procedures/`) returning the same fields as the HTTP endpoint
- [ ] 4.2 Verify it appears in `CALL dbms.procedures()` (or the Nexus equivalent)
- [ ] 4.3 Unit test: procedure returns N entries when N queries are active

## 5. Parameter redaction

- [ ] 5.1 Define a redaction policy: parameters longer than 256 chars are truncated with `<<truncated N bytes>>`; binary parameters become `<<binary N bytes>>`; never log raw parameter values that match patterns from the existing secret-detection layer (if any) — fall back to length-only when in doubt
- [ ] 5.2 Apply the same redaction to slow-query log lines and to `/admin/queries` responses

## 6. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 6.1 Update or create documentation covering the implementation — `docs/operations/RUNBOOK.md` (create if missing) under a "Diagnosing a wedged server" section documenting the new endpoint, procedure, env vars, and redaction policy; reference the `phase6_merge-unindexed-property-warning` notification as the upstream signal
- [ ] 6.2 Write tests covering the new behavior — items 1.4, 2.x, 3.4, 4.3 with coverage ≥95% on the new modules
- [ ] 6.3 Run tests and confirm they pass — `cargo +nightly fmt --all`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace --verbose` all green
