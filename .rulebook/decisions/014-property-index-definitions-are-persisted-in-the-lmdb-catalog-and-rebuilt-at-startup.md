# 14. Property-index definitions are persisted in the LMDB catalog and rebuilt at startup

**Status**: proposed
**Date**: 2026-06-08
**Related Tasks**: phase6_fix-index-durability-restart, phase6_fix-read-match-index-seek, phase6_fix-create-index-api-populate

## Context

Property indexes created via CREATE INDEX were lost on restart: the (label_id,key_id) definitions were never persisted, and rebuild_indexes_from_storage rebuilt only the label/relationship indexes. After any restart has_index was false, so NodeIndexSeek (#8) and index-backed MERGE existence (#9) silently fell back to O(N) label scans (UnindexedPropertyAccess) until a client re-issued CREATE INDEX; the catalog index entry was also gone, so a duplicate CREATE INDEX wrongly succeeded. A restart (deploy/crash/OOM) re-introduced the very meltdown #8/#9 fixed (issue #11).

## Decision

Persist property-index definitions durably and rebuild them at startup. (1) Add a `property_index_db: Database<SerdeBincode<(u32,u32)>, SerdeBincode<()>>` LMDB sub-database to the Catalog (mirrors udf_db/procedure_db) with persist_property_index / remove_property_index / list_property_indexes. (2) Every CREATE INDEX (engine execute_index_commands property branch AND executor admin.rs execute_create_index property branch) calls persist_property_index after create_index+populate; DROP INDEX calls remove_property_index. (3) rebuild_indexes_from_storage (run at every engine open) iterates list_property_indexes() and rebuilds the typed property_index via create_index + populate_index (backfill from storage). LMDB is crash-safe, so the definitions survive crashes; has_index existence is restored so duplicate CREATE INDEX errors correctly.

## Alternatives Considered

- Persist index defs in the WAL and replay on recovery (rejected: the LMDB catalog already durably stores schema like constraints/udfs/procedures; reuse that crash-safe store)
- Rebuild the property index by scanning all node properties heuristically (rejected: cannot know which (label,key) pairs were intended as indexes without the explicit definition)
- Require clients to re-run CREATE INDEX IF NOT EXISTS after every restart (rejected: the reported operational gap — a Nexus-only restart leaves the graph un-indexed and meltdown-prone)

## Consequences

Property indexes survive restarts; NodeIndexSeek + index-backed MERGE stay O(log N) across deploys/crashes without a client re-issuing CREATE INDEX. Verified by a unit test (reopen engine on same data dir) and end-to-end via a REST server restart (no UnindexedPropertyAccess after restart; duplicate CREATE errors). nexus-core lib 2369 passed. Scope is the typed property index named by the issue; composite/spatial/full-text restart durability is separate. On-disk format gains an LMDB sub-db (backward-readable: absent for old data dirs => empty list => no rebuild, same as before).
