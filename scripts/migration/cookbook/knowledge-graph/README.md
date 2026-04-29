# Cookbook — Knowledge graph with hybrid query (Kùzu → Nexus)

A research knowledge graph with two query shapes:

* **Hybrid retrieval**: full-text + vector + citation
  neighbourhood. Three index types active simultaneously.
* **Influence path**: shortest path between two papers through
  `:CITES`.

The migration is largely a search-and-replace exercise. The
[`from_kuzu.py rewrite-cypher`](../../from_kuzu.py) subcommand
handles the FTS / vector / shortest-path constructs automatically;
manual review is needed only for the score sign-flip on KNN.

| Construct | Kùzu | Nexus |
|---|---|---|
| HNSW DDL | `CALL CREATE_HNSW_INDEX('L', 'idx', 'col', mu := M, efc := efC)` | `CALL db.knn.create('L', 'col', dim, { M, efConstruction, efSearch })` |
| FTS DDL | `CALL CREATE_FTS_INDEX('L', 'idx', [...])` | `CALL db.index.fulltext.createNodeIndex('idx', ['L'], [...])` |
| HNSW query | `CALL QUERY_VECTOR_INDEX('L', 'idx', $vec, k)` returns `distance` | `CALL db.knn.search('L', 'col', $vec, k)` returns `score` |
| FTS query | `CALL QUERY_FTS_INDEX('L', 'idx', $q)` | `CALL db.index.fulltext.queryNodes('idx', $q)` |
| Shortest path | `MATCH p = (a)-[*SHORTEST n..m]->(b)` | `MATCH p = shortestPath((a)-[*n..m]->(b))` |

## Files

| File | Engine |
|---|---|
| [`kuzu_before.py`](./kuzu_before.py) | Kùzu reference. |
| [`nexus_after.py`](./nexus_after.py) | Nexus port. |

## See also

- [`docs/migration/FROM_KUZU.md`](../../../../docs/migration/FROM_KUZU.md) — full migration guide.
- [`docs/performance/KNN_RECALL.md`](../../../../docs/performance/KNN_RECALL.md) — HNSW tuning.
