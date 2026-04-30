"""Co-purchase recommendation against KuzuDB (the *before* picture).

Read alongside `nexus_after.py`. The pipeline:

1. Build a `User`-`PURCHASED`->`Product` graph from a transactions
   CSV.
2. For a target user, surface the K nearest products via two paths:
   * **Behavioural**: shortest-path expansion through other users
     who also bought what the target user bought.
   * **Semantic**: cosine-similarity over a `Product.embedding`
     column.
3. Combine the two ranked lists with reciprocal-rank fusion at the
   application layer.
"""

from __future__ import annotations

import kuzu  # type: ignore[import-not-found]


def main() -> None:
    db = kuzu.Database("./recsys.kz")
    conn = kuzu.Connection(db)

    conn.execute("""
        CREATE NODE TABLE IF NOT EXISTS User(id SERIAL PRIMARY KEY, name STRING);
        CREATE NODE TABLE IF NOT EXISTS Product(
            id SERIAL PRIMARY KEY,
            sku STRING,
            embedding FLOAT[256]
        );
        CREATE REL TABLE IF NOT EXISTS Purchased(FROM User TO Product, qty INT64);
        """)

    # Behavioural recall: products bought by users similar to me
    # within 4 hops of my purchase graph.
    behavioural = conn.execute(
        """
        MATCH p = (me:User {id: $uid})
                  -[*SHORTEST 2..4]-(candidate:Product)
        WHERE NOT (me)-[:Purchased]->(candidate)
        RETURN candidate.sku AS sku, length(p) AS hops
        ORDER BY hops ASC
        LIMIT 50
        """,
        {"uid": 1},
    ).get_as_df()

    # Semantic recall: products with embeddings nearest to the
    # target user's recent purchase history (averaged externally).
    target_vec: list[float] = [0.0] * 256

    semantic = conn.execute(
        """
        CALL QUERY_VECTOR_INDEX('Product', 'product_embedding', $vec, 50)
        YIELD node, distance
        RETURN node.sku AS sku, distance
        ORDER BY distance ASC
        """,
        {"vec": target_vec},
    ).get_as_df()

    print(behavioural.head(), semantic.head(), sep="\n---\n")


if __name__ == "__main__":
    main()
