# Cookbook — GraphRAG (Kùzu → Nexus)

A retrieval-augmented generation pipeline lives or dies by the
quality of its retrieval. Both Kùzu and Nexus support hybrid
vector + graph-traversal retrieval; the surfaces differ only in
naming and score conventions.

This cookbook ports a minimal end-to-end pipeline:

1. Chunk a corpus and embed each chunk.
2. Persist chunks as `Document` nodes with the embedding as a
   property.
3. Link consecutive chunks with `:NEXT` so we can surface
   neighbouring context around a hit.
4. Answer a query by combining KNN search with two hops over
   `:NEXT`.

## Files

| File | Engine |
|---|---|
| [`kuzu_before.py`](./kuzu_before.py) | Kùzu — `CREATE_HNSW_INDEX` + `QUERY_VECTOR_INDEX` table functions, in-process. |
| [`nexus_after.py`](./nexus_after.py) | Nexus — `db.knn.create` + `db.knn.search` Cypher procs, RPC. |

The diff is intentionally minimal: the *what* is identical; only
the API surface and the score convention change.

## Three differences worth flagging

1. **Score sign**. Kùzu reports cosine **distance** (lower is
   better). Nexus reports cosine **similarity** (higher is
   better). Flip every `ORDER BY distance ASC` to
   `ORDER BY score DESC`.
2. **Index DDL**. Kùzu's `CREATE_HNSW_INDEX('Label', 'idx_name',
   'column', mu := M, efc := efConstruction)` becomes
   `db.knn.create('Label', 'column', dim, { M, efConstruction,
   efSearch })`. Note the extra `dim` and `efSearch` parameters.
3. **Variable-length edges**. Kùzu requires an explicit
   relationship type. Nexus does not. The translator script
   ([`scripts/migration/from_kuzu.py`](../../from_kuzu.py)) does not
   touch this — Kùzu queries that *do* name the type port
   unchanged.

## See also

- [`docs/migration/FROM_KUZU.md`](../../../../docs/migration/FROM_KUZU.md) — full migration guide.
- [`docs/performance/KNN_RECALL.md`](../../../../docs/performance/KNN_RECALL.md) — HNSW tuning methodology.
