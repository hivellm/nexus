//! `ecosystem.*` seed scenarios covering the APOC / GDS surface
//! and the Cypher 5 `CALL { } IN TRANSACTIONS` clause.
//!
//! Each scenario drives a real Neo4j-recognised procedure or
//! clause. On Nexus the procedure registry does not carry
//! any of these entries yet, so Run-N logs a
//! "procedure not found" or parse error and moves on. The row
//! turns green the day Nexus registers the matching procedure.
//!
//! Tracks §7.5 (IN TRANSACTIONS) and §8.3-§8.6 of
//! `phase6_bench-scenario-expansion`.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "ecosystem.apoc_coll_sum",
            "apoc.coll.sum over a literal list (§8.3)",
            DatasetKind::Tiny,
            "RETURN apoc.coll.sum([1, 2, 3, 4, 5]) AS total",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "ecosystem.apoc_map_merge",
            "apoc.map.merge of two literal maps (§8.4)",
            DatasetKind::Tiny,
            "RETURN apoc.map.merge({a: 1, b: 2}, {b: 3, c: 4}) AS merged",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "ecosystem.apoc_path_expand",
            "apoc.path.expand from p0 — compare against native *1..3 (§8.5)",
            DatasetKind::Small,
            "MATCH (start:P {id: 0}) \
             CALL apoc.path.expand(start, 'KNOWS>', null, 1, 3) \
             YIELD path RETURN count(path) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "ecosystem.gds_pagerank",
            "gds.pageRank.stream on the KNOWS subgraph (§8.6)",
            DatasetKind::Tiny,
            "CALL gds.pageRank.stream({\
             nodeProjection: 'A', \
             relationshipProjection: 'KNOWS'}) \
             YIELD nodeId, score \
             RETURN count(score) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "ecosystem.call_in_transactions",
            "CALL { } IN TRANSACTIONS throughput — 10-batch (§7.5)",
            DatasetKind::Tiny,
            "UNWIND range(1, 10) AS i \
             CALL { WITH i MERGE (:BenchTx {i: i}) } \
             IN TRANSACTIONS OF 5 ROWS \
             RETURN count(*) AS c",
        )
        .expected_rows(1)
        .build(),
    ]
}
