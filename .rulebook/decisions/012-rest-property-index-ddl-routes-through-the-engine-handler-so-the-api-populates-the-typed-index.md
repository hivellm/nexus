# 12. REST property-index DDL routes through the engine handler so the API populates the typed index

**Status**: proposed
**Date**: 2026-06-07
**Related Tasks**: phase6_fix-create-index-api-populate, phase6_fix-read-match-index-seek

## Context

The REST/RPC server routes property CREATE INDEX through server.executor (Operator::CreateIndex -> executor execute_create_index), which only interned the catalog key — it never called property_index.create_index or backfilled. So has_index stayed false on the engine's typed property_index that the read NodeIndexSeek (#8) and index-backed MERGE existence consult. API-created indexes silently did nothing; reads fell back to O(N) label scans (UnindexedPropertyAccess) and MERGE existence stayed O(N). The engine handler execute_index_commands was already correct but the API path bypassed it. Confirmed empirically against the release server.

## Decision

Two-part fix. (1) Server: in crates/nexus-server/src/api/cypher/execute.rs, route property CREATE INDEX / DROP INDEX (Clause::CreateIndex with index_type None|property, and Clause::DropIndex) through engine.execute_cypher (-> execute_index_commands, which calls property_index.create_index + populate_index), mirroring how SHOW/CREATE CONSTRAINT and functions are already routed. Spatial and fulltext index DDL keep the executor path. The single-column ["index"] / "Label.prop.property" response shape is preserved by reformatting the engine result. (2) Core defense: executor execute_create_index (admin.rs) now also registers + backfills the typed property index when a property_index handle is present, with correct IF NOT EXISTS / OR REPLACE handling via has_index and null/array/object values excluded from backfill.

## Alternatives Considered

- Change the REST CREATE INDEX response to the engine's 2-column shape (rejected: needless client-facing contract change)
- Only fix executor execute_create_index without routing the server through the engine (rejected: insufficient — the server executor path did not make the populated index visible to the engine-backed read/MERGE consumers, so the notification persisted)
- Make server.executor share the engine's property_index handle directly (rejected: larger, riskier executor/engine wiring change; routing DDL through the engine is the smaller correct path already used for constraints)

## Consequences

API-created property indexes now engage NodeIndexSeek and index-backed MERGE existence; no UnindexedPropertyAccess after CREATE INDEX (verified end-to-end via REST). Response shape unchanged. nexus-server 437+ tests and nexus-core 2366 lib tests pass. Spatial/fulltext DDL behavior unchanged.
