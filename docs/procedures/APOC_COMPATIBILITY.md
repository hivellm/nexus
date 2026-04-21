# APOC Compatibility

**Version**: Nexus 1.6.0
**Status**: 100 APOC procedures shipped across 5 namespaces (`apoc.coll.*`,
`apoc.map.*`, `apoc.text.*`, `apoc.date.*`, `apoc.schema.*`). External
surfaces (`apoc.load.*`, `apoc.export.*`, `apoc.path.*`,
`apoc.periodic.*`) are tracked as follow-up tasks — they pull in
HTTP/filesystem sandboxing and depend on the `CALL ... IN TRANSACTIONS`
subquery task.

## Shipped procedures

### `apoc.coll.*` (30)

| Procedure | Signature | Parity |
|-----------|-----------|--------|
| `apoc.coll.union` | `(list, list) -> list` | exact |
| `apoc.coll.intersection` | `(list, list) -> list` | exact |
| `apoc.coll.disjunction` | `(list, list) -> list` | exact |
| `apoc.coll.subtract` | `(list, list) -> list` | exact |
| `apoc.coll.sort` | `(list) -> list` | exact (mixed-type ordinal rule) |
| `apoc.coll.sortNodes` | `(list) -> list` | exact — entity id tiebreak pending |
| `apoc.coll.sortMaps` | `(list<map>, key) -> list` | exact |
| `apoc.coll.shuffle` | `(list) -> list` | exact |
| `apoc.coll.reverse` | `(list) -> list` | exact |
| `apoc.coll.zip` | `(list, list) -> list` | exact |
| `apoc.coll.pairs` | `(list) -> list` | exact (trailing `[last, null]`) |
| `apoc.coll.pairsMin` | `(list) -> list` | exact |
| `apoc.coll.combinations` | `(list, min, max) -> list` | exact (contiguous sub-lists) |
| `apoc.coll.partitions` | `(list, size) -> list` | exact |
| `apoc.coll.flatten` | `(list, deep=false) -> list` | exact |
| `apoc.coll.frequencies` | `(list) -> list<{item, count}>` | exact (count-desc) |
| `apoc.coll.frequenciesAsMap` | `(list) -> map` | exact |
| `apoc.coll.duplicates` | `(list) -> list` | exact |
| `apoc.coll.toSet` | `(list) -> list` | exact |
| `apoc.coll.indexOf` | `(list, value) -> int` | exact (-1 for missing) |
| `apoc.coll.contains` | `(list, value) -> bool` | exact |
| `apoc.coll.containsAll` | `(list, list) -> bool` | exact |
| `apoc.coll.max` / `min` | `(list) -> any` | exact (mixed-type ordinal rule) |
| `apoc.coll.sum` | `(list<number>) -> number` | exact |
| `apoc.coll.avg` | `(list<number>) -> float` | exact |
| `apoc.coll.stdev` | `(list<number>) -> float` | exact (sample stdev) |
| `apoc.coll.remove` | `(list, index, count=1) -> list` | exact |
| `apoc.coll.fill` | `(value, count) -> list` | exact |
| `apoc.coll.runningTotal` | `(list<number>) -> list<number>` | exact |

### `apoc.map.*` (20)

| Procedure | Signature | Parity |
|-----------|-----------|--------|
| `apoc.map.merge` | `(map, map) -> map` (right-wins) | exact |
| `apoc.map.mergeList` | `(list<map>) -> map` | exact |
| `apoc.map.fromPairs` / `fromEntries` | `(list<[k, v]>) -> map` | exact |
| `apoc.map.fromLists` | `(keys, values) -> map` | exact |
| `apoc.map.fromValues` | `([k1, v1, ...]) -> map` | exact |
| `apoc.map.setKey` | `(map, key, value) -> map` | exact |
| `apoc.map.removeKey` | `(map, key) -> map` | exact |
| `apoc.map.removeKeys` | `(map, list<key>) -> map` | exact |
| `apoc.map.clean` | `(map, removeKeys, removeValues) -> map` | exact |
| `apoc.map.flatten` | `(map, delim='.') -> map` | exact |
| `apoc.map.unflatten` | `(map, delim='.') -> map` | exact |
| `apoc.map.values` | `(map, keys?) -> list` | exact |
| `apoc.map.fromNodes` | — | engine-context required; dispatcher override pending |
| `apoc.map.groupBy` | `(list<map>, key) -> map` | exact (last-wins) |
| `apoc.map.groupByMulti` | `(list<map>, key) -> map<key, list>` | exact |
| `apoc.map.updateTree` | `(tree, pathKey, updates) -> map` | exact |
| `apoc.map.submap` | `(map, keys) -> map` | exact |
| `apoc.map.get` / `getOrDefault` | `(map, key, default=null) -> any` | exact |

