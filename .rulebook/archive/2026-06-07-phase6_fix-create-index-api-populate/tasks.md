## 1. Implementation
- [x] 1.1 `execute_create_index` property branch registers the typed index (`property_index.create_index`) and backfills existing nodes (mirror `engine::populate_index`)
- [x] 1.2 Correct `IF NOT EXISTS` / `OR REPLACE` handling for property indexes via `property_index.has_index` (not the spatial R-tree registry); backfill indexes only String/Integer/Float/Boolean values (null never indexed)
- [x] 1.3 Route REST property `CREATE INDEX` / `DROP INDEX` through `engine.execute_cypher` (execute_index_commands) so the server populates the engine's typed index that reads + MERGE consult; preserve the existing single-column response shape; spatial/fulltext stay on the executor path

## 2. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 2.1 Update or create documentation covering the implementation (CHANGELOG Fixed / GH #9)
- [x] 2.2 Write tests: API/executor CREATE INDEX registers + populates (has_index true, find_exact finds existing nodes, plan emits NodeIndexSeek); IF NOT EXISTS / OR REPLACE behavior
- [x] 2.3 Run tests and confirm they pass
