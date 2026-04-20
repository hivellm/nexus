# Implementation Tasks — APOC Ecosystem

## 1. Crate Scaffolding

- [ ] 1.1 Create `nexus-apoc` crate in workspace
- [ ] 1.2 Wire to `nexus-core` via the public procedure API
- [ ] 1.3 Registry entry so `apoc.*` dispatches to this crate
- [ ] 1.4 `dbms.procedures()` enumerates the apoc catalogue
- [ ] 1.5 Basic CI smoke test

## 2. apoc.coll.* (30 procedures)

- [ ] 2.1 `union`, `intersection`, `disjunction`, `subtract`
- [ ] 2.2 `sort`, `sortNodes`, `sortMaps`, `shuffle`, `reverse`
- [ ] 2.3 `zip`, `pairs`, `pairsMin`, `combinations`, `partitions`
- [ ] 2.4 `flatten`, `frequencies`, `frequenciesAsMap`, `duplicates`
- [ ] 2.5 `toSet`, `indexOf`, `contains`, `containsAll`
- [ ] 2.6 `max`, `min`, `sum`, `avg`, `stdev`, `remove`, `fill`, `runningTotal`
- [ ] 2.7 Comprehensive tests per procedure

## 3. apoc.map.* (20 procedures)

- [ ] 3.1 `merge`, `mergeList`, `fromPairs`, `fromLists`, `fromValues`
- [ ] 3.2 `setKey`, `removeKey`, `removeKeys`, `clean`
- [ ] 3.3 `flatten`, `unflatten`, `values`, `fromNodes`
- [ ] 3.4 `groupBy`, `groupByMulti`
- [ ] 3.5 `updateTree`, `submap`
- [ ] 3.6 Tests

## 4. apoc.date.* (25 procedures)

- [ ] 4.1 `apoc.date.format`, `apoc.date.parse`, `apoc.date.convertFormat`
- [ ] 4.2 Timezone-aware formatters via `chrono-tz`
- [ ] 4.3 `apoc.date.systemTimezone`, `currentTimestamp`
- [ ] 4.4 Bucketing: `apoc.date.toYears`, `toDays`, `toHours`, `toMinutes`, `toSeconds`
- [ ] 4.5 Arithmetic: `apoc.date.add`, `apoc.date.subtract`
- [ ] 4.6 Tests across multiple zones

## 5. apoc.text.* (20 procedures)

- [ ] 5.1 Levenshtein, JaroWinkler, Sorensen-Dice via `strsim`
- [ ] 5.2 `apoc.text.regexGroups`, `apoc.text.replace`, `apoc.text.split`
- [ ] 5.3 `apoc.text.phonetic` (Soundex), `doubleMetaphone`
- [ ] 5.4 `apoc.text.clean`, `lpad`, `rpad`, `format`, `base64Encode/Decode`
- [ ] 5.5 `camelCase`, `capitalize`, `hexValue`, `byteCount`
- [ ] 5.6 Tests

## 6. apoc.path.* (25 procedures)

- [ ] 6.1 `apoc.path.expand`, `expandConfig`, `expandTree`
- [ ] 6.2 `apoc.path.subgraphNodes`, `subgraphAll`
- [ ] 6.3 `apoc.path.spanningTree`
- [ ] 6.4 Relationship filters, label filters, depth limits
- [ ] 6.5 `uniqueness: NODE_GLOBAL | NODE_PATH | RELATIONSHIP_GLOBAL` semantics
- [ ] 6.6 Tests including cycle cases

## 7. apoc.periodic.* (5 procedures)

- [ ] 7.1 `apoc.periodic.iterate(cypher, action, config)` over `CALL IN TRANSACTIONS`
- [ ] 7.2 `apoc.periodic.commit` (repeat until no rows)
- [ ] 7.3 `apoc.periodic.submit` (fire-and-forget background)
- [ ] 7.4 `apoc.periodic.list`, `apoc.periodic.cancel`
- [ ] 7.5 Tests including error modes

## 8. apoc.load.* (8 procedures)

- [ ] 8.1 `apoc.load.json(url)` over HTTP
- [ ] 8.2 `apoc.load.jsonParams(url, headers, payload)`
- [ ] 8.3 `apoc.load.jsonPath(url, path)`
- [ ] 8.4 `apoc.load.csv(url)`, `csvParams`
- [ ] 8.5 `apoc.load.xml`
- [ ] 8.6 HTTP allow-list: config `apoc.http.enabled`, `apoc.http.allow`
- [ ] 8.7 File loading behind `apoc.import.file.enabled` (default false)
- [ ] 8.8 Tests with local mock HTTP server

## 9. apoc.schema.* (10 procedures)

- [ ] 9.1 `apoc.schema.assert(indexes, constraints)` idempotent DDL
- [ ] 9.2 `apoc.schema.nodes()`, `apoc.schema.relationships()`
- [ ] 9.3 `apoc.schema.properties.distinctCount`
- [ ] 9.4 Tests

## 10. apoc.export.* (10 procedures)

- [ ] 10.1 `apoc.export.json.all(file, config)`
- [ ] 10.2 `apoc.export.csv.all(file, config)`
- [ ] 10.3 `apoc.export.cypher.all(file, config)` dumps to Cypher script
- [ ] 10.4 Query-scoped exports (`export.json.query`, etc.)
- [ ] 10.5 Streaming — no full in-memory materialisation
- [ ] 10.6 Tests round-tripping against importer procedures

## 11. Sandboxing & Safety

- [ ] 11.1 `apoc.import.file.enabled = false` by default
- [ ] 11.2 `apoc.http.allow` regex allow-list
- [ ] 11.3 `apoc.http.timeout_ms` request timeout
- [ ] 11.4 Export paths restricted to `data_dir/exports/`
- [ ] 11.5 Security tests: rejected paths + disallowed hosts

## 12. Migration Parity Harness

- [ ] 12.1 Run the top-100 StackOverflow APOC queries from Neo4j docs against Nexus
- [ ] 12.2 Compare output row-for-row with Neo4j 5.x
- [ ] 12.3 Track parity ≥ 95% in `docs/compatibility/APOC_COMPATIBILITY.md`
- [ ] 12.4 CI gate on regression

## 13. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 13.1 Write `docs/procedures/APOC_COMPATIBILITY.md` (full surface)
- [ ] 13.2 Update `docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`
- [ ] 13.3 Add CHANGELOG entry "Added APOC procedure ecosystem (~200 procedures)"
- [ ] 13.4 Update or create documentation covering the implementation
- [ ] 13.5 Write tests covering the new behavior
- [ ] 13.6 Run tests and confirm they pass
- [ ] 13.7 Quality pipeline: fmt + clippy + ≥95% coverage
