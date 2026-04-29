"""Hybrid graph + vector + FTS knowledge graph (Nexus port).

Same shape as `kuzu_before.py`. The notable changes are concentrated
in the index DDL and the FTS / KNN procedure surface:

* `CREATE_HNSW_INDEX` → `db.knn.create`.
* `CREATE_FTS_INDEX`  → `db.index.fulltext.createNodeIndex`
  (Neo4j-compatible).
* `QUERY_FTS_INDEX`  → `db.index.fulltext.queryNodes`.
* `QUERY_VECTOR_INDEX` → `db.knn.search` (returns `score`,
  similarity, not `distance`).
* `[*SHORTEST 1..6]` → `shortestPath((...)-[*1..6]-...)`.
"""

from __future__ import annotations

import asyncio

from nexus_sdk import NexusClient


async def main() -> None:
    async with NexusClient("nexus://localhost:15475") as client:
        await client.execute_cypher(
            "CREATE CONSTRAINT paper_id_unique IF NOT EXISTS "
            "FOR (p:Paper) REQUIRE p.id IS UNIQUE"
        )
        await client.execute_cypher(
            "CALL db.knn.create('Paper', 'embedding', 768, "
            "{ M: 32, efConstruction: 400, efSearch: 100 }) YIELD name"
        )
        await client.execute_cypher(
            "CALL db.index.fulltext.createNodeIndex("
            "'paper_fts', ['Paper'], ['title', 'abstract'])"
        )

        query_vec: list[float] = [0.0] * 768

        fts_hits = await client.execute_cypher(
            "CALL db.index.fulltext.queryNodes('paper_fts', 'graph databases') "
            "YIELD node, score "
            "RETURN node.id AS id, node.title AS title, score "
            "ORDER BY score DESC LIMIT 50"
        )

        knn_hits = await client.execute_cypher(
            "CALL db.knn.search('Paper', 'embedding', $vec, 50) "
            "YIELD node, score "
            "RETURN node.id AS id, node.title AS title, score "
            "ORDER BY score DESC",
            {"vec": query_vec},
        )

        influence = await client.execute_cypher(
            "MATCH (a:Paper { id: $a }), (b:Paper { id: $b }), "
            "      p = shortestPath((a)-[:CITES*1..6]->(b)) "
            "RETURN p",
            {"a": 1, "b": 2},
        )

        print(fts_hits.rows[:5], knn_hits.rows[:5], influence.rows[:1], sep="\n---\n")


if __name__ == "__main__":
    asyncio.run(main())
