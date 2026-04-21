# Implementation Tasks — FTS Async Writer

## 1. Per-index writer task

- [ ] 1.1 Spawn a per-index background task with a bounded channel (`tokio::sync::mpsc` or `crossbeam-channel`)
- [ ] 1.2 Writer loop: drain channel → push into `tantivy::IndexWriter` → commit on cadence or capacity threshold → `reader.reload()`
- [ ] 1.3 Graceful shutdown: `Drop` on `NamedFullTextIndex` signals the writer; the task commits outstanding buffer before exit
- [ ] 1.4 Fallback path when the writer has not been spawned (cluster test mode, etc.) — keep the sync `add_document` / `add_documents_bulk` callable

## 2. Config + cadence

- [ ] 2.1 `refresh_ms` (default 1000) read from `FullTextIndexMeta`
- [ ] 2.2 Configurable per-index channel capacity (default 1024)
- [ ] 2.3 Enforce a max commit-batch size to cap segment-write latency

## 3. Hot-path integration

- [ ] 3.1 `FullTextRegistry::add_node_document` enqueues onto the writer's channel when present
- [ ] 3.2 Bulk path (`add_node_documents_bulk`) enqueues one message per doc (or one bulk message)
- [ ] 3.3 Refresh-policy doc in `docs/guides/FULL_TEXT_SEARCH.md` explains the eventually-consistent reader lag

## 4. Crash-recovery integration test (was wal-integration §5.3)

- [ ] 4.1 Test harness: fork a child process that bulk-ingests against an FTS index, kill mid-way
- [ ] 4.2 Parent reopens the engine and verifies every WAL-committed doc surfaces via `queryNodes`
- [ ] 4.3 Negative case: confirm docs that were in the in-memory buffer but never hit the WAL are correctly absent

## 5. Tail (mandatory)

- [ ] 5.1 Update `docs/guides/FULL_TEXT_SEARCH.md` write-path section
- [ ] 5.2 Update `docs/performance/PERFORMANCE_V1.md` with async-writer throughput numbers
- [ ] 5.3 CHANGELOG entry
- [ ] 5.4 Update or create documentation covering the implementation
- [ ] 5.5 Write tests covering the new behavior (crash harness + refresh cadence)
- [ ] 5.6 Run tests and confirm they pass
- [ ] 5.7 Run full workspace tests + fmt + clippy
