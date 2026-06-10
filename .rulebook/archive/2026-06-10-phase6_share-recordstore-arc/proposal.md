# Proposal: phase6_share-recordstore-arc

Source: GitHub issue #16 (https://github.com/hivellm/nexus/issues/16)

## Why
`Engine::refresh_executor` (`crates/nexus-core/src/engine/mod.rs:1468`)
runs at the tail of every write and rebuilds the executor via
`Executor::new` -> `ExecutorShared::new` -> `RecordStore::clone`
(`storage/mod.rs:1708`), which re-opens and mmaps `nodes.store` +
`rels.store` (and adjacency store if present) — 2 to 6 file-open + mmap
syscalls per write — plus throwaway allocation of several managers. Root
cause: `engine.storage` is a plain `RecordStore` (mod.rs:156), not a
shared `Arc<RwLock<RecordStore>>`, so the executor's `shared.store` is a
structurally separate copy that must be resynced after each engine write.
This is the top throughput ceiling for sustained ingest.

## What Changes
- Lift `engine.storage` to `Arc<RwLock<RecordStore>>` (or an equivalent
  shared handle) and pass the SAME Arc into `ExecutorShared`, so the
  engine and executor read/write one store.
- Reduce `refresh_executor` to swapping the already-Arc-wrapped index
  handles (label_index, knn_index) — eliminating the per-write
  `RecordStore::clone` mmap syscalls and the throwaway manager allocation.
- Audit all `refresh_executor` call sites (write/create/delete/commit/
  rollback) and remove any now-redundant ones.

## Impact
- Affected specs: storage / executor handle sharing
- Affected code: `crates/nexus-core/src/engine/mod.rs` (storage field,
  refresh_executor), `crates/nexus-core/src/executor/{shared.rs,engine.rs}`,
  `crates/nexus-core/src/storage/mod.rs` (RecordStore handle)
- Breaking change: NO (internal); must preserve read/write correctness and
  the per-write durability semantics
- User benefit: large drop in per-write CPU under sustained ingest; removes
  the dominant write-throughput ceiling.

## Notes
- Audit finding #2. Larger/structural change — sequence carefully and keep
  the full test suite green (concurrency + durability paths). Pairs with #15.
