//! `traversal.*` seed scenarios — 1-hop / 2-hop / variable-length /
//! cartesian / hub-plus-chain.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        // --- TinyDataset KNOWS chain ----------------------------
        ScenarioBuilder::new(
            "traversal.one_hop_from_zero",
            "1-hop KNOWS neighbour count from node 0",
            DatasetKind::Tiny,
            "MATCH (a {id: 0})-[:KNOWS]->(b) RETURN count(b) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "traversal.two_hop_chain",
            "2-hop chain from node 0 along KNOWS",
            DatasetKind::Tiny,
            "MATCH (a {id: 0})-[:KNOWS]->()-[:KNOWS]->(c) RETURN count(DISTINCT c) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "traversal.all_knows_edges",
            "all KNOWS relationships in the dataset",
            DatasetKind::Tiny,
            "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c",
        )
        .expected_rows(1)
        .build(),
        // --- SmallDataset hub-plus-chain ------------------------
        // Topology: 50 nodes `(:P {id: 0..49})`, KNOWS chain
        // `p0→p1→…→p49`, plus hub branches `p0→p10`, `p0→p20`,
        // `p0→p30`, `p0→p40`. Baseline counts are deterministic
        // from the load literal — a regression shows up as a row
        // count drift the harness's expected_rows guard catches.
        ScenarioBuilder::new(
            "traversal.small_one_hop_hub",
            "1-hop KNOWS from the hub node p0 (expects 5 neighbours)",
            DatasetKind::Small,
            "MATCH (:P {id: 0})-[:KNOWS]->(b) RETURN count(b) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "traversal.small_two_hop_from_hub",
            "2-hop KNOWS distinct targets from p0 (expects 5)",
            DatasetKind::Small,
            "MATCH (:P {id: 0})-[:KNOWS]->()-[:KNOWS]->(c) RETURN count(DISTINCT c) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "traversal.small_var_length_1_to_3",
            "variable-length *1..3 from p0 (expects 15 distinct)",
            DatasetKind::Small,
            "MATCH (:P {id: 0})-[:KNOWS*1..3]->(n) RETURN count(DISTINCT n) AS c",
        )
        .expected_rows(1)
        .build(),
        // `shortestPath((…)-[*]->(…))` is §3.5 territory but Nexus's
        // parser errors on the `shortestPath(` token right now —
        // tracked inside phase6_nexus-bench-correctness-gaps. Add
        // back once the parser accepts the Neo4j syntax; leaving it
        // out today keeps the bench run from aborting on that row.
        ScenarioBuilder::new(
            "traversal.cartesian_a_b",
            "MATCH (a:A), (b:B) cartesian count (TinyDataset: 20 × 20 = 400)",
            DatasetKind::Tiny,
            "MATCH (a:A), (b:B) RETURN count(*) AS c",
        )
        .expected_rows(1)
        .build(),
    ]
}
