"""Co-purchase recommendation against Nexus (the *after* picture).

Same pipeline as `kuzu_before.py` ported to Nexus. The two changes
worth calling out:

* `[*SHORTEST 2..4]` becomes `shortestPath((...)-[*2..4]-...)`.
  Nexus exposes shortest-path through the `shortestPath()` Cypher
  function (Neo4j-compatible). The migration script's
  `rewrite-cypher` subcommand drops a `TRANSLATOR-NOTE` reminding
  you to wrap the pattern.
* The KNN search procedure returns cosine **similarity**, so the
  `ORDER BY` direction flips.
"""

from __future__ import annotations

import asyncio

from nexus_sdk import NexusClient


async def main() -> None:
    async with NexusClient("nexus://localhost:15475") as client:
        await client.execute_cypher(
            "CREATE CONSTRAINT user_id_unique IF NOT EXISTS "
            "FOR (u:User) REQUIRE u.id IS UNIQUE"
        )
        await client.execute_cypher(
            "CREATE CONSTRAINT product_id_unique IF NOT EXISTS "
            "FOR (p:Product) REQUIRE p.id IS UNIQUE"
        )
        await client.execute_cypher(
            "CALL db.knn.create('Product', 'embedding', 256, "
            "{ M: 16, efConstruction: 200, efSearch: 100 }) YIELD name"
        )

        behavioural = await client.execute_cypher(
            "MATCH (me:User { id: $uid }), (candidate:Product), "
            "      p = shortestPath((me)-[*2..4]-(candidate)) "
            "WHERE NOT (me)-[:PURCHASED]->(candidate) "
            "RETURN candidate.sku AS sku, length(p) AS hops "
            "ORDER BY hops ASC "
            "LIMIT 50",
            {"uid": 1},
        )

        target_vec: list[float] = [0.0] * 256

        semantic = await client.execute_cypher(
            "CALL db.knn.search('Product', 'embedding', $vec, 50) "
            "YIELD node, score "
            "RETURN node.sku AS sku, score "
            "ORDER BY score DESC",
            {"vec": target_vec},
        )

        print(behavioural.rows[:5], semantic.rows[:5], sep="\n---\n")


if __name__ == "__main__":
    asyncio.run(main())
