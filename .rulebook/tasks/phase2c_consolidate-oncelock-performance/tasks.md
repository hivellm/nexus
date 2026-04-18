## 1. Implementation
- [ ] 1.1 Audit every handler under `api/performance.rs` and `api/mcp_performance.rs` that reaches for `QUERY_STATS`, `PLAN_CACHE`, `DBMS_PROCEDURES`, `MCP_TOOL_STATS`, `MCP_TOOL_CACHE`; confirm the types are `Arc<QueryStatistics>`, `Arc<QueryPlanCache>`, `Arc<DbmsProcedures>`, `Arc<McpToolStatistics>`, `Arc<McpToolCache>`
- [ ] 1.2 Extend `NexusServer` with the five fields plus their builder arguments; update `NexusServer::new` (and every caller: `main.rs`, resp3/rpc test fixtures)
- [ ] 1.3 Move the configuration logic currently embedded in `init_performance_monitoring` / `init_mcp_performance_monitoring` (thresholds, capacities, TTLs) into helpers the server constructor can call with the values `main.rs` already has
- [ ] 1.4 Migrate handlers to `State<Arc<NexusServer>>`; replace `get_query_stats()` / `get_plan_cache()` / `get_dbms_procedures()` / `get_mcp_tool_stats()` / `get_mcp_tool_cache()` with `server.query_stats`, etc.
- [ ] 1.5 Delete the five OnceLock statics and the `init_*` / `get_*` helpers
- [ ] 1.6 Remove the init calls from `main.rs::async_main`; the values flow through `NexusServer::new`
- [ ] 1.7 `cargo +nightly build -p nexus-server` + `cargo +nightly clippy -p nexus-server --all-targets -- -D warnings` clean

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 2.1 Update or create documentation covering the implementation: note in `docs/ARCHITECTURE.md` that perf monitoring state is owned by `NexusServer` and how to add a new monitored dimension
- [ ] 2.2 Write tests covering the new behavior: a `#[tokio::test]` that boots two separate `NexusServer`s, emits a query via server A, and asserts server B's `QueryStatistics` is still empty
- [ ] 2.3 Run tests and confirm they pass: `cargo +nightly test -p nexus-server --lib api::performance` + `... api::mcp_performance`
