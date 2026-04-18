# Proposal: phase2a_consolidate-oncelock-cypher-data

## Why

First decomposition slice of the old `phase2_consolidate-oncelock-into-app-state`.
`nexus-server/src/api/cypher/mod.rs` carries `EXECUTOR`, `_EXECUTOR_SHARED`,
`ENGINE`, `DATABASE_MANAGER` as `OnceLock<Arc<_>>` globals. `nexus-server/src/api/data.rs`
carries `CATALOG`, `EXECUTOR`, `ENGINE` in the same style. **Every one of those
Arcs is already owned by `NexusServer`** (the struct already passed into every
Axum route via `.with_state()`), so the OnceLocks are pure duplication that
breaks test isolation: two tests that each call `init_engine` collide on the
same global.

## What Changes

- Migrate every handler in `nexus-server/src/api/cypher/` and
  `nexus-server/src/api/data.rs` to accept `State(server):
  State<Arc<NexusServer>>` (or an equivalent extractor) and reach for
  `server.engine`, `server.executor`, `server.database_manager` directly.
- Delete `CATALOG`, `EXECUTOR`, `_EXECUTOR_SHARED`, `ENGINE`, `DATABASE_MANAGER`
  `OnceLock` statics and the matching `init_*` / `get_*` helpers from both
  modules.
- Update `nexus-server/src/main.rs::async_main` to stop calling the deleted
  `api::cypher::init_executor` / `init_engine` / `init_database_manager`
  and `api::data::init_engine`; the values flow through `NexusServer::new`.
- Update every test site in those two modules that relied on
  `api::data::init_engine` / `api::cypher::init_*` to build a real
  `Arc<NexusServer>` instead.
- `api::cypher` exposes whatever helper `main.rs` still needs to enable
  the query cache on the executor (currently embedded in
  `init_executor`); move that into a constructor on `Executor`-related
  helpers so the global isn't the only way in.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-server/src/api/cypher/mod.rs`, `execute.rs`, `commands.rs`,
    `tests.rs`
  - `nexus-server/src/api/data.rs` (including its ~10 `#[tokio::test]`
    blocks)
  - `nexus-server/src/main.rs` (the `init_*` calls)
  - Any integration test under `nexus-server/tests/` that imports
    `api::cypher::init_*` or `api::data::init_*` (grep before PR)
- Breaking change: NO — Axum handler signatures already compose via
  `State`; users of the HTTP surface see no difference.
- User benefit: tests in those two modules can run in parallel without
  stepping on shared singletons. Adding a new Cypher-path helper means
  adding a field to `NexusServer`, not inventing a new global.
