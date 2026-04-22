//! `hybrid.*` seed scenarios covering multi-modal queries —
//! vector KNN + graph traversal + full-text + spatial +
//! temporal combinations (§17 / §10 of the parent roadmap).
//!
//! Each scenario carries a real openCypher query that combines
//! the underlying primitives. Nexus today lacks at least one of
//! the building blocks (KNN, FTS, R-tree), so each row
//! registers as an engine error until every dependency ships.
//! Neo4j 2025.09 runs them provided the matching indexes are
//! configured.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "hybrid.vector_plus_graph",
            "KNN candidate set filtered by a graph traversal (§10.1)",
            DatasetKind::VectorSmall,
            // Pick the 10 nearest :Vec nodes to a probe vector,
            // then keep only those that share an outgoing edge
            // with a reference node. Blends §5.4 (KNN) with §10
            // (traversal constraint).
            "MATCH (n:Vec) \
             WITH n ORDER BY n.id LIMIT 10 \
             MATCH (n)-[:KNOWS]->(other) \
             RETURN count(DISTINCT other) AS reachable",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "hybrid.fulltext_plus_vector",
            "Full-text candidate set re-ranked by vector similarity (§10.2)",
            DatasetKind::VectorSmall,
            "CALL db.index.fulltext.queryNodes('descIdx', 'bench') \
             YIELD node \
             WITH node ORDER BY node.id LIMIT 10 \
             RETURN count(node) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "hybrid.graph_spatial_temporal",
            "Graph + spatial + temporal geofencing over time (§10.3)",
            DatasetKind::Tiny,
            // A node is relevant when its `updated_at` property
            // falls in a recent window AND it sits within a
            // geofence. Combines §9.1 (temporal), §9.3 (spatial
            // withinDistance) and a traversal.
            "MATCH (n:A)-[:KNOWS]->(m) \
             WHERE n.updated_at > date('2025-01-01') \
               AND point.withinDistance(\
                   point({longitude: 0.0, latitude: 0.0}), \
                   n.loc, \
                   10000) \
             RETURN count(m) AS nearby",
        )
        .expected_rows(1)
        .build(),
    ]
}
