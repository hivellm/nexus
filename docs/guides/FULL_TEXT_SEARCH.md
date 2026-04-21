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

### Catalogue persistence (v1.11)

Every create writes a `_meta.json` sidecar into the index directory
with the registry-level metadata (name, entity scope, labels /
types, properties, resolved analyzer, refresh_ms, top_k). On engine
startup `FullTextRegistry::load_from_disk` scans the base directory
and re-opens every index it finds — so the catalogue survives
restarts without a WAL replay. Malformed or unreadable sidecars are
logged and skipped rather than aborting the whole boot path.

### WAL integration (v1.11)

The WAL op-code set grew four FTS codes:

| Code | Shape | Replayed by |
|---|---|---|
| `0x40` `FtsCreateIndex` | name + entity + labels + properties + analyzer | `FullTextRegistry::apply_wal_entry` |
| `0x41` `FtsDropIndex`   | name | same |
| `0x42` `FtsAdd`         | name + entity_id + label_or_type_id + key_id + content | same |
| `0x43` `FtsDel`         | name + entity_id | same |

`apply_wal_entry` returns `Ok(false)` when the entry is not
FTS-shaped, so a recovery loop feeds every decoded entry through
and lets the registry pick the ones it owns. Idempotent on replay:
a duplicate `FtsCreateIndex` against an already-registered name is
a no-op; an `FtsAdd` / `FtsDel` against a name that no longer
exists is skipped rather than aborting recovery.

The commit-hook path that turns every `CREATE` / `MERGE` / `SET`
into WAL-backed FTS ops is scoped for the next slice of
`phase6_fulltext-wal-integration`; today callers drive the
programmatic API (or the bulk-ingest path) and are responsible
for emitting the matching WAL entries.

## Write path (v1.12)

`CREATE` / `SET` / `REMOVE` / `DELETE` auto-maintain every
registered FTS index whose labels + properties match the node
being mutated. Users no longer need to call `add_node_document`
explicitly — the executor + engine hooks do it on every write.

- `CREATE (:Label {p: 'text'})` → `FtsAdd` for every index whose
  `labels` match `Label` and `properties` include `p`.
- `SET n.p = 'new'` → delete-then-add: the stale doc goes, a
  fresh one lands with the new content.
- `REMOVE n.p` → delete-then-add with empty content = effective
  eviction.
- `DELETE n` → `FtsDel` for every index the node was in.

Every mutation emits the matching `FtsAdd` / `FtsDel` WAL entry
(see *WAL integration* below) so crash recovery can replay the
FTS state.

### Programmatic API

Callers outside the Cypher pipeline (or wanting to back-fill an
existing dataset after registering a new index) drive the
registry directly.

Interactive (one-doc-at-a-time) callers use `add_node_document` —
commits and reloads the reader synchronously, so the next
`queryNodes` sees the document without waiting for any refresh
tick:

```rust
let reg = engine.indexes().fulltext.clone();
reg.add_node_document("moviesFts", node_id, label_id, key_id, text)?;
```

Bulk loaders (import scripts, catch-up rebuilds) use
`add_node_documents_bulk` — one Tantivy writer, every doc, one
commit. Delivers ≈60 k docs/sec on the reference hardware vs.
Tantivy segment-flush latency floored by per-doc commits:

```rust
let docs: Vec<(u64, u32, u32, &str)> = rows
    .iter()
    .map(|r| (r.node_id, r.label_id, r.key_id, r.text.as_str()))
    .collect();
reg.add_node_documents_bulk("moviesFts", &docs)?;
```

## Analyzer catalogue (v1.9)

Pick an analyzer per index via the `config` map argument of
`createNodeIndex` / `createRelationshipIndex`. Default is
`standard` (Neo4j parity).

| Name | Behaviour |
|---|---|
| `standard` | Lowercase + English stopword removal. Default. |
| `whitespace` | Split on whitespace only; case preserved. |
| `simple` | Lowercase + split on non-alphanumeric runs. |
| `keyword` | Single token pass-through; case preserved. |
| `ngram` | Character n-grams. Default `2..3`; configurable via `ngram_min` / `ngram_max` in `config`. |
| `english` | English stemmer + lowercase + stopwords. |
| `spanish` | Spanish stemmer + lowercase + stopwords. |
| `portuguese` | Portuguese stemmer + lowercase + stopwords. |
| `german` | German stemmer + lowercase + stopwords. |
| `french` | French stemmer + lowercase + stopwords. |

Example:

```cypher
CALL db.index.fulltext.createNodeIndex(
  'imageCaptions',
  ['Image'],
  ['caption'],
  {analyzer: 'ngram', ngram_min: 3, ngram_max: 5}
)
```

Unknown analyzer names surface `ERR_FTS_UNKNOWN_ANALYZER`. Call
`db.index.fulltext.listAvailableAnalyzers()` to enumerate the
catalogue at runtime (rows are alphabetical, matching Neo4j).

The resolved analyzer name is echoed back through the
`options.analyzer` field of each row returned by `db.indexes()`,
so driver tooling can render the tokenisation choice without
probing the backend.

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
| `ERR_FTS_UNKNOWN_ANALYZER` | `config.analyzer` is not in the catalogue, or `ngram` sizes are invalid |

## Parked follow-ups

The tail of the feature ships behind explicit follow-up tasks:

- **WAL integration** — `OP_FTS_ADD / OP_FTS_DEL` + commit-hook
  enqueue so CREATE / MERGE / SET auto-populate the index and crash
  recovery replays pending docs.
- ~~**Per-index analyzer config**~~ — shipped in v1.9
  (phase6_fulltext-analyzer-catalogue).
- ~~**Bench targets**~~ — shipped in v1.10
  (phase6_fulltext-benchmarks). See
  [docs/performance/PERFORMANCE_V1.md](../performance/PERFORMANCE_V1.md)
  for baselines.
- **TCK import** — the fulltext scenarios from the Neo4j TCK.

Each is tracked as a standalone rulebook task; this release is
scoped to the DDL + query path so the procedures behave correctly
under driver-level tooling.
