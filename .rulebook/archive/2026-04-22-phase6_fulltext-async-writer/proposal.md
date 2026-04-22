# Proposal: phase6_fulltext-async-writer

## Why

`phase6_fulltext-wal-integration` shipped WAL op-codes, on-disk
catalogue persistence, the replay dispatcher, and auto-populate /
refresh / evict hooks for `CREATE` / `SET` / `REMOVE` / `DELETE`,
but left §3 of that task's original spec parked: a per-index
single-writer task with `refresh_ms` cadence and graceful
shutdown.

The current FTS write path commits + reloads synchronously on
every `add_node_document` call. The bulk-ingest path
(`add_node_documents_bulk`) opens one Tantivy writer and commits
once, delivering ≈60 k docs/sec on the reference hardware
(phase6_fulltext-benchmarks). Interactive writes through Cypher
`CREATE` / `SET` therefore pay a per-commit cost on the hot path —
it's correct and fast enough to beat the >5 k docs/sec SLO, but
under hundreds of concurrent writers Tantivy's segment-flush
latency dominates and the commit-per-doc cadence becomes the
bottleneck.

Shipping an async writer per index lets the hot path enqueue and
return, with a background task batching + committing on a
configurable cadence. Required for high-concurrency workloads and
for the crash-during-bulk-ingest integration test that the WAL
integration task's §5.3 deferred.

## What Changes

1. Per-index async writer task with a bounded channel
   (`tokio::sync::mpsc` or `crossbeam-channel`).
2. `refresh_ms` configuration (default 1000) drives the reader-
   reload cadence inside the writer loop.
3. Graceful shutdown: drain the channel on `Drop`, commit any
   buffered docs before exit.
4. Hot-path `add_node_document` / `add_node_documents_bulk` now
   enqueue onto the writer's channel instead of committing
   inline. Fallback to the synchronous path when the writer
   hasn't been spawned.
5. Crash-during-bulk-ingest integration test: fork a child
   process, start bulk ingest, kill mid-way, restart and verify
   every WAL-committed row is visible in the FTS backend after
   `apply_wal_entry` replay.

## Impact

- Affected specs: `docs/guides/FULL_TEXT_SEARCH.md` write-path
  section, `docs/performance/PERFORMANCE_V1.md` FTS table.
- Affected code: `crates/nexus-core/src/index/fulltext.rs`,
  `fulltext_registry.rs`; new test harness under
  `crates/nexus-core/tests/fulltext_crash_recovery.rs`.
- Breaking change: NO (sync path stays as a fallback).
- User benefit: higher sustained ingest under concurrent writers;
  the final crash-recovery guarantee closed end-to-end.
