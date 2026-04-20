# Proposal: APOC Ecosystem (200+ Procedures across 8 Namespaces)

## Why

APOC ("Awesome Procedures on Cypher") is Neo4j's unofficial standard
library. It is so ubiquitous that every substantial real-world Cypher
workload we've seen uses it. It provides essential utilities that
openCypher never standardised:

- `apoc.load.*` — HTTP, JSON, XML, CSV, JDBC, Parquet loading.
- `apoc.periodic.*` — `iterate`, `commit`, `submit` for bulk work.
- `apoc.coll.*` — set/list operations (`union`, `intersection`,
  `sort`, `shuffle`, `pairs`, `zip`, `flatten`).
- `apoc.map.*` — map merge, projection, key renaming, value
  coalescing.
- `apoc.date.*` — timezone-aware formatting, parsing, bucketing.
- `apoc.text.*` — fuzzy string similarity (Levenshtein, Jaro),
  regex, PhoneticS metrics.
- `apoc.path.*` — advanced path-finding (expand, subgraph, spanning
  tree) that goes beyond shortestPath.
- `apoc.schema.*` — schema introspection beyond `db.schema.*`.
- `apoc.export.*` — dump to JSON/CSV/Cypher scripts.
- `apoc.trigger.*` — write triggers (v2; optional).
- `apoc.graph.*` — subgraph creation, generation of sample graphs.

Nexus currently implements **zero** of these. The impact of that gap:

- Neo4j migration guides all assume APOC. Every such guide breaks.
- Third-party ETL tools (Hackolade, Airbyte Neo4j adapter) hard-code
  APOC calls.
- StackOverflow's top Neo4j answers reference APOC. Users copy-paste
  and hit "unknown procedure".
- Without `apoc.periodic.iterate` the `CALL IN TRANSACTIONS` surface
  (shipped in the subquery task) is less ergonomic for ETL workflows
  already written for APOC.

This task ships an APOC-compatible subset — not 100% of APOC, but the
~200 procedures that cover 95% of real usage per the APOC usage
telemetry Neo4j has published.

## What Changes

- **New crate** `nexus-apoc` in the workspace. It depends only on
  `nexus-core` through its public procedure API; no private coupling.
- **Procedure registry** plumbs `apoc.*` dispatch through the
  existing system-procedures catalogue, so `dbms.procedures()`
  enumerates them alongside `db.*` and `gds.*`.
- **Namespaces shipped in v1 of this task**:
  - `apoc.coll.*` — 30 procedures (~1 week)
  - `apoc.map.*` — 20 procedures (~3 days)
  - `apoc.date.*` — 25 procedures (~1 week)
  - `apoc.text.*` — 20 procedures (~1 week)
  - `apoc.path.*` — 25 procedures (~2 weeks)
  - `apoc.periodic.*` — 5 procedures (~3 days, rides on
    `CALL IN TRANSACTIONS`)
  - `apoc.load.*` — 8 procedures (~1 week, HTTP + JSON + CSV; JDBC
    deferred to a later release as its own task)
  - `apoc.schema.*` — 10 procedures (~3 days)
  - `apoc.export.*` — 10 procedures (~1 week)
- **External dependencies** (vetted, Apache-2.0 or MIT):
  - `chrono-tz` for timezone-aware date APIs.
  - `reqwest` for HTTP loading (blocking variant inside a thread pool).
  - `jsonpath-rust` for `apoc.load.jsonPath`.
  - `strsim` for similarity metrics.
  - `serde_json` (already present).
- **Sandboxing**: `apoc.load.*` is controlled by the config key
  `apoc.import.file.enabled` (default `false` for safety, matching
  Neo4j's default). HTTP loading obeys an allow-list of host/URL
  prefixes.

**BREAKING**: none. All additions live under the `apoc.*` namespace
that is currently empty.

## Impact

### Affected Specs

- NEW capability: `apoc-coll`
- NEW capability: `apoc-map`
- NEW capability: `apoc-date`
- NEW capability: `apoc-text`
- NEW capability: `apoc-path`
- NEW capability: `apoc-periodic`
- NEW capability: `apoc-load`
- NEW capability: `apoc-schema`
- NEW capability: `apoc-export`
- MODIFIED capability: `procedures-registry` (apoc namespace)

### Affected Code

- `nexus-apoc/Cargo.toml` (NEW crate)
- `nexus-apoc/src/coll/` (~1200 lines across 30 procedures)
- `nexus-apoc/src/map/` (~600 lines)
- `nexus-apoc/src/date/` (~900 lines)
- `nexus-apoc/src/text/` (~700 lines)
- `nexus-apoc/src/path/` (~1400 lines)
- `nexus-apoc/src/periodic/` (~400 lines)
- `nexus-apoc/src/load/` (~800 lines)
- `nexus-apoc/src/schema/` (~400 lines)
- `nexus-apoc/src/export/` (~700 lines)
- `nexus-apoc/tests/` (~3500 lines)

### Dependencies

- Requires: `phase6_opencypher-system-procedures` (procedure registry
  extension).
- Requires: `phase6_opencypher-quickwins` (dynamic property access —
  APOC code uses `n[$k]` heavily).
- Requires: `phase6_opencypher-subquery-transactions`
  (`apoc.periodic.iterate` wraps `CALL IN TRANSACTIONS`).
- Requires: `phase6_opencypher-geospatial-predicates` (some APOC
  path procedures touch spatial).

### Timeline

- **Duration**: 8–12 weeks (split into three mini-milestones)
- **Complexity**: High due to surface area; individual procedures are
  usually straightforward.
- **Risk**: Medium — large surface means many test scenarios; APOC
  has subtle behavioural details that must match Neo4j's for
  compatibility.
