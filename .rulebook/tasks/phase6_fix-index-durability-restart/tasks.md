## 1. Investigation
- [ ] 1.1 Reproduce: create + use a property index (seek, no notification), restart the engine, confirm `has_index` is false and `MATCH (n:L {p:v})` falls back to a full scan (UnindexedPropertyAccess); confirm `CREATE INDEX` (no IF NOT EXISTS) wrongly succeeds post-restart
- [ ] 1.2 Confirm `rebuild_indexes_from_storage` rebuilds only the label index and that CREATE INDEX definitions are not persisted; decide where to persist index definitions (LMDB catalog vs WAL)

## 2. Implementation — persistence
- [ ] 2.1 Persist property-index definitions (the `(label, property)` pairs) durably on `CREATE INDEX`; remove them on `DROP INDEX`
- [ ] 2.2 Restore catalog-level index existence so `CREATE INDEX` without IF NOT EXISTS errors "already exists" after a restart

## 3. Implementation — startup rebuild
- [ ] 3.1 On startup recovery, reload persisted index definitions and rebuild the typed `property_index` by backfilling from storage (mirror `populate_index` per definition)
- [ ] 3.2 Ensure the rebuild path covers the typed property index (extend `rebuild_indexes_from_storage` or add a sibling); verify composite B-tree / spatial / full-text restart durability or note follow-ups

## 4. Tail (mandatory — enforced by rulebook v5.3.0)
- [ ] 4.1 Update or create documentation covering index durability + recovery (CHANGELOG Fixed / GH #11)
- [ ] 4.2 Write tests: index survives an engine restart (reopen) — `has_index` true, seek engages (no UnindexedPropertyAccess), duplicate `CREATE INDEX` errors; backfill correctness after reopen
- [ ] 4.3 Run tests and confirm they pass
