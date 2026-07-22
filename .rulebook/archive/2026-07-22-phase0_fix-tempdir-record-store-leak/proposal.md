# Proposal: phase0_fix-tempdir-record-store-leak

**Priority: CRITICAL (resource leak) — `Executor::default()` deliberately leaks
a system-temp directory holding the full record-store set on EVERY call via
`tempfile::tempdir().keep()`, and the record stores have no `Drop` to release
their mmaps, so the directories accumulate forever and fill the disk.** Reported
by a user whose `C:\Users\...\AppData\Local\Temp` had 21,000+ leaked
`.tmpXXXXXX` directories (~100 GB).

## Why

`Executor::default()`
(`crates/nexus-core/src/executor/dispatch.rs`, ~line 1466):

```rust
let temp_dir = tempfile::tempdir().expect(...);   // %TEMP%\.tmpXXXXXX
let path = temp_dir.keep();                         // disarms removal — leaks forever
let store = RecordStore::new(&path)...;
```

`.keep()` consumes the `TempDir` and suppresses its destructor, so the directory
(containing `nodes.store`, `rels.store`, `adjacency.incoming.store`,
`adjacency.outgoing.store`, `properties.store`) is never removed. The helper is
used by 50+ test files; a single `cargo test` run leaks hundreds of directories,
and repeated runs accumulate tens of thousands. The `.keep()` was used because
the temp dir must outlive the store — `Executor` is `Clone` and clones share the
store's `Arc<RwLock<MmapMut>>`, so no single owner could remove the directory.

Compounding it: `RecordStore`, `PropertyStore`, and `AdjacencyListStore` mmap
their files (`memmap2`) but have NO `Drop` impls, and on Windows a still-mapped
file cannot be deleted — so even RAII temp dirs (`Engine::new`) can fail removal
silently. `TestContext` is NOT the culprit: it isolates temp dirs under
`%TEMP%\nexus-test-tmp\` with a deferred-retry + stale-sweep and cleans up
correctly; the leak is from the paths that bypass it.

## What Changes

Tie the temp directory's lifetime to the store via a reference-counted cleanup
guard, so it is removed exactly when the last store clone drops — cross-platform
RAII, with no time-based sweep (server-safe):

- Add a `TempDirGuard` whose `Drop` best-effort `remove_dir_all`s its path.
- `RecordStore` gains `_cleanup: Option<Arc<TempDirGuard>>`, declared AFTER the
  mmap fields so it drops after them (mmaps unmap before the dir is removed).
  All persistent constructors keep `None` (never auto-delete a data dir).
- Add `RecordStore::new_temporary()` that creates a `nexus-store-`-prefixed temp
  dir and attaches the guard.
- `Executor::default()` uses `RecordStore::new_temporary()` (no more `.keep()`).
- `Engine::new()` uses `RecordStore::new_temporary()` and drops its redundant
  `_temp_dir` field (guard now owns cleanup; safe for the long-running server —
  removal happens only on Engine drop, never on a timer).
- Existing user-visible leaked directories were bulk-removed out of band; this
  fix stops NEW leaks.

## Impact

- Affected specs: none (internal storage lifecycle)
- Affected code: `crates/nexus-core/src/storage/record_store.rs` (+guard field,
  `new_temporary`), a new `TempDirGuard`, `crates/nexus-core/src/executor/dispatch.rs`
  (`Executor::default`), `crates/nexus-core/src/engine/mod.rs` (`Engine::new`)
- Breaking change: NO — internal; `Engine::new()`/`Executor::default()` keep the
  same signatures and now clean up their temp dir on drop
- User benefit: `cargo test` and ephemeral engines no longer leak temp
  directories; the disk-filling accumulation stops
- Related: `RecordStore`/`PropertyStore`/`AdjacencyListStore` missing-`Drop`
  (memmap flush-on-drop) is a durability nicety noted for follow-up; the leak
  itself is fixed by the guard's drop-ordering
