# Proposal: phase6_fulltext-wal-integration

## Why

v1.8 (phase6_opencypher-fulltext-search) ships the Neo4j
`db.index.fulltext.*` DDL + query surface but ingest today requires
the programmatic `FullTextRegistry::add_node_document` API. The
write path is inline (mutex-per-index), metadata is reconstructed
from the filesystem, and there is no WAL record that replays the
FTS ops on crash recovery. This gap means CREATE / MERGE / SET do
not auto-populate the index, and a crash between Tantivy commit and
server shutdown can leave the FTS state behind the graph state.

## What Changes

1. Define WAL ops `OP_FTS_ADD`, `OP_FTS_DEL`, `OP_FTS_CREATE_INDEX`,
   `OP_FTS_DROP_INDEX` with encode / decode / replay paths.
2. Persist FTS metadata in LMDB alongside existing indexes so the
   catalogue round-trips through the same durable barrier as
   labels / types / keys.
3. Spawn a per-index single-writer task that consumes adds/deletes
   from a channel, drains on shutdown, and refreshes on a
   configurable `refresh_ms` cadence (default 1000).
4. Hook CREATE / MERGE / SET commit paths to enqueue the right add
   / del for every affected node whose labels and properties match
   a registered FTS index.
5. Replay WAL ops on engine startup to rebuild index state that was
   in-flight at crash time.
6. Tests: refresh cadence, shutdown correctness, crash-during-bulk-
   ingest.

## Impact

- Affected specs: `docs/specs/wal-mvcc.md`, `docs/specs/storage-format.md`, `docs/guides/FULL_TEXT_SEARCH.md`.
- Affected code: `crates/nexus-core/src/wal/*`, `crates/nexus-core/src/index/fulltext_registry.rs`, `crates/nexus-core/src/executor/operators/create.rs` / `set.rs` / `merge.rs`.
- Breaking change: NO (additive; existing programmatic ingest keeps working).
- User benefit: `CREATE (n:Movie {title: "..."})` automatically lands in a registered FTS index; crash recovery restores pending ingest; shutdown is graceful.
