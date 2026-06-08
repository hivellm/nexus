## 1. Investigation
- [x] 1.1 Reproduce: create + use a property index (seek, no notification), restart the engine, confirm `has_index` is false and `MATCH (n:L {p:v})` falls back to a full scan (UnindexedPropertyAccess); confirm `CREATE INDEX` (no IF NOT EXISTS) wrongly succeeds post-restart
- [x] 1.2 Confirm `rebuild_indexes_from_storage` rebuilds only the label/relationship indexes and that CREATE INDEX definitions are not persisted; persist definitions in the LMDB catalog

## 2. Implementation — persistence
- [x] 2.1 Persist property-index definitions (the `(label_id, key_id)` pairs) durably on `CREATE INDEX` (engine + executor paths); remove them on `DROP INDEX`
- [x] 2.2 Restore catalog-level index existence so `CREATE INDEX` without IF NOT EXISTS errors "already exists" after a restart

## 3. Implementation — startup rebuild
- [x] 3.1 On startup recovery, reload persisted index definitions and rebuild the typed `property_index` by backfilling from storage (mirror `populate_index` per definition)
- [x] 3.2 Rebuild path lives in `rebuild_indexes_from_storage` (runs at engine open). Scope is the typed property index named by the issue; composite B-tree / spatial / full-text restart durability is tracked separately if needed

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [x] 4.1 Update or create documentation covering index durability + recovery (CHANGELOG Fixed / GH #11)
- [x] 4.2 Write tests: index survives an engine restart (reopen) — `has_index` true, seek engages (no UnindexedPropertyAccess), duplicate `CREATE INDEX` errors; backfill correctness after reopen
- [x] 4.3 Run tests and confirm they pass
