# Proposal: phase2c_consolidate-oncelock-performance

## Why

Third decomposition slice of the old `phase2_consolidate-oncelock-into-app-state`.
`api/performance.rs` owns `QUERY_STATS`, `PLAN_CACHE`, `DBMS_PROCEDURES`, and
`api/mcp_performance.rs` owns `MCP_TOOL_STATS` + `MCP_TOOL_CACHE`. Unlike the
phase2a/phase2b slices, these values are **not** already on `NexusServer` — the
migration has to extend the state struct, then swap the handlers.

## What Changes

- Extend `NexusServer` with `query_stats: Arc<QueryStatistics>`,
  `plan_cache: Arc<QueryPlanCache>`, `dbms_procedures: Arc<DbmsProcedures>`,
  `mcp_tool_stats: Arc<McpToolStatistics>`,
  `mcp_tool_cache: Arc<nexus_core::performance::McpToolCache>` (names match
  the existing types).
- Move the construction currently done inside
  `api::performance::init_performance_monitoring(...)` and
  `api::mcp_performance::init_mcp_performance_monitoring(...)` into
  `NexusServer::new` (or a short helper called from `main.rs::async_main`).
- Migrate every handler in both modules to take
  `State(server): State<Arc<NexusServer>>` and read the new fields.
- Delete the five OnceLock statics and their `init_*` / `get_*` helpers.
- Drop the `api::performance::init_performance_monitoring(...)` and
  `api::mcp_performance::init_mcp_performance_monitoring(...)` calls from
  `main.rs`; the values flow through `NexusServer::new`.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-server/src/lib.rs` (`NexusServer` struct + constructor)
  - `nexus-server/src/api/performance.rs`
  - `nexus-server/src/api/mcp_performance.rs`
  - `nexus-server/src/main.rs` (init calls)
- Breaking change: `NexusServer::new` gains new parameters. Internal crate
  only — no public HTTP surface change.
- User benefit: perf-monitoring state lives on the same handle as every
  other piece of shared server state; tests can swap out the stats /
  caches without touching process-wide globals.
