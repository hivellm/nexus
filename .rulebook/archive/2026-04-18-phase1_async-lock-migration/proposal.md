# Proposal: phase1_async-lock-migration

## Why

`DatabaseManager` uses `parking_lot::RwLock` and is read/written from inside
Axum `async fn` handlers without `spawn_blocking`. Every HTTP request holding
the lock blocks the tokio worker thread that scheduled it. Under concurrent
load this starves the runtime — observed symptom during the `fix/memory-leak-v1`
debugging session was the container dropping requests long before hitting any
memory limit. This is the single largest throughput gotcha in the server today.

## What Changes

- Migrate `DatabaseManager` (and any co-located helpers that hold the same
  lock) from `parking_lot::RwLock` to `tokio::sync::RwLock`.
- Update every handler call site to `.read().await` / `.write().await`.
- Where a synchronous consumer of `DatabaseManager` still exists in
  non-async code, wrap the hot path in `tokio::task::spawn_blocking` or
  expose a `blocking_read()` helper that is never called from async
  context.
- Add a clippy-enforced guard (custom lint or `#[deny(await_holding_lock)]`
  equivalent) so the regression can't return silently.

## Impact

- Affected specs: none (internal refactor of how locks are taken)
- Affected code:
  - `nexus-server/src/api/cypher.rs:1815, 1848`
  - `nexus-server/src/api/database.rs:67` (+ other `get_database_manager()` call sites)
  - `nexus-core/src/database.rs` and anywhere `Arc<RwLock<DatabaseManager>>` is declared
- Breaking change: NO (behaviour-preserving refactor)
- User benefit: higher sustained request throughput, no thread starvation
  under concurrent Cypher load
