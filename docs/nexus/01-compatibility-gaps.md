# openCypher / Neo4j 5.x Compatibility Gap Analysis

> **Date**: 2026-07-11 · **Analyzed version**: 2.4.0 · **Method**: code-verified
> inventory (parser `Clause`/`Expression` enums, executor function dispatch,
> compat suite sections) cross-checked against openCypher 9 + Neo4j 5.x.
>
> Part of the [Nexus 2.5.0 competitive analysis](README.md).

## Verdict

Nexus is at roughly **~85% feature parity** with openCypher 9 / Neo4j 5.x for
single-database, read-heavy + moderate-write workloads — substantially higher
than the "~55%" figure still quoted in older docs. Gaps concentrate in:
protocol surface (no Bolt), vendor libraries (no APOC), concurrent bulk
ingestion (`CALL {} IN TRANSACTIONS` concurrency), and a handful of missing
scalar functions. **The dominant compatibility problem is not missing
features — it is transport-dependent correctness** (see
[02-bug-inventory.md](02-bug-inventory.md) and
[04-write-path-unification.md](04-write-path-unification.md)): the same query
can return different results over HTTP vs RPC vs GraphQL.

Estimated parity by area:

| Area | Parity | Notes |
|---|---|---|
| Core Cypher (read) | ~90% | All major clauses, patterns, aggregations |
| Write operations | ~95% engine / **much lower over HTTP** | write-path fork bugs |
| Functions | ~85% | 75+ implemented; APOC absent |
| Indexes | ~95% | bitmap, B-tree (composite), spatial, full-text, KNN |
| Types | ~95% | temporal, spatial, bytes; NaN/Inf JSON edge |
| Transactions | ~90% | MVCC, savepoints; single-writer |
| Protocols | ~30% | REST + MessagePack RPC + RESP3; **no Bolt** |
| Procedures/GDS | ~75% | 30+ procedures; no APOC |

## Supported (works — code-verified)

**Clauses**: MATCH, OPTIONAL MATCH (incl. standalone null semantics), WHERE
(after MATCH/OPTIONAL MATCH/WITH), RETURN (DISTINCT), ORDER BY, SKIP/LIMIT,
CREATE (nodes, rels, multi-label), MERGE (ON CREATE/ON MATCH), SET (props,
labels, `+=`, dynamic label params), DELETE / DETACH DELETE, REMOVE, WITH,
UNWIND, FOREACH, UNION / UNION ALL, CALL {} subqueries (scoped form), CALL
procedure, LOAD CSV (WITH HEADERS, FIELDTERMINATOR), EXPLAIN/PROFILE,
BEGIN/COMMIT/ROLLBACK + SAVEPOINT suite, CREATE/DROP INDEX (incl. composite +
spatial), CREATE/DROP CONSTRAINT (UNIQUE, EXISTS, NODE KEY), multi-database
DDL (SHOW/CREATE/DROP/USE DATABASE), SHOW FUNCTIONS/QUERIES/CONSTRAINTS/USERS,
TERMINATE QUERY.

**Patterns**: multi-label nodes, directed/undirected rels, variable-length
`[*1..3]`/`[*]`, named paths `p=()`, comma-separated patterns, inline property
filters, shortestPath/allShortestPaths, QPP anonymous-body form (collapses to
`*m..n`), path modes (WALK/TRAIL/ACYCLIC/SIMPLE — phase8 in progress).

**Expressions**: full operator set (arithmetic, comparison, logical, IN,
IS NULL, STARTS WITH/ENDS WITH/CONTAINS/`=~`), CASE (simple + generic), list
comprehensions, pattern comprehensions, map projections, EXISTS {} / COLLECT {}
subqueries, list index/slice, parameters everywhere.

**Functions (75+)**: string (21 incl. regex family + bytes codecs), math (22),
list/predicate (20+ incl. all/any/none/single, reduce, coalesce, type
predicates + conversions), temporal (20+ incl. duration.between and temporal
arithmetic), spatial (point/distance/withinBBox/withinDistance + accessors),
graph/path (labels, type, keys, id, elementId, properties, nodes,
relationships, length), aggregation (count, sum, avg, min, max, collect
[DISTINCT], percentileDisc/Cont, stDev/stDevP).

