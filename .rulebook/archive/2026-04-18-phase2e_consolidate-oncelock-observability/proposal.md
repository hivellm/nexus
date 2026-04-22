# Proposal: phase2e_consolidate-oncelock-observability

## Why

Final decomposition slice of the old `phase2_consolidate-oncelock-into-app-state`.
After phase2a–d land, `api/health.rs::START_TIME` and
`api/prometheus.rs::METRICS` are the last remaining OnceLock statics in the
server crate. They cap the refactor and let the whole "no globals" invariant
be enforced by a grep in CI.

## What Changes

- Move `START_TIME: Instant` onto `NexusServer` (captured at construction).
- Move `PrometheusMetrics` onto `NexusServer` (replacing the public
  `METRICS` OnceLock); adjust `record_query` / `record_cache_hit` /
  `increment_connections` call sites to go through `server.metrics`.
- Migrate the `/health` / `/metrics` / `/prometheus` handlers to take
  `State<Arc<NexusServer>>`.
- Delete both OnceLocks and the `init` / `get_metrics` helpers; delete
  `api::prometheus::init()` call from `main.rs`.
- Add a CI regression-check snippet (or a `#[test]` under
  `nexus-server/tests/no_oncelock_globals.rs`) that greps the
  `nexus-server/src/api` tree for `static.*OnceLock` and fails if any
  match resurfaces.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-server/src/lib.rs`
  - `nexus-server/src/api/health.rs`
  - `nexus-server/src/api/prometheus.rs` (moves the struct fields onto
    `NexusServer`, keeps `format_prometheus` as a method)
  - `nexus-server/src/main.rs`
  - NEW: `nexus-server/tests/no_oncelock_globals.rs` (guard test)
- Breaking change: NO (HTTP endpoints unchanged).
- User benefit: every piece of server-wide state lives on one struct;
  the guard test prevents the anti-pattern from sneaking back in.
