# Full-Text Search

phase6_opencypher-fulltext-search ships the Neo4j `db.index.fulltext.*`
procedure namespace on top of a Tantivy 0.22 backend. Nexus now
maintains named BM25-scored full-text indexes over node / relationship
property sets and exposes them through the same CALL surface Neo4j
drivers already use.

## Supported procedures (v1.8)

| Procedure | Shape | Notes |
|---|---|---|
| `db.index.fulltext.createNodeIndex(name, labels, properties, config?)` | `()` | Creates a node-scoped index |
| `db.index.fulltext.createRelationshipIndex(name, types, properties, config?)` | `()` | Relationship-scoped variant |
| `db.index.fulltext.queryNodes(name, query, limit?)` | `(node, score)` | BM25 ranking, ties by node id asc |
| `db.index.fulltext.queryRelationships(name, query, limit?)` | `(relationship, score)` | Same shape, rel-scoped |
| `db.index.fulltext.drop(name)` | `()` | Idempotent when the name is unknown |
| `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()` | `()` | No-op (reader reloads synchronously in v1.8) |
| `db.index.fulltext.listAvailableAnalyzers()` | `(analyzer)` | Returns the `standard` baseline in v1.8 |

Every FTS index created this way appears in `db.indexes()` with
`type = "FULLTEXT"` and `indexProvider = "tantivy-0.22"`, matching the
columns Neo4j tooling already knows how to render.

## Minimal example

```cypher
CALL db.index.fulltext.createNodeIndex(
  'moviesFts',
  ['Movie'],
  ['title', 'overview']
)

// ... data ingested via CREATE / MERGE, or programmatically via
// Engine::fulltext_add_node_document (see "Write path" below)

CALL db.index.fulltext.queryNodes('moviesFts', 'matrix')
YIELD node, score
RETURN node.title AS title, score
ORDER BY score DESC
LIMIT 10
```

## Storage layout

Each index lives under `<data_dir>/indexes/fulltext/<name>/` as a
standalone Tantivy directory. Dropping the index best-effort removes
the directory tree. Directory state is reused when the registry is
re-instantiated, so a graceful restart does not rebuild the index.

## Write path (v1.8 scope)

`CREATE` / `MERGE` / `SET` do **not** yet auto-enqueue rows to the
FTS backend — that hook is parked for the WAL-integration follow-up.
Callers that want populated indexes today use the programmatic API:

```rust
let reg = engine.indexes().fulltext.clone();
reg.add_node_document("moviesFts", node_id, label_id, key_id, text)?;
```

Tantivy commits on every call and reloads the reader synchronously,
so the next `queryNodes` sees the document without waiting for any
refresh tick.

## Query syntax

The backend is Tantivy's `QueryParser`, which accepts the Lucene-like
subset Neo4j users expect: bare terms, phrases (`"quick fox"`), boolean
connectives (`+`, `-`, `AND`, `OR`, `NOT`), fielded `field:value`, and
prefix `term*`. Malformed queries surface as `ERR_FTS_PARSE` from the
wrapped parser error.

## Ranking

BM25 is the default Tantivy scorer. `top_k` defaults to 100 per index;
callers pass an explicit `limit` as the third procedure argument when
they want a different cut-off. Tie-breaks use node id ascending order.

## Error codes

| Code | Trigger |
|---|---|
| `ERR_FTS_INDEX_EXISTS` | `createNodeIndex` called with a name already in the registry |
| `ERR_FTS_INDEX_NOT_FOUND` | `queryNodes` / `drop` called on an unknown name |
| `ERR_FTS_INDEX_INVALID` | Empty labels-list or properties-list on create |
| `ERR_FTS_PARSE` | Tantivy rejected the query string |

## Parked follow-ups

The tail of the feature ships behind explicit follow-up tasks:

- **WAL integration** — `OP_FTS_ADD / OP_FTS_DEL` + commit-hook
  enqueue so CREATE / MERGE / SET auto-populate the index and crash
  recovery replays pending docs.
- **Per-index analyzer config** — `whitespace`, `simple`, `keyword`,
  and n-gram analyzers wired through the `config` map argument.
- **Bench targets** — single-term < 5 ms p95, phrase < 20 ms p95,
  ingest > 5k docs/sec (Criterion harness).
- **TCK import** — the fulltext scenarios from the Neo4j TCK.

Each is tracked as a standalone rulebook task; this release is
scoped to the DDL + query path so the procedures behave correctly
under driver-level tooling.