**Procedures (30+)**: GDS suite (pageRank, betweenness, closeness, degree,
eigenvector, louvain, labelPropagation, dijkstra, astar, bellmanFord, Yen's K,
WCC/SCC, triangle count, clustering coefficients, jaccard/cosine),
db.index.fulltext.* (Tantivy BM25), db.indexes/schema/constraints, spatial.*.

**Other**: external IDs (`_id` + ON CONFLICT), query hints (USING
INDEX/SCAN/JOIN), LRU plan cache (xxh3 canonicalization), GRAPH[name] scoping.

## Partial (works with caveats)

| Feature | Caveat | Effort | Priority |
|---|---|---|---|
| `CALL {} IN TRANSACTIONS OF N ROWS` | Parses; concurrency rejected (`ERR_CALL_IN_TX_CONCURRENCY_UNSUPPORTED`) until V2 sharding | L | P0 |
| QPP named/labelled bodies | Route to QuantifiedExpand but planner surfaces `ERR_QPP_NOT_IMPLEMENTED` (slice 2 pending) | M | P1 |
| `elementId()` | Returns internal 64-bit ID, not Neo4j 5 opaque stable string | S | P1 |
| Full-text index freshness | No WAL auto-populate on CREATE/MERGE/SET; manual refresh needed | S | P1 |
| percentile/stDev family | Declared; implementation status needs verification | S | P1 |
| Composite index costing | Seek works; cost model may not reflect multi-column benefit | M | P2 |
| BYTES scalar | 64 MiB per-property cap (by design) | — | — |
| NaN/Infinity | Fail at JSON serialization (`ERR_SERDE_FALLBACK`, by design) | — | P2 |

## Missing

| Feature | Notes | Effort | Priority |
|---|---|---|---|
| **Bolt protocol** | Biggest ecosystem gap: Neo4j drivers/tools (Browser, Bloom, official drivers) can't connect. REST+RPC only. | L | P0 |
| **MATCH ... CREATE mixed in executor** | Architectural (executor clones RecordStore); works via engine path — unification (04) resolves the user-visible part | M | P1 |
| Multiple patterns in one CREATE (`CREATE (a), (b)`) | Workaround: separate CREATEs | S | P2 |
| APOC library (coll/string/path/…) | Vendor lib; large surface. Prioritize top-20 most-used procs | L | P2 |
| `randomUUID()` | Not found in dispatch | S | P1 |
| String fns: `ascii`, `chr`, `lpad`, `rpad`, `normalize` | | S | P2 |
| Math: `log(x, base)`, `isNaN` | | S | P2 |
| List: `shuffle` | | S | P2 |
| Streaming result cursors | REST returns full JSON arrays; no pagination cursors | M | P2 |
| CREATE FUNCTION bodies (UDF execution) | Parses; execution unverified | M | P2 |
| Relationship existence constraints | Node-scope only today | M | P2 |
| Query TIMEOUT clause | | S | P2 |
| Encryption at rest | | L | P2 |

## What the 300-test diff suite actually covers

The `scripts/compatibility/test-neo4j-nexus-compatibility-200.ps1` suite
(300/300 green) covers basic queries, pattern matching, aggregation, string /
math / temporal functions, NULL handling, CASE, UNION, MERGE — **all through
the paths that work**. It does not exercise: HTTP MERGE-relationship, SET on
rel variables over HTTP, GraphQL mutations, RPC parameterized writes — which
is why the transport-correctness bugs survived it. Action: extend the suite to
run the same battery over every transport (see task
`phase1_write-path-parity-harness`).

## Priority roadmap to "100% practical compatibility"

1. **P0 — correctness before features**: unify the write path
   ([04](04-write-path-unification.md)) so the ~95% engine parity is what
   every transport actually delivers.
2. **P0 — Bolt protocol**: unlocks the Neo4j driver ecosystem + Browser;
   biggest single adoption lever.
3. **P1 — close the small-function tail** (`randomUUID`, string/math/list
   fns, elementId format): cheap wins that remove diff-suite asterisks.
4. **P1 — QPP slice 2, full-text auto-refresh, `CALL {} IN TX` concurrency.**
5. **P2 — APOC top-20, cursors, UDFs, rel constraints.**
