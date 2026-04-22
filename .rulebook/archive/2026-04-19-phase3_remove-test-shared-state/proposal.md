# Proposal: phase3_remove-test-shared-state

## Why

`nexus-core/src/executor/mod.rs:14313` defines:

```rust
static INIT: Once = Once::new();
static SHARED_STORE: Mutex<Option<RecordStore>> = Mutex::new(None);
```

…and `impl Default for Executor` lazily initialises the store, then calls
`std::mem::forget(temp_dir)` to keep the temp directory alive. Consequences:

- Tests that rely on `Executor::default()` share a single record store,
  which means ordering-dependent failures the instant anyone adds a test
  that mutates state.
- `mem::forget` is a deliberate, permanent leak — fine for a test binary
  if you accept the cost, but it also masks engine-drop bugs during
  development.
- Under `cargo test --workspace` with `test-threads = num_cpus` this shared
  state plus each test's `TempDir` is a contributing factor to the RSS
  explosion that was originally attributed to a runtime leak.

## What Changes

- Delete `SHARED_STORE` / `INIT` / `mem::forget(temp_dir)`.
- Rewrite `impl Default for Executor` to construct a fresh `RecordStore`
  backed by a `tempfile::TempDir` held as a field of a new helper struct
  (so it drops with the executor).
- Audit other `static *_STORE` / `static *_ENGINE` patterns under tests
  and apply the same fix.

## Impact

- Affected specs: none
- Affected code:
  - `nexus-core/src/executor/mod.rs:14313` and the surrounding
    `impl Default for Executor`
- Breaking change: tests that depend on the shared store's cross-test
  state will need to be updated (they were already wrong)
- User benefit: parallel-safe test runs; lower RSS under `cargo test
  --workspace`; removes a deliberate leak
