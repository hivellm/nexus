# Cookbook — Recommendation system (Kùzu → Nexus)

Co-purchase recommendation has two recall phases:

* **Behavioural**: shortest-path expansion across the
  user-purchased-product bipartite graph. Surfaces products bought
  by users with similar carts.
* **Semantic**: cosine-similarity search over a product-embedding
  column. Surfaces products that *look* similar in the embedding
  space.

Both engines support both phases; only the surface differs. The
ports are mechanical:

| Construct | Kùzu | Nexus |
|---|---|---|
| Shortest path | `[*SHORTEST 2..4]` | `shortestPath((...)-[*2..4]-...)` |
| Vector search | `CALL QUERY_VECTOR_INDEX('L', 'idx', $vec, k)` | `CALL db.knn.search('L', 'col', $vec, k)` |
| Score | distance, lower is better | similarity, higher is better |

## Files

| File | Engine |
|---|---|
| [`kuzu_before.py`](./kuzu_before.py) | Kùzu reference. |
| [`nexus_after.py`](./nexus_after.py) | Nexus port. |

## See also

- [`docs/migration/FROM_KUZU.md`](../../../../docs/migration/FROM_KUZU.md) — full migration guide.
