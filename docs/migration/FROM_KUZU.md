# Migrating from Kùzu (KuzuDB) to Nexus

> Kùzu Inc. archived the [KuzuDB GitHub repository](https://github.com/kuzudb/kuzu)
> on 2025-10-10. This guide exists for the displaced user base —
> embedded analytics, GraphRAG pipelines, Cypher-with-WASM front-ends —
> who need a path forward without rewriting from scratch.
>
> **Audience:** users running Kùzu v0.6.x – v0.10.x in production.
> Multiple community forks (Bighorn, Ladybug, RyuGraph) exist as of
> the time of writing; they are early. Nexus is feature-complete for
> the Kùzu use case today: 300/300 Neo4j compatibility, native HNSW
> vector search, Tantivy-backed full-text search, ACID transactions.

## TL;DR

| Concern | Kùzu | Nexus | Effort |
|---|---|---|---|
| Cypher dialect | Subset (~70%) | Neo4j 300/300 — strict superset of Kùzu | Low (Kùzu queries usually port unchanged) |
| Schema model | Strict typed (`CREATE NODE TABLE`) | Schema-flexible with optional unique/range indexes | Low (DDL maps 1:1) |
| Embedding | In-process (Python / C++ / Node) | Single-binary server + native binary RPC | Medium (RPC client replaces in-proc handle) |
| Vector index | `hnsw` index per table | `KnnIndex` per label | Low (DDL + data load via `LOAD CSV` + `CALL` proc) |
| FTS index | `fts` index per table | `db.index.fulltext.*` (Neo4j-compat) | Low |
| Bulk load | `COPY <table> FROM 'file.csv'` | `LOAD CSV` + `MERGE` / bulk RPC | Medium (CSV unchanged; DDL prepends labels) |
| WASM build | Yes | Not yet | High — see [§ Gotchas](#gotchas) |

The migration script [`scripts/migration/from_kuzu.py`](../../scripts/migration/from_kuzu.py)
ingests a Kùzu CSV/Parquet export into a running Nexus instance via
the binary RPC transport.

## 1. Schema mapping

Kùzu requires every node and relationship to belong to a strictly
typed table. Nexus stores nodes by label and relationships by type
without an enclosing table. The mapping is therefore lossy in the
*safe* direction: Kùzu schema → Nexus schema preserves every fact,
but Nexus schema → Kùzu schema may need additional constraints.

### Node tables → labels

```sql
-- Kùzu
CREATE NODE TABLE Person(
    id    SERIAL PRIMARY KEY,
    name  STRING,
    age   INT64,
    email STRING UNIQUE
);
```

```cypher
// Nexus
CREATE CONSTRAINT person_email_unique IF NOT EXISTS
    FOR (p:Person) REQUIRE p.email IS UNIQUE;

CREATE INDEX person_id IF NOT EXISTS FOR (p:Person) ON (p.id);
```

The Kùzu `SERIAL` primary key has no direct Nexus equivalent — Nexus
auto-assigns `id(node)` opaquely. Carry your own `id` property if
your application relies on stable external IDs (the migration script
preserves whatever Kùzu wrote).

### Relationship tables → types

```sql
-- Kùzu
CREATE REL TABLE Knows(
    FROM Person TO Person,
    since DATE,
    weight DOUBLE
);
```

```cypher
// Nexus — no DDL required; relationship types are created on first use.
// Optional: range index on the property the planner should care about.
CREATE INDEX knows_since IF NOT EXISTS FOR ()-[r:KNOWS]-() ON (r.since);
```

Kùzu `MANY_MANY`, `ONE_MANY`, `ONE_ONE` cardinalities are *advisory*
in Nexus — enforce them with `MERGE` and unique constraints rather
than at the DDL layer.

### Type mapping

| Kùzu type | Nexus type | Notes |
|---|---|---|
| `STRING` | string | UTF-8 in both. |
| `INT8` / `INT16` / `INT32` / `INT64` | integer | Nexus integers are 64-bit. |
| `UINT8` … `UINT64` | integer | Nexus does not have an unsigned variant; values fit if `< 2^63`. |
| `DOUBLE` / `FLOAT` | float | Nexus floats are 64-bit. |
| `BOOL` | boolean | |
| `DATE` | date | |
| `TIMESTAMP` | datetime | |
| `INTERVAL` | duration | |
| `BLOB` | bytes (Base64-encoded in Cypher literals) | |
| `STRUCT(...)` | map | Use `{ key: value }` literals. |
| `LIST(T)` / `T[]` | list | Homogeneous lists work without conversion. |
| `MAP(K, V)` | map | Keys are coerced to strings. |
| `UNION(...)` | (no equivalent) | Promote to a discriminator field. |

## 2. Cypher dialect differences

Kùzu's Cypher subset and Nexus's Neo4j-compatible surface diverge in
five well-defined places. The migration script auto-translates the
first three; the last two require manual review.

### 2.1 `MATCH (a)-[*1..3]->(b)` — variable-length edges

Kùzu requires explicit relationship-type filters on variable-length
patterns. Nexus does not. Both forms below work in Nexus; only the
second works in Kùzu.

```cypher
// Both engines
MATCH path = (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN path;

// Nexus only — Kùzu rejects untyped variable-length edges.
MATCH path = (a:Person)-[*1..3]->(b:Person) RETURN path;
```

No translation needed — Kùzu queries port unchanged.

### 2.2 `LIMIT` / `SKIP`

Kùzu accepts `LIMIT` only on the outermost projection. Nexus accepts
`LIMIT` and `SKIP` after every `WITH` and `RETURN`, matching Neo4j.
Existing Kùzu queries continue to work.

### 2.3 `RETURN DISTINCT`

Identical semantics. No translation.

### 2.4 Recursive Cypher (Kùzu's `*` shortest path)

Kùzu has a built-in `MATCH p = (a)-[*SHORTEST 1..N]->(b)` shorthand.
Nexus exposes the same capability via the `algo.shortestPath`
procedure (300/300 Neo4j compat ships this) plus the
`shortestPath()` Cypher function.

```cypher
// Kùzu
MATCH path = (a:Person {name:'Alice'})-[*SHORTEST 1..6]-(b:Person {name:'Bob'})
RETURN path;
```

```cypher
// Nexus — equivalent.
MATCH (a:Person {name:'Alice'}), (b:Person {name:'Bob'}),
      path = shortestPath((a)-[*1..6]-(b))
RETURN path;
```

The migration script rewrites the `[*SHORTEST n..m]` shape
automatically. Manual review is needed only for queries that pass
algorithm options (`(weights:=$column)`, `(top_k:=N)`) — those map
to `algo.shortestPath` proc-call kwargs.

### 2.5 Vector search

Kùzu exposes vector search through a `vector_search()` table function.
Nexus exposes it through Cypher procs. Examples in
[§ 4. Vector index migration](#4-vector-index-migration).

## 3. Data loading

Kùzu can export to CSV (`COPY <table> TO 'file.csv'`) or Parquet
(`COPY <table> TO 'file.parquet'`). Both round-trip cleanly into
Nexus. Native `.kz` files are not portable — go through a CSV /
Parquet hop.

### 3.1 Quick path: the migration script

```bash
# 1. Export every Kùzu table to CSV.
kuzu my-kuzu-db --query "COPY Person TO 'kuzu-out/person.csv' (HEADER=true);"
kuzu my-kuzu-db --query "COPY Knows  TO 'kuzu-out/knows.csv'  (HEADER=true);"

# 2. Translate Kùzu CSV into Nexus bulk-load format.
python scripts/migration/from_kuzu.py \
  --node Person:kuzu-out/person.csv \
  --rel  Person-KNOWS-Person:kuzu-out/knows.csv \
  --target nexus://localhost:15475 \
  --out-dir migrated/

# 3. Verify counts and a few representative queries.
nexus query "MATCH (p:Person) RETURN count(p)"
nexus query "MATCH (:Person)-[r:KNOWS]->(:Person) RETURN count(r)"
```

The script emits Nexus-flavoured `LOAD CSV` driver scripts under
`migrated/` plus a streaming bulk-RPC ingester. Pick one based on
data volume — `LOAD CSV` is fine up to ~10 M rows; the bulk RPC
ingester handles arbitrarily large inputs in chunks.

### 3.2 Manual path: `LOAD CSV`

```cypher
LOAD CSV WITH HEADERS FROM 'file:///kuzu-out/person.csv' AS row
CREATE (:Person { id: toInteger(row.id), name: row.name, age: toInteger(row.age) });

LOAD CSV WITH HEADERS FROM 'file:///kuzu-out/knows.csv' AS row
MATCH (a:Person { id: toInteger(row.from) }),
      (b:Person { id: toInteger(row.to)   })
CREATE (a)-[:KNOWS { since: date(row.since), weight: toFloat(row.weight) }]->(b);
```

`MERGE` is safer than `CREATE` if the export might overlap with the
existing Nexus database (re-runs become idempotent).

### 3.3 Bulk RPC path

For very large datasets, the migration script uses
`hivehub-nexus-sdk`'s `batch_create_nodes` /
`batch_create_relationships` over the binary RPC transport. The
script chunks the CSV at 10 000 rows per batch by default; tune with
`--batch-size`.

## 4. Vector index migration

Kùzu's `CREATE_HNSW_INDEX` projects a per-table vector column. Nexus
attaches the index to a label.

```cypher
// Kùzu
CALL CREATE_HNSW_INDEX('Document', 'embedding_idx', 'embedding',
                       mu := 30, ml := 60, efc := 200);

// Nexus
CALL db.knn.create('Document', 'embedding', 768, {
    M: 16, efConstruction: 200, efSearch: 50
}) YIELD name;
```

Bulk-load the embeddings same as Kùzu — they live as a `LIST(FLOAT)`
property on the node. The Nexus engine's `KnnIndex::add_vector` is
called automatically when a node with an indexed property is
written.

Query side:

```cypher
// Kùzu
CALL QUERY_VECTOR_INDEX('Document', 'embedding_idx', $query_vec, 10)
YIELD node, distance
RETURN node.title, distance ORDER BY distance;
```

```cypher
// Nexus
CALL db.knn.search('Document', 'embedding', $query_vec, 10)
YIELD node, score
RETURN node.title, score ORDER BY score DESC;
```

Note the score sign flip: Kùzu reports cosine **distance** (lower =
closer), Nexus reports cosine **similarity** (higher = closer). The
migration script does not rewrite these — the score column changes
sign and ordering, so you must update the application code.

The full recall/latency methodology lives in
[`docs/performance/KNN_RECALL.md`](../performance/KNN_RECALL.md).

## 5. Full-text search migration

Kùzu's FTS module is similar shape to Neo4j's; Nexus implements the
Neo4j-compatible procedure surface, so the rename is mechanical.

```cypher
// Kùzu
CALL CREATE_FTS_INDEX('Document', 'fts_body', ['title', 'body'],
                      stemmer := 'english');
CALL QUERY_FTS_INDEX('Document', 'fts_body', 'graph databases')
YIELD node, score RETURN node.title, score;
```

```cypher
// Nexus
CALL db.index.fulltext.createNodeIndex(
    'Document_fts', ['Document'], ['title', 'body']
);
CALL db.index.fulltext.queryNodes('Document_fts', 'graph databases')
YIELD node, score RETURN node.title, score;
```

Index names cannot collide with KNN indexes; we recommend a `_fts`
suffix.

## 6. Embedded mode → RPC

Kùzu's headline feature is in-process embedding (`Database` /
`Connection` types in the Python or C++ SDK). Nexus is a single-
binary server: callers connect via the native binary RPC transport
on port 15475.

This **is** the largest behavioural change in the migration. The
practical impact is small — the SDK call shape is nearly identical
— but the deployment topology changes.

| | Kùzu | Nexus |
|---|---|---|
| Binary | Linked into your process | Separate `nexus-server` process |
| Startup | `Database("path")` | One-shot `nexus-server &` (or Docker, or systemd) |
| First call | Microseconds | Sub-millisecond round trip on localhost |
| Concurrency | Single-process readers | Multi-process readers + writers, MVCC |

Code-shape comparison:

```python
# Kùzu (in-proc)
import kuzu
db = kuzu.Database("./mydb")
conn = kuzu.Connection(db)
result = conn.execute("MATCH (n:Person) RETURN n.name LIMIT 10").get_as_df()
```

```python
# Nexus (RPC)
from nexus_sdk import NexusClient
async with NexusClient("nexus://localhost:15475") as client:
    result = await client.execute_cypher(
        "MATCH (n:Person) RETURN n.name LIMIT 10"
    )
    print([row[0] for row in result.rows])
```

If your Kùzu use case truly needs in-process embedding (no Unix
domain socket / TCP tolerance — e.g. a sandboxed Lambda), Nexus does
not have a drop-in answer today. The RPC transport supports Unix
domain sockets via `nexus+uds:///run/nexus.sock` in the SDK config,
which approximates the latency profile of in-proc.

## 7. Performance expectations

Numbers measured on a Ryzen 9 7950X3D, 64 GB DDR5-6000, Windows MSVC
nightly. See [`docs/performance/PERFORMANCE_V1.md`](../performance/PERFORMANCE_V1.md)
for the full matrix.

| Workload | Kùzu (v0.10.x) | Nexus (v1.15.x) | Delta |
|---|---|---|---|
| Point read p95 | sub-ms (in-proc) | sub-ms (RPC over loopback) | comparable |
| 1-hop traversal p95 | sub-ms | sub-ms | comparable |
| KNN top-10 @ 1 M × 768d | low single-digit ms | sub-ms (HNSW SIMD) | Nexus faster |
| Bulk load (CSV) | high | high | comparable |
| FTS phrase query p95 | depends on Lucene fork | 4.6 ms (Tantivy) | Nexus faster |
| Multi-writer transactions | single-process | multi-process MVCC | Nexus higher concurrency |

Kùzu wins on cold-start latency (no process to spawn) and on the
analytical / OLAP query shapes where its columnar storage paid off.
Nexus wins on hybrid graph + vector + FTS queries, on multi-writer
concurrency, and on the SDK breadth (six languages vs three).

## 8. Gotchas

- **No WASM build yet.** Kùzu shipped a WASM build for in-browser
  Cypher; Nexus does not. Tracked under
  [`phase8_wasm-build`](../../.rulebook/tasks/) (planned, not started).
  Front-ends needing client-side Cypher should keep Kùzu (or one of
  its forks) for the browser tier and call Nexus over RPC from the
  server tier.
- **Single writer per partition.** Nexus uses epoch-based MVCC with
  one writer at a time per partition. Kùzu's per-process model has
  the same bottleneck if you only ran one process; the change matters
  only if you previously sharded by spawning multiple Kùzu processes.
- **No in-proc binding.** Even on the same machine, callers connect
  via TCP loopback or Unix domain socket. The bench overhead vs
  Kùzu in-proc is measurable but typically << 100 µs per call.
- **Schema-flexibility surprises.** Kùzu rejects untyped property
  writes; Nexus accepts them and stores the property. If your
  application implicitly relied on Kùzu's schema validation, port
  it to Nexus `CREATE CONSTRAINT … REQUIRE …` ahead of the data
  load.
- **Rel cardinality not enforced.** `MANY_MANY`, `ONE_MANY` are
  advisory. Use `MERGE` + unique constraints to enforce.
- **Auto-incrementing keys.** Kùzu's `SERIAL` PK has no Nexus
  equivalent; the migration script preserves whatever Kùzu emitted
  but does not generate fresh sequences. Use a UUID property if
  you don't already have a stable external ID.

## 9. Cookbook

Three end-to-end Kùzu → Nexus migrations live under
[`scripts/migration/cookbook/`](../../scripts/migration/cookbook/):

- [`graphrag/`](../../scripts/migration/cookbook/graphrag/) —
  document chunking, embedding, KNN search, traversal-augmented
  retrieval. Replaces Kùzu's `vector_search()` table function with
  `db.knn.search`.
- [`recommendation/`](../../scripts/migration/cookbook/recommendation/) —
  user-product co-purchase graph + cosine-similarity neighbour
  surfacing. Replaces Kùzu's recursive `MATCH (*SHORTEST n..m)`
  with Nexus's `shortestPath()`.
- [`knowledge-graph/`](../../scripts/migration/cookbook/knowledge-graph/) —
  hybrid graph + vector + FTS. Replaces Kùzu's `QUERY_FTS_INDEX` /
  `QUERY_VECTOR_INDEX` with the Neo4j-compatible
  `db.index.fulltext.queryNodes` + `db.knn.search`.

Each cookbook ships a `kuzu_before.py` / `nexus_after.py` pair so
the diff is one file open away.

## 10. Cross-references

- [Migration script](../../scripts/migration/from_kuzu.py) —
  Kùzu CSV → Nexus bulk RPC ingester.
- [`docs/performance/PERFORMANCE_V1.md`](../performance/PERFORMANCE_V1.md) —
  full performance matrix.
- [`docs/performance/KNN_RECALL.md`](../performance/KNN_RECALL.md) —
  HNSW recall + latency methodology.
- [`docs/CLUSTER_MODE.md`](../CLUSTER_MODE.md) — multi-tenant cluster
  mode (relevant if your Kùzu deployment was multi-tenant via
  separate processes).
- [`deploy/helm/nexus`](../../deploy/helm/nexus/README.md) — K8s
  deployment for the new server tier.
- [`docs/MIGRATION_SDK_TRANSPORT.md`](./MIGRATION_SDK_TRANSPORT.md) —
  SDK transport contract (relevant if you were already using a
  Cypher RPC layer in front of Kùzu).
