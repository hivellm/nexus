# Implementation Tasks — FTS WAL Integration

## 1. WAL Ops

- [ ] 1.1 Define `OP_FTS_ADD` / `OP_FTS_DEL` / `OP_FTS_CREATE_INDEX` / `OP_FTS_DROP_INDEX` in the WAL op-code enum
- [ ] 1.2 Encode / decode round-trip tests for each op
- [ ] 1.3 Replay dispatcher in `wal::recover` that re-executes each op against the registry

## 2. LMDB Metadata Persistence

- [ ] 2.1 Persist `FullTextIndexMeta` rows in LMDB keyed by name
- [ ] 2.2 Load persisted metadata on `IndexManager::new` and rebuild the `FullTextRegistry`
- [ ] 2.3 CRUD tests for catalogue persistence

## 3. Per-index Writer Task

- [ ] 3.1 Per-index single-writer task with bounded channel
- [ ] 3.2 Configurable `refresh_ms` (default 1000) drives reader reload cadence
- [ ] 3.3 Graceful shutdown drains the channel before exit
- [ ] 3.4 Tests for refresh cadence + shutdown drain

## 4. Commit Hook

- [ ] 4.1 On transaction commit, enumerate affected node / relationship records
- [ ] 4.2 For each registered FTS index whose label / type / property set matches, enqueue an add / del
- [ ] 4.3 `REMOVE n.p` / relationship delete emit `OP_FTS_DEL`
- [ ] 4.4 Integration test: `CREATE (n:Movie {title: "matrix"})` appears in `queryNodes` without a programmatic `add_node_document` call

## 5. Crash Recovery

- [ ] 5.1 Recovery replays `OP_FTS_ADD` / `OP_FTS_DEL` against the rebuilt registry
- [ ] 5.2 Recovery creates / drops indexes from `OP_FTS_CREATE_INDEX` / `OP_FTS_DROP_INDEX`
- [ ] 5.3 Test: crash mid bulk-ingest, restart, assert all committed rows visible in FTS
- [ ] 5.4 Test: crash between `createNodeIndex` and first ingest, restart, verify index exists with empty content

## 6. Tail (mandatory)

- [ ] 6.1 Update `docs/guides/FULL_TEXT_SEARCH.md` write-path section
- [ ] 6.2 Update `docs/specs/wal-mvcc.md` with FTS op-codes
- [ ] 6.3 CHANGELOG entry
- [ ] 6.4 Run full workspace tests + fmt + clippy
