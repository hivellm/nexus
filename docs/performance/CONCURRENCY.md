# Concurrency Model

This document describes how Nexus handles concurrent access inside the HTTP server, with particular focus on the `DatabaseManager` lock and the tokio runtime.

## The primitives

Nexus mixes two kinds of locks:

- **`parking_lot::RwLock`** — used throughout `nexus-core` and the server. Fast, fair-ish, OS-mutex-free. Acquisition blocks the calling OS thread.
- **`tokio::sync::RwLock`** — used where a handler genuinely needs to await other async work while holding the guard (e.g. the RBAC manager in `NexusServer::rbac`).

The two are not interchangeable: a `parking_lot` guard acquired inside an `async fn` pins the tokio worker that scheduled the task for the entire lock-held window, because the guard cannot suspend. A `tokio::sync` guard releases the worker while it waits, but its sync-context callers have to call `.blocking_read()` / `.blocking_write()`, which panics if ever reached from an async context.

## The `DatabaseManager` rule

`DatabaseManager` is wrapped in `Arc<parking_lot::RwLock<DatabaseManager>>` and is accessed from **both** async HTTP handlers (server) and sync Cypher execution (core/executor). Changing the lock type to `tokio::sync::RwLock` would force every sync consumer in `nexus-core` to go through `.blocking_read()` or restructure into async, which in turn would need every `execute_cypher` caller to either hold a tokio runtime or wrap the call in `spawn_blocking`.

We resolved this as follows:

> **The `DatabaseManager` lock stays `parking_lot::RwLock`. Async handlers that touch it MUST acquire it inside `tokio::task::spawn_blocking`.**

That keeps the lock cheap for the executor (no async overhead on the hot path) while also keeping HTTP handlers from pinning tokio workers during contention.

## The enforcement

`nexus-server/Cargo.toml` sets `clippy::await_holding_lock = "deny"`. Any future code that does

```rust
async fn handler(State(state): State<AppState>) -> Response {
    let manager = state.manager.read(); // parking_lot guard
    some_future().await;                // ← clippy errors here
    // ...
}
```

will fail CI, because the `parking_lot` guard crosses an `.await`. The mitigation is always the same: move the lock+work inside `spawn_blocking`.

```rust
async fn handler(State(state): State<AppState>) -> Response {
    let manager_arc = state.manager.clone();
    let result = tokio::task::spawn_blocking(move || {
        let manager = manager_arc.read();
        manager.do_sync_work()
    })
    .await
    .expect("spawn_blocking panicked");
    // ...
}
```

## Why not migrate the whole lock to `tokio::sync::RwLock`?

Option rejected during the `phase1_async-lock-migration` task. The concrete tradeoff:

| Aspect | Keep `parking_lot` + `spawn_blocking` | Migrate to `tokio::sync::RwLock` |
|---|---|---|
| Files touched | 2 (`api/database.rs`, `api/cypher/commands.rs`) | ~20 (server + nexus-core executor) |
| Executor sync callers | unchanged | must call `.blocking_read()` and only from `spawn_blocking` contexts |
| Lock acquisition cost | ~ns | ~µs (scheduler work on every acquire) |
| Deadlock risk | none added | new — `.blocking_read()` panics if reached from async without `spawn_blocking` |
| Enforcement | `clippy::await_holding_lock` | requires a custom lint to prevent `.read().await` inside sync call chains |

Given the executor holds the lock for sub-microsecond windows on almost every Cypher query, the blocking-pool handoff is a rounding error at the aggregate call rate but eliminates the "one slow read starves 8 tokio workers" failure mode that triggered this task. See commit history on `phase1_async-lock-migration` for the full rationale.

## Locks that ARE `tokio::sync::RwLock`

These are legitimately `tokio::sync::RwLock<_>` because their consumers do `await` while holding the guard (mostly `async`-only subsystems):

- `NexusServer::rbac` — user/role mutations live entirely in async commands (e.g. `execute_user_commands` awaits `check_and_disable_root_if_needed` while holding the write guard).
- Streaming subscriber lists, websocket connection registries, etc.

If you add a new lock ask: *does the critical section call `.await`?* If yes, use `tokio::sync::RwLock`. If no, use `parking_lot::RwLock` and wrap server handler access in `spawn_blocking`.

## Regression test

`nexus-server/src/api/database.rs::tests::test_concurrent_list_databases_does_not_starve_runtime` fires 32 concurrent `list_databases` calls on a 2-worker tokio runtime and asserts they all complete in well under a 30 s pathological timeout. Prior to the `spawn_blocking` migration this test would have revealed the starvation behaviour directly if ever regressed.
