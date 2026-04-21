# Full-Text Search — Technical Design

## Scope

Ship Lucene-grade FTS over node and relationship properties, exposed
through Neo4j's `db.index.fulltext.*` procedure namespace. Built on
Tantivy 0.22.

## Choice of backend

| Option        | Pros                                      | Cons                         |
|---------------|-------------------------------------------|------------------------------|
| Roll our own  | Full control                              | 6–12 months to match Lucene  |
| `meilisearch` | Fast, ergonomic                           | Not embeddable as a library  |
| **Tantivy**   | **Embeddable, BM25, Lucene-like, Apache-2.0** | Single-writer per index     |

Tantivy wins on every axis that matters for a library-sized FTS inside
an embeddable database kernel.

## Directory layout

```
<data_dir>/
  catalog.lmdb
  fulltext/
    <index_name>/
      meta.json         -- Nexus metadata (labels, props, analyzer)
      segment-*.idx     -- Tantivy segments
      .lock             -- Tantivy's own exclusive-writer lock
```

Each index is a Tantivy `Index` object with its own directory. No two
indexes share a directory (matches Tantivy's design).

## Writer task

Tantivy requires exactly one writer per index. Writers live as
long-running tasks on a dedicated thread pool:

```rust
struct FtsWriter {
    index_name: String,
    tantivy: tantivy::IndexWriter,
    rx: mpsc::UnboundedReceiver<FtsOp>,
    refresh_ms: u64,
}

enum FtsOp {
    Add { node_id: u64, fields: Document },
    Del { node_id: u64 },
    Commit,
    Shutdown,
}
```

Commit hook in the transaction layer enqueues `Add`/`Del` on every
affected node. A periodic timer fires `Commit` every `refresh_ms`
(default 1000 ms) which flushes the writer's in-memory buffer to disk
segments.

## Schema

Every FTS index uses a fixed Tantivy schema:

```
_node_id    :  u64, STORED, INDEXED, FAST
_labels     :  STRING (facet, not analyzed)
<prop_1>    :  TEXT (analyzed per-index config)
<prop_2>    :  TEXT
...
```

Labels is a facet so the same index can back multi-label queries via
filter clauses. Property fields are TEXT for BM25 scoring.

## WAL ordering

Order of operations on transaction commit:

1. Transaction accepts at Raft/primary.
2. Writes commit to primary store.
3. `OP_FTS_ADD/DEL` entries appended to WAL.
4. Commit-hook enqueues the same ops on the FTS writer channel.
5. Periodic refresh flushes.

On crash before refresh: WAL replay re-enqueues the ops → the FTS
writer re-applies them → index converges to the committed state.

## Query parser

Accepts a Lucene-like subset:

```
term              -- simple term
"a phrase"        -- phrase query
+required -excluded
field:value       -- restrict to a property
term~2            -- fuzzy (edit distance)
term*             -- prefix
[start TO end]    -- range (on string fields)
```

Implemented by wrapping Tantivy's `QueryParser` with a light shim
that reshapes our error codes. Operators `AND`, `OR`, `NOT` are
supported as aliases for `+`, space (implicit OR), `-`.

## Procedure surface (Neo4j-compatible)

```
db.index.fulltext.createNodeIndex(name, labels, properties, config?)
    -- config: { analyzer: 'standard'|'whitespace'|'keyword'|'ngram',
    --           refresh_ms: 1000,
    --           top_k: 100 }

db.index.fulltext.queryNodes(name, query)
    -- returns: node :: NODE, score :: FLOAT
    -- ordered by score desc, ties by node id asc
    -- default top_k = 100, overridable via trailing LIMIT

db.index.fulltext.drop(name)
    -- closes writer, removes directory, removes catalogue entry

db.index.fulltext.awaitEventuallyConsistentIndexRefresh()
    -- blocks until every index has refreshed at least once since the call
```

## Cypher integration

Procedures compose with MATCH via `YIELD`:

```cypher
CALL db.index.fulltext.queryNodes('movies', 'star wars') YIELD node, score
MATCH (node)-[:HAS_GENRE]->(g)
RETURN node.title, score, collect(g.name) AS genres
ORDER BY score DESC
LIMIT 10
```

No new grammar. This is the same pattern GDS procedures already use.

## Consistency model

**Eventual consistency** by default: reads may lag writes by at most
`refresh_ms` milliseconds. This matches Neo4j's FTS behaviour.

For tests and strict workflows,
`db.index.fulltext.awaitEventuallyConsistentIndexRefresh()` forces
a synchronous refresh.

## Error taxonomy

| Code                        | Condition                                      |
|-----------------------------|------------------------------------------------|
| `ERR_FTS_PARSE`             | Query string failed to parse                   |
| `ERR_FTS_INDEX_NOT_FOUND`   | Named index does not exist                     |
| `ERR_FTS_INDEX_EXISTS`      | Index name taken (any kind)                    |
| `ERR_FTS_WRITER_UNAVAILABLE`| Writer task crashed; recovery in progress      |
| `ERR_FTS_FIELD_UNKNOWN`     | `field:value` references a non-indexed property|

## Configuration

```toml
[fulltext]
default_refresh_ms = 1000
max_concurrent_writers = 16
writer_heap_bytes = 50_000_000   # per-writer memory budget
```

## Benchmarks (targets)

| Scenario                                  | Target p95  |
|-------------------------------------------|-------------|
| Single-term query, 100k docs × 1 KB       | < 5 ms      |
| Phrase query, same corpus                 | < 20 ms     |
| Fuzzy `term~1` on 100k docs               | < 30 ms     |
| Bulk indexing throughput (1 KB docs)      | > 5 k/sec   |

## Rollout

- v1.3.0 ships FTS behind feature flag `fts_enabled = true` by default.
- The flag remains for one release to allow disable in constrained
  environments (it adds a transitive Tantivy dependency ~3 MB compiled).
- Removed at v1.4.

## Out of scope

- Hybrid retrieval operator (combining BM25 score with HNSW similarity
  in a single operator) — tracked as a later V2 item.
- Custom analyzers via user code — compile-time analyzers only.
- Search-API surface via a dedicated REST endpoint (`/search`); FTS is
  reached only through `CALL db.index.fulltext.*`.
