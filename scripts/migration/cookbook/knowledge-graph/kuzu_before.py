"""Hybrid graph + vector + FTS knowledge graph (Kùzu reference).

Domain: a research knowledge graph. Each `Paper` has a title, an
abstract, a vector embedding of the abstract, and `:CITES` edges to
other papers. Two query shapes:

1. **Hybrid retrieval**: full-text search the abstract for a query
   string, vector-search for semantic siblings, intersect the
   ranked lists, and project the citation neighbourhood for each
   surviving paper.
2. **Influence path**: shortest path between two papers through
   `:CITES`.
"""

from __future__ import annotations

import kuzu  # type: ignore[import-not-found]


def main() -> None:
    db = kuzu.Database("./kg.kz")
    conn = kuzu.Connection(db)

    conn.execute(
        """
        CREATE NODE TABLE IF NOT EXISTS Paper(
            id SERIAL PRIMARY KEY,
            title STRING,
            abstract STRING,
            embedding FLOAT[768]
        );
        CREATE REL TABLE IF NOT EXISTS Cites(FROM Paper TO Paper);
        """
    )
    conn.execute(
        """
        CALL CREATE_HNSW_INDEX('Paper', 'paper_embedding', 'embedding',
                               mu := 32, efc := 400);
        CALL CREATE_FTS_INDEX('Paper', 'paper_fts', ['title', 'abstract'],
                              stemmer := 'english');
        """
    )

    query_vec: list[float] = [0.0] * 768

    fts_hits = conn.execute(
        """
        CALL QUERY_FTS_INDEX('Paper', 'paper_fts', 'graph databases')
        YIELD node, score
        RETURN node.id AS id, node.title AS title, score
        ORDER BY score DESC
        LIMIT 50
        """
    ).get_as_df()

    knn_hits = conn.execute(
        """
        CALL QUERY_VECTOR_INDEX('Paper', 'paper_embedding', $vec, 50)
        YIELD node, distance
        RETURN node.id AS id, node.title AS title, distance
        ORDER BY distance ASC
        """,
        {"vec": query_vec},
    ).get_as_df()

    influence = conn.execute(
        """
        MATCH p = (a:Paper {id: $a})-[*SHORTEST 1..6]->(b:Paper {id: $b})
        RETURN p
        """,
        {"a": 1, "b": 2},
    ).get_as_df()

    print(fts_hits.head(), knn_hits.head(), influence.head(), sep="\n---\n")


if __name__ == "__main__":
    main()
