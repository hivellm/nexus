# Proposal: phase6_slow-query-log-and-active-queries

## Why

When `cortex-nexus` saturated 100% CPU on 2026-05-04, the operator had **no way to discover which query was running**. The state was:

- `/cypher` requests timed out at 5–10s.
- `/stats` requests timed out at 5–10s.
- `docker logs --since 60s` returned 0 lines (logs only fire on query completion, and nothing was completing).
- `docker top` showed a single `nexus-server` process at 100% CPU.
- `CALL db.indexes()` eventually returned (15ms), proving the server was alive — just stuck on a long scan it would not log until done.

Triage required reading 5 minutes of historical INFO logs and inferring the query pattern from `MATCH ... CONTAINS $q OR ... CONTAINS $q LIMIT 50`. With the planner notification (sister task `phase6_merge-unindexed-property-warning`), operators learn about missing indexes; this task gives them the runtime visibility to identify which query is currently wedged.

Two missing capabilities:

1. **Slow-query log**: queries exceeding a threshold (default 1s) should log at WARN with `{query, parameters_redacted, elapsed_ms, rows_so_far}` on a periodic tick *while still executing*, not only on completion. The current "Query executed successfully in 15ms" line fires post-completion — useless when completion never happens.
2. **`SHOW QUERIES` / `/admin/queries`**: an introspection endpoint listing currently-executing queries with `{query_id, query_text, elapsed_ms, started_at, client_addr}`. Neo4j supports `SHOW TRANSACTIONS` / `dbms.listQueries()` for the same reason.

Without these, the only diagnostic options are to attach `strace` to the binary (impossible in distroless without `--privileged`) or kill the container (loses the evidence).

## What Changes

- Per-query execution context registers itself on start in a `Mutex<HashMap<QueryId, ActiveQueryEntry>>` and removes itself on drop (RAII guard) — drop-safe so panics/cancellations don't leak entries.
- Background tokio task ticks every `NEXUS_SLOW_QUERY_TICK_MS` (default 1000ms) and emits a WARN log for any active query whose elapsed time exceeds `NEXUS_SLOW_QUERY_THRESHOLD_MS` (default 1000ms). One log per query per tick to avoid log spam.
- New endpoint `GET /admin/queries` returns the active-query map as JSON. Auth-gated under existing admin middleware.
- New Cypher meta-procedure `CALL nexus.queries.list()` returns the same data through the Cypher path so the CLI / SDKs already speak it.

Out of scope:
- Killing in-flight queries (`CALL nexus.queries.kill(id)` — needs cooperative cancellation across the executor; separate task).
- Per-user / per-database filtering on `SHOW QUERIES` — single-tenant use case for now.

## Impact

- Affected specs: `crates/nexus-core/src/executor` (active-query registry), `crates/nexus-server/src/api` (admin endpoint).
- Affected code:
  - `crates/nexus-core/src/executor/active_queries.rs` (new) — registry + RAII guard.
  - `crates/nexus-core/src/executor/mod.rs` — register/deregister hooks around the query execution loop.
  - `crates/nexus-server/src/api/admin/queries.rs` (new) — HTTP endpoint.
  - `crates/nexus-server/src/main.rs` — spawn slow-query-tick task.
  - `crates/nexus-core/src/executor/procedures/` — `nexus.queries.list` registration.
  - `crates/nexus-cli/src/commands/` — optional `nexus queries list` subcommand for ergonomic triage.
- Breaking change: NO. Pure addition. Default thresholds are configurable via env, and the tick task can be disabled by setting `NEXUS_SLOW_QUERY_THRESHOLD_MS=0`.
- User benefit: a wedged Nexus is now triagable in seconds — a single `curl /admin/queries` returns the offending query text and its elapsed time. Slow-query log gives the same answer in `docker logs` for environments where the HTTP endpoint is unreachable.
