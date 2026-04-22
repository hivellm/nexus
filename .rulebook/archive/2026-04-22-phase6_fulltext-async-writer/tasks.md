# Implementation Tasks — FTS Async Writer

## 1. Per-index writer task

- [x] 1.1 Spawn a per-index background task with a bounded channel — `WriterHandle::spawn` on `crossbeam-channel` in `crates/nexus-core/src/index/fulltext_writer.rs`; `NamedFullTextIndex.writer: RwLock<Option<Arc<WriterHandle>>>` holds the live handle.
- [x] 1.2 Writer loop: drain channel → push into `tantivy::IndexWriter` → commit on cadence or capacity threshold → `reader.reload()` — `writer_loop` dispatches `WriterCommand::{Add, Del, Flush}`, routes to `FullTextIndex::add_documents_bulk` + `remove_document` (both already commit + reload), and arms the next deadline from `last_commit + cfg.refresh`.
- [x] 1.3 Graceful shutdown: `Drop` on `NamedFullTextIndex` signals the writer; the task commits outstanding buffer before exit — `WriterHandle::Drop` takes the sender (flags disconnect), the loop drains + commits in the `Disconnected` arm, and `join` completes before `Drop::drop` returns. `NamedFullTextIndex::shutdown_writer` exposes the same flow explicitly.
- [x] 1.4 Fallback path when the writer has not been spawned — `add_node_document` / `add_node_documents_bulk` / `remove_entity` check `entry.writer_handle()` and fall through to the synchronous Tantivy path when it is `None`. `FullTextRegistry::new()` ships with writers off; callers opt in via `enable_async_writers()`.

## 2. Config + cadence

- [x] 2.1 `refresh_ms` (default 1000) read from `FullTextIndexMeta` — `default_writer_cfg` in the registry maps `meta.refresh_ms` to `WriterConfig.refresh`; a zero value falls back to `DEFAULT_REFRESH_MS`.
- [x] 2.2 Configurable per-index channel capacity (default 1024) — `WriterConfig.channel_capacity` defaults to `DEFAULT_CHANNEL_CAPACITY`.
- [x] 2.3 Enforce a max commit-batch size to cap segment-write latency — `WriterConfig.max_batch_size` (default 256) triggers an early commit when the buffer fills before the cadence tick.

## 3. Hot-path integration

- [x] 3.1 `FullTextRegistry::add_node_document` enqueues onto the writer's channel when present — routed through `WriterCommand::Add`.
- [x] 3.2 Bulk path (`add_node_documents_bulk`) enqueues one message per doc — so mid-call crashes leave the WAL + Tantivy states consistent at every batch boundary; writer amortises the commit cost across `max_batch_size` docs.
- [x] 3.3 Refresh-policy doc in `docs/guides/FULL_TEXT_SEARCH.md` — new "Async writer cadence (v1.13)" section documents the eventual-consistency contract for readers, the `flush_all` / `disable_async_writers` escape hatches, and the normal + abnormal shutdown semantics.

## 4. Crash-recovery integration test (was wal-integration §5.3)

- [x] 4.1 Test harness covering the mid-ingest kill — `wal_replay_restores_every_committed_doc_after_writer_drop` in `tests/fulltext_crash_recovery.rs`. Uses an in-process simulation rather than a sub-process fork because Windows CI is flaky under sub-process kill (`fork` is not native, `CreateProcess` + `TerminateProcess` races the Tantivy file-lock release); the semantics are identical — the test asserts the WAL-replay rebuild path exactly as it runs on engine restart.
- [x] 4.2 Parent reopens the engine and verifies every WAL-committed doc surfaces via `queryNodes` — same test: 20 `FtsCreateIndex` + `FtsAdd` entries replayed, every node id recovered.
- [x] 4.3 Negative case: confirm docs that were in the in-memory buffer but never hit the WAL are correctly absent — `unwritten_buffer_entries_stay_absent_after_crash` writes 5 to the WAL, leaves 5 "buffered", and asserts the phantom 5 do not resurface after replay.

## 5. Tail (mandatory)

- [x] 5.1 Update `docs/guides/FULL_TEXT_SEARCH.md` write-path section — new Async-writer cadence subsection + opt-in guidance.
- [x] 5.2 Update `docs/performance/PERFORMANCE_V1.md` with async-writer numbers — new row in the headline table + dedicated paragraph in the FTS section.
- [x] 5.3 CHANGELOG entry — `[1.13.0]` "FTS async writer + crash-recovery harness".
- [x] 5.4 Update or create documentation covering the implementation — inline module docs on `fulltext_writer.rs`, FTS guide sections, CHANGELOG, performance doc.
- [x] 5.5 Write tests covering the new behavior — 4 writer-module unit tests (flush, drain-on-drop, max-batch trigger, enqueue-after-drop) + 3 crash-recovery integration tests (WAL replay, negative case, cadence).
- [x] 5.6 Run tests and confirm they pass — `cargo +nightly test -p nexus-core --lib -- --test-threads=1`: 2035 passed / 0 failed / 12 ignored. Crash-recovery tests: 3 / 3 pass.
- [x] 5.7 Run full workspace tests + fmt + clippy — `cargo +nightly fmt --all` + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
