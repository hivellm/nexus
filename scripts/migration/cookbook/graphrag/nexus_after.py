"""GraphRAG retrieval against Nexus (the *after* picture).

Read alongside `kuzu_before.py`. Same retrieval pattern: chunk →
embed → store as `Document` nodes with a `:NEXT` chain → query with
hybrid vector + traversal.

Differences vs the Kùzu version:

* Cypher dialect: Neo4j-compatible. Variable-length edges no longer
  need an explicit type filter, and we use the `db.knn.*`
  procedures instead of Kùzu's `CREATE_HNSW_INDEX` /
  `QUERY_VECTOR_INDEX` table functions.
* Score sign: Nexus reports cosine **similarity** (higher = closer),
  so we sort `score DESC` instead of `distance ASC`.
* Connection: the Python SDK speaks the binary RPC transport on
  `nexus://localhost:15475` — start `nexus-server` separately.
"""

from __future__ import annotations

import asyncio

from nexus_sdk import NexusClient


async def main() -> None:
    async with NexusClient("nexus://localhost:15475") as client:
        # Optional: surface the desired uniqueness constraint so the
        # planner reaches for the index when matching by id.
        await client.execute_cypher(
            "CREATE CONSTRAINT document_id_unique IF NOT EXISTS "
            "FOR (d:Document) REQUIRE d.id IS UNIQUE"
        )

        # Provision the KNN index. The third argument is the vector
        # dimension; Kùzu's per-table column projection is replaced by
        # a label + property pair.
        await client.execute_cypher(
            "CALL db.knn.create('Document', 'embedding', 768, "
            "{ M: 16, efConstruction: 200, efSearch: 50 }) YIELD name"
        )

        # ... corpus loading + embedding omitted ...

        query_vec: list[float] = [0.0] * 768

        seeds = await client.execute_cypher(
            "CALL db.knn.search('Document', 'embedding', $vec, 5) "
            "YIELD node, score "
            "RETURN node.id AS id, node.title AS title, score "
            "ORDER BY score DESC",
            {"vec": query_vec},
        )

        seed_ids = [row[0] for row in seeds.rows]

        expanded = await client.execute_cypher(
            "MATCH (seed:Document)-[:NEXT*1..2]-(neighbour:Document) "
            "WHERE seed.id IN $ids "
            "RETURN DISTINCT neighbour.title AS title, neighbour.body AS body",
            {"ids": seed_ids},
        )

        for title, body in expanded.rows:
            print(title, "—", body[:120], "…")


if __name__ == "__main__":
    asyncio.run(main())