### `apoc.text.*` (20)

| Procedure | Parity | Notes |
|-----------|--------|-------|
| `apoc.text.levenshteinDistance` / `Similarity` | exact | `strsim` crate |
| `apoc.text.jaroWinklerDistance` | exact | — |
| `apoc.text.sorensenDiceSimilarity` | exact | — |
| `apoc.text.hammingDistance` | exact | — |
| `apoc.text.regexGroups` | exact | Rust `regex` crate (RE2-style) |
| `apoc.text.replace` | exact | — |
| `apoc.text.split` | exact | — |
| `apoc.text.phonetic` | exact | American Soundex (4-char) |
| `apoc.text.doubleMetaphone` | Philips Metaphone | single-code variant of APOC double-metaphone |
| `apoc.text.clean` | exact | lowercase + non-alnum strip |
| `apoc.text.lpad` / `rpad` | exact | — |
| `apoc.text.format` | `{0}` / `{name}` slot substitution | — |
| `apoc.text.base64Encode` / `Decode` | exact | `base64` crate |
| `apoc.text.camelCase` | exact | — |
| `apoc.text.capitalize` | exact | — |
| `apoc.text.hexValue` | exact | uppercase hex of u64 |
| `apoc.text.byteCount` | exact | UTF-8 byte length |

### `apoc.date.*` (25)

| Procedure | Parity |
|-----------|--------|
| `apoc.date.format` / `parse` / `convertFormat` | exact — Java `yyyy-MM-dd HH:mm:ss` tokens translated to chrono |
| `apoc.date.currentMillis` / `currentTimestamp` | exact (UTC) |
| `apoc.date.systemTimezone` | returns `UTC` (Nexus is UTC-internal) |
| `apoc.date.toYears` / `toMonths` / `toDays` / `toHours` / `toMinutes` / `toSeconds` | exact |
| `apoc.date.add` / `subtract` | exact (ms/s/m/h/d units) |
| `apoc.date.fromISO` / `toISO` | exact (RFC3339) |
| `apoc.date.yearQuarter` / `week` / `weekday` / `dayOfYear` | exact (Monday=1 weekday) |
| `apoc.date.startOfDay` / `endOfDay` | exact (inclusive 23:59:59.999) |
| `apoc.date.diff` / `between` | exact |
| `apoc.date.parseAsZonedDateTime` | falls back to `parse` (UTC zone) |

### `apoc.schema.*` (10)

| Procedure | Parity | Notes |
|-----------|--------|-------|
| `apoc.schema.assert(indexes, constraints)` | shape-compatible | returns (label, key, keys, unique, action) rows; engine applies the DDL |
| `apoc.schema.nodes` / `relationships` | empty-by-default shape | engine-context override pending |
| `apoc.schema.properties.distinctCount` | empty-by-default shape | — |
| `apoc.schema.node.constraintExists` / `indexExists` | `false` by default | engine-context override pending |
| `apoc.schema.relationship.constraintExists` / `indexExists` | `false` by default | engine-context override pending |
| `apoc.schema.stats` | skeleton map | zero-valued counters |
| `apoc.schema.info` | exact | reports the APOC registry size |

## Not yet shipped

- `apoc.load.*` — HTTP + JSON/CSV + file loading; gated on HTTP
  sandboxing + filesystem allow-list.
- `apoc.export.*` — JSON/CSV/Cypher dump; gated on filesystem
  allow-list.
- `apoc.path.*` — advanced path finding; follow-up task.
- `apoc.periodic.*` — depends on `CALL ... IN TRANSACTIONS` which
  ships in a separate phase6 task.
- `apoc.cypher.*`, `apoc.ml.*`, `apoc.jdbc.*`, `apoc.trigger.*`,
  `apoc.bolt.*` — out of scope for v1 per the ecosystem proposal's
  §"Out of scope" note.

## CI gate

`cargo +nightly test -p nexus-core --lib apoc::` runs the full
82-test suite per commit; the count is pinned in
`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md` so coverage
regressions surface as a test-count mismatch.
