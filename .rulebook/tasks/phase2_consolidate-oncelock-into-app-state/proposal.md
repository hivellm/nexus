# Proposal: phase2_consolidate-oncelock-into-app-state

## Why

`nexus-server/src/api/` declares ~20 `OnceLock<Arc<T>>` globals across files
(`cypher.rs:18`, `data.rs:13`, `graph_correlation.rs`, `performance.rs:18-24`,
`comparison.rs:16`, `knn.rs`, …). Every module carries its own `init_*` +
`get_*` pair. Consequences:

1. **Test isolation is impossible** — two tests that each call `init_engine`
   collide on the same `OnceLock`.
2. **Init order is fragile** — `main.rs` has a long prose sequence of
   `init_X(...)?` calls that must be kept in a specific order.
3. **Adding a new module starts a new singleton** — an anti-pattern compounds
   itself.

## What Changes

- Create `nexus-server::state::AppState { engine, executor, db_manager,
  dbms_procedures, query_stats, plan_cache, ... }` — an `Arc`-backed struct.
- Construct it once in `main.rs` and pass it to Axum via
  `.with_state(app_state)` / `.layer(Extension(app_state.clone()))`.
- Replace handler-side `crate::api::performance::get_query_stats()` calls
  with `State(state): State<Arc<AppState>>` extractors.
- Delete the `OnceLock<_>` + `init_*` / `get_*` scaffolding once every
  caller is migrated.

## Impact

- Affected specs: none
- Affected code: every file under `nexus-server/src/api/` that declares
  a `static X: OnceLock<_>` (roughly 20 sites) plus `nexus-server/src/main.rs`
- Breaking change: NO (behaviour preserved; purely structural)
- User benefit: tests become parallel-safe; adding a new subsystem is
  changing one struct field instead of inventing a new singleton
