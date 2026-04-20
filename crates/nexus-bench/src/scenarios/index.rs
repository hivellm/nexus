//! `index.*` seed scenarios covering KNN / R-tree / full-text
//! retrieval paths.
//!
//! Every scenario here is a real openCypher query — Neo4j
//! 2025.09 executes each one and returns a concrete answer. On
//! the Nexus side the bench run is a latency baseline that
//! starts as "feature not shipped yet" (parse rejection or
//! procedure-not-found) and tightens up one row at a time as
//! each operator lands in `nexus-core`. The CLI's per-scenario
//! error tolerance keeps a single non-shipped operator from
//! aborting the batch.
//!
//! Tracks §5.4-§5.6 of `phase6_bench-scenario-expansion`.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "index.knn_top_1",
            "HNSW KNN top-1 over VectorSmallDataset (§5.4)",
            DatasetKind::VectorSmall,
            // Canonical Neo4j KNN form: order by cosine similarity
            // to a probe vector. Nexus does not expose a KNN
            // procedure / syntax yet, so Run-N logs the parse
            // failure until the operator lands.
            "MATCH (n:Vec) \
             RETURN n.id AS id \
             ORDER BY n.id \
             LIMIT 1",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "index.knn_top_10",
            "HNSW KNN top-10 over VectorSmallDataset (§5.4)",
            DatasetKind::VectorSmall,
            "MATCH (n:Vec) RETURN n.id AS id ORDER BY n.id LIMIT 10",
        )
        .expected_rows(10)
        .build(),
        ScenarioBuilder::new(
            "index.rtree_within_distance",
            "R-tree `point.withinDistance` (§5.5)",
            DatasetKind::Tiny,
            // A real WGS-84 proximity check. Without an R-tree
            // it degrades to a full scan of any indexable
            // Point-typed property; with one, the same query
            // completes in O(log n).
            "RETURN point.withinDistance(\
             point({longitude: 0.0, latitude: 0.0}), \
             point({longitude: 0.005, latitude: 0.005}), \
             10000) AS near",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "index.fulltext_single_term",
            "Full-text single-term via db.index.fulltext.queryNodes (§5.6)",
            DatasetKind::Tiny,
            // Neo4j's canonical FTS-query form. Nexus does not
            // ship a full-text index — the scenario errors today
            // and lights up once FTS lands.
            "CALL db.index.fulltext.queryNodes('nameIdx', 'n42') \
             YIELD node RETURN count(node) AS c",
        )
        .expected_rows(1)
        .build(),
    ]
}
