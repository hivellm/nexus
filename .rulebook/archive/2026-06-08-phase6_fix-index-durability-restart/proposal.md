# Proposal: phase6_fix-index-durability-restart

Source: GitHub issue #11 (https://github.com/hivellm/nexus/issues/11)

## Why
Property indexes created via `CREATE INDEX FOR (n:Label) ON (n.prop)` are
lost on server restart — they are not reloaded into the typed
`property_index` (nor, it appears, the catalog) on startup. After a
restart, `NodeIndexSeek` (#8) and index-backed MERGE existence (#9)
silently fall back to O(N) label scans until every index is re-created.

Critical: any restart (deploy, crash recovery, OOM) silently degrades a
previously-fast graph to O(N), so the next write burst melts the server
again — exactly the meltdown #8/#9 fixed.

Reproduction (2.3.1):
```
MATCH (n:ToolCall {id:"x"}) RETURN n.id      # notifications: [] (seek, fast)
docker restart nexus
MATCH (n:ToolCall {id:"x"}) RETURN n.id
   -> notifications: ["Nexus.Performance.UnindexedPropertyAccess"], 570 ms (full scan)
CREATE INDEX FOR (n:ToolCall) ON (n.id)      # SUCCEEDS (no "already exists")
   -> i.e. the catalog index entry is gone too; re-running restores the seek
```

Confirmed root cause area: `rebuild_indexes_from_storage` (engine startup)
rebuilds only the label index, not the typed `property_index`, and the
`CREATE INDEX` definitions are not persisted, so `has_index` is false for
every previously-created index after a restart.

## What Changes
- Persist property-index definitions durably (the `(label, property)`
  pairs registered by `CREATE INDEX`) — e.g. an index catalog/registry in
  LMDB or the WAL — so they survive a restart. Define where definitions
  live and how DROP INDEX removes them.
- On startup recovery, reload the persisted index definitions and rebuild
  the typed `property_index` (the structure `has_index` / `find_exact`
  consult) by backfilling from storage — mirror `populate_index` for every
  persisted definition. Extend `rebuild_indexes_from_storage` (or add a
  sibling) to cover the typed property index, composite B-tree, and any
  other typed index that is currently label-only.
- Restore catalog-level index existence so `CREATE INDEX` (no
  `IF NOT EXISTS`) correctly errors "already exists" after a restart.

## Impact
- Affected specs: storage / catalog (index persistence), ops / recovery,
  cypher-subset / index DDL
- Affected code: `crates/nexus-core/src/engine/mod.rs`
  (`rebuild_indexes_from_storage`, `execute_index_commands` /
  `populate_index`), `crates/nexus-core/src/catalog/` or `wal/` for the
  persisted index definitions, `index/` registry
- Breaking change: NO (durability fix; on-disk format gains an index
  catalog — must be backward-readable for existing data dirs)
- User benefit: indexes survive restarts; `NodeIndexSeek` (#8) and
  index-backed MERGE (#9) stay O(log N) across deploys/crashes without a
  client re-issuing `CREATE INDEX`; removes the silent post-restart
  meltdown.

## Notes
- Directly protects the #8 and #9 fixes (and the index-backed MERGE) from
  silent post-restart regression. Companion to #12 (the post-restart O(N)
  scan is one path into the sustained-write meltdown).
- Also confirm spatial / full-text / composite indexes' restart durability
  while here; at minimum cover the typed property index this issue names.
