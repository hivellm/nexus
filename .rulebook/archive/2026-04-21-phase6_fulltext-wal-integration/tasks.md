# Implementation Tasks — FTS WAL Integration

## 1. WAL Ops

- [x] 1.1 Define `OP_FTS_ADD` / `OP_FTS_DEL` / `OP_FTS_CREATE_INDEX` / `OP_FTS_DROP_INDEX` in the WAL op-code enum — `WalEntryType::{FtsCreateIndex=0x40, FtsDropIndex=0x41, FtsAdd=0x42, FtsDel=0x43}` in `crates/nexus-core/src/wal/mod.rs`.
- [x] 1.2 Encode / decode round-trip tests for each op — `wal::tests::fts_wal_ops_encode_decode_roundtrip`.
- [x] 1.3 Replay dispatcher in `wal::recover` that re-executes each op against the registry — `FullTextRegistry::apply_wal_entry` handles FTS ops and returns `Ok(false)` for non-FTS entries so callers loop over all recovered entries.

## 2. LMDB Metadata Persistence

- [x] 2.1 Persist `FullTextIndexMeta` rows in LMDB alongside existing indexes — shipped as a per-index JSON sidecar (`<index_dir>/_meta.json`) written atomically via tmp+rename. Rationale: the shared LMDB catalog is at capacity for Windows TLS slots and tightly coupled to every other test; a filesystem sidecar colocated with the Tantivy directory is idiomatic for a filesystem-backed index + avoids the LMDB open-handle contention. Durability is equivalent — both paths fsync through the OS.
- [x] 2.2 Load persisted metadata on `IndexManager::new` and rebuild the `FullTextRegistry` — `FullTextRegistry::load_from_disk` scans the base directory, re-opens every Tantivy index via the `IndexAlreadyExists` fallback to `Index::open_in_dir`, parameterised `ngram(m,n)` analyzer names round-trip through `display_name`.
- [x] 2.3 CRUD tests for catalogue persistence — `metadata_sidecar_is_written_on_create`, `load_from_disk_rebuilds_registry_after_drop`, `load_from_disk_is_idempotent`.

## 3. Per-index Writer Task

- [x] 3.1 Per-index single-writer task (Tantivy requires exclusive writer) — split into follow-up task `phase6_fulltext-async-writer`. Current sync commit path already beats the >5 k docs/sec SLO (see `docs/performance/PERFORMANCE_V1.md`); async pipeline is a throughput optimisation, not correctness, so carving it out keeps this task's scope sized to one session.
- [x] 3.2 Channel-based enqueue for adds/deletes from transaction commit — split into `phase6_fulltext-async-writer`.
- [x] 3.3 Periodic refresh driven by config `refresh_ms` (default 1000) — split into `phase6_fulltext-async-writer`.
- [x] 3.4 Graceful shutdown flushes remaining buffer before exit — split into `phase6_fulltext-async-writer`.
- [x] 3.5 Tests for refresh cadence and shutdown correctness — split into `phase6_fulltext-async-writer`.

## 4. Commit Hook

- [x] 4.1 On transaction commit, enumerate affected node / relationship records — `Executor::fts_autopopulate_node` runs inside the CREATE operator pipeline; `Engine::persist_node_state` runs `fts_refresh_node` after SET / REMOVE / SET-label writes; `Engine::delete_node` runs `fts_evict_node`.
- [x] 4.2 For each registered FTS index whose label / type / property set matches, enqueue an add / del — CREATE match rule (label + string-valued indexed property); SET / REMOVE / DELETE use the registry's own `members: HashSet<u64>` tracking so drift between engine- and executor-side label indexes doesn't break the hook.
- [x] 4.3 `REMOVE n.p` / relationship delete emit `OP_FTS_DEL` — SET / REMOVE go through `fts_refresh_node` (del + conditional add); DELETE goes through `fts_evict_node` (del only). Both emit matching `FtsDel` WAL entries via `write_wal_async`.
- [x] 4.4 Integration test: `CREATE (n:Movie {title: "matrix"})` appears in `queryNodes` without a programmatic `add_node_document` call — `fulltext_create_node_auto_populates_matching_index`, `fulltext_create_node_skips_non_matching_label`.

## 5. Crash Recovery

- [x] 5.1 Recovery replays `OP_FTS_ADD` / `OP_FTS_DEL` against the rebuilt registry — `apply_wal_entry` handles every FTS op idempotently (`apply_wal_entry_creates_and_drops_index`, `apply_wal_entry_tolerates_missing_index`).
- [x] 5.2 Recovery creates / drops indexes from `OP_FTS_CREATE_INDEX` / `OP_FTS_DROP_INDEX` — covered by `apply_wal_entry_creates_and_drops_index`.
- [x] 5.3 Test: crash mid bulk-ingest, restart, assert all committed rows visible in FTS — split into `phase6_fulltext-async-writer`. Needs the sub-process kill-restart harness that lives naturally with the async-writer work (the sub-process test plumbing is shared with §3.1-§3.5).
- [x] 5.4 Test: crash between `createNodeIndex` and first ingest, restart, verify index exists with empty content — `fulltext_wal_replay_reconstructs_registry_and_content` simulates the full replay sequence (create → adds → del → non-FTS interleave) against a fresh registry and asserts only the surviving doc is queryable; `load_from_disk_rebuilds_registry_after_drop` covers the empty-content restore path.

## 6. Tail (mandatory)

- [x] 6.1 Update `docs/guides/FULL_TEXT_SEARCH.md` write-path section — new v1.12 write-path section documenting `CREATE` / `SET` / `REMOVE` / `DELETE` auto-maintenance + WAL integration subsection.
- [x] 6.2 Update `docs/specs/wal-mvcc.md` with FTS op-codes — 0x40-0x43 registered; `AddLabel`/`RemoveLabel` slots reassigned to suggested 0x50/0x51 in the spec comment.
- [x] 6.3 CHANGELOG entry — `[1.11.0]` (slice 1) + `[1.12.0]` (slices 2+3).
- [x] 6.4 Update or create documentation covering the implementation — FTS guide, wal-mvcc spec, CHANGELOG.
- [x] 6.5 Write tests covering the new behavior — 10 new tests across wal + registry + engine.
- [x] 6.6 Run tests and confirm they pass — full lib suite 2019 passed / 0 failed / 12 ignored.
- [x] 6.7 Run full workspace tests + fmt + clippy — fmt + `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.

## Follow-up tasks

- **phase6_fulltext-async-writer** — §3.1-§3.5 (per-index writer task + refresh_ms cadence + graceful shutdown) + §5.3 (crash-during-bulk-ingest integration test). Created ahead of archive per the rulebook follow-up-task protocol.
