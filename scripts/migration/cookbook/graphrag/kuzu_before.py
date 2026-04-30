"""GraphRAG retrieval against KuzuDB (the *before* picture).

Read alongside `nexus_after.py` — both implement the same retrieval:
chunk a small corpus, embed each chunk, store the embeddings on
`Document` nodes, link consecutive chunks with `:NEXT`, and answer a
query by combining vector search with one-hop traversal to surface
neighbouring chunks.

This file is the Kùzu reference. It is not executed by CI — the Kùzu
package is no longer maintained — but it stays here so the diff with
`nexus_after.py` is a single side-by-side read.
"""

from __future__ import annotations

# This script is illustrative — `kuzu` is not on the project's
# dependency list. Install it manually if you want to run the script
# end-to-end on an existing Kùzu database.
import kuzu  # type: ignore[import-not-found]


def main() -> None:
    db = kuzu.Database("./graphrag.kz")
    conn = kuzu.Connection(db)

    conn.execute("""
        CREATE NODE TABLE IF NOT EXISTS Document(
            id        SERIAL PRIMARY KEY,
            title     STRING,
            body      STRING,
            embedding FLOAT[768]
        );
        """)
    conn.execute("""
        CREATE REL TABLE IF NOT EXISTS Next(FROM Document TO Document);
        """)

    conn.execute("""
        CALL CREATE_HNSW_INDEX('Document', 'doc_embedding', 'embedding',
                               mu := 16, efc := 200);
        """)

    # ... corpus loading + embedding omitted ...

    query_vec: list[float] = [0.0] * 768  # produced by your embedder

    seeds = conn.execute(
        """
        CALL QUERY_VECTOR_INDEX('Document', 'doc_embedding', $vec, 5)
        YIELD node, distance
        RETURN node.id AS id, node.title AS title, distance
        ORDER BY distance ASC
        """,
        {"vec": query_vec},
    ).get_as_df()

    # Expand each seed to its immediate neighbours via :NEXT to give
    # the LLM context bracketing the seed chunk.
    expanded = conn.execute(
        """
        MATCH (seed:Document)-[:Next*1..2]-(neighbour:Document)
        WHERE seed.id IN $ids
        RETURN DISTINCT neighbour.title AS title, neighbour.body AS body
        """,
        {"ids": [int(row["id"]) for _, row in seeds.iterrows()]},
    ).get_as_df()

    print(expanded)


if __name__ == "__main__":
    main()
