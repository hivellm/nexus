//! Seed scenario catalogue — the first ~12 scenarios to anchor the
//! harness. Built via [`ScenarioBuilder`] so each entry reads top-
//! down. The list grows as Phase 6 tasks (constraints, FTS, APOC,
//! QPP, …) ship — each feature lands its own scenarios in this file
//! or a sibling module.

use std::time::Duration;

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

/// Built-in scenarios. Ordered by id so diffs are readable.
#[must_use]
pub fn seed_scenarios() -> Vec<Scenario> {
    vec![
        // Scalar / literal RETURN — the cheapest possible scenario.
        // Pins the wire + parse + return cost floor.
        ScenarioBuilder::new(
            "scalar.literal_int",
            "RETURN a literal integer",
            DatasetKind::Micro,
            "RETURN 1 AS n",
        )
        .expected_rows(1)
        .build(),
        // Arithmetic over literals — exercises the evaluator's fast
        // path for constant folding (if any).
        ScenarioBuilder::new(
            "scalar.arithmetic",
            "1 + 2 * 3",
            DatasetKind::Micro,
            "RETURN 1 + 2 * 3 AS n",
        )
        .expected_rows(1)
        .build(),
        // String function.
        ScenarioBuilder::new(
            "scalar.to_upper",
            "toUpper on a literal string",
            DatasetKind::Micro,
            "RETURN toUpper('hello') AS s",
        )
        .expected_rows(1)
        .build(),
        // Type-check predicate.
        ScenarioBuilder::new(
            "scalar.type_check",
            "type check an int literal",
            DatasetKind::Micro,
            "RETURN 42 IS :: INTEGER AS b",
        )
        .expected_rows(1)
        .timeout(Duration::from_secs(5))
        .build(),
        // Point read by indexed property. The micro dataset gives
        // 10k nodes with unique `id: Int` across the full range.
        ScenarioBuilder::new(
            "point_read.by_id",
            "MATCH node with id = 500",
            DatasetKind::Micro,
            "MATCH (n {id: 500}) RETURN n.name AS name",
        )
        .expected_rows(1)
        .warmup(3)
        .measured(20)
        .build(),
        // Label scan — degenerates into an index scan when the
        // label index is populated.
        ScenarioBuilder::new(
            "label_scan.count_a",
            "COUNT of label A",
            DatasetKind::Micro,
            "MATCH (n:A) RETURN count(n) AS c",
        )
        .expected_rows(1)
        .measured(10)
        .build(),
        // Aggregation: SUM over the micro dataset's score column.
        ScenarioBuilder::new(
            "aggregation.sum_score",
            "SUM of all nodes' score",
            DatasetKind::Micro,
            "MATCH (n) RETURN sum(n.score) AS s",
        )
        .expected_rows(1)
        .measured(10)
        .build(),
        // AVG.
        ScenarioBuilder::new(
            "aggregation.avg_score_a",
            "AVG score restricted to label A",
            DatasetKind::Micro,
            "MATCH (n:A) RETURN avg(n.score) AS s",
        )
        .expected_rows(1)
        .measured(10)
        .build(),
        // 1-hop traversal.
        ScenarioBuilder::new(
            "traversal.one_hop",
            "count of 1-hop KNOWS neighbours of node 0",
            DatasetKind::Micro,
            "MATCH (a {id: 0})-[:KNOWS]->(b) RETURN count(b) AS c",
        )
        .expected_rows(1)
        .warmup(3)
        .measured(15)
        .build(),
        // 2-hop traversal — exercises the expand pipeline.
        ScenarioBuilder::new(
            "traversal.two_hop_fof",
            "count of distinct 2-hop KNOWS friends",
            DatasetKind::Micro,
            "MATCH (a {id: 0})-[:KNOWS]->()-[:KNOWS]->(b) RETURN count(DISTINCT b) AS c",
        )
        .expected_rows(1)
        .warmup(3)
        .measured(10)
        .build(),
        // WHERE filter + projection.
        ScenarioBuilder::new(
            "filter.score_gt_half",
            "nodes with score > 0.5, count only",
            DatasetKind::Micro,
            "MATCH (n) WHERE n.score > 0.5 RETURN count(n) AS c",
        )
        .expected_rows(1)
        .measured(10)
        .build(),
        // ORDER BY + LIMIT — exercises the top-k path.
        ScenarioBuilder::new(
            "order.top_10_by_score",
            "top 10 nodes by score",
            DatasetKind::Micro,
            "MATCH (n) RETURN n.name AS name ORDER BY n.score DESC LIMIT 10",
        )
        .expected_rows(10)
        .measured(10)
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_catalog_is_non_empty() {
        let s = seed_scenarios();
        assert!(s.len() >= 10);
    }

    #[test]
    fn every_seed_has_unique_id() {
        let s = seed_scenarios();
        let mut ids: Vec<&str> = s.iter().map(|x| x.id.as_str()).collect();
        ids.sort();
        let len = ids.len();
        ids.dedup();
        assert_eq!(len, ids.len(), "scenario ids must be unique");
    }

    #[test]
    fn every_seed_has_plausible_iteration_counts() {
        for scen in seed_scenarios() {
            assert!(scen.measured_iters > 0, "{}: 0 measured", scen.id);
            assert!(scen.warmup_iters > 0, "{}: 0 warmup", scen.id);
            assert!(
                scen.timeout > Duration::from_millis(100),
                "{}: tiny timeout",
                scen.id
            );
        }
    }

    #[test]
    fn every_seed_declares_expected_rows() {
        for scen in seed_scenarios() {
            // 0 is only valid for scenarios that return no rows —
            // we don't have any of those in the seed list.
            assert!(scen.expected_row_count > 0, "{}: expected 0 rows", scen.id);
        }
    }

    #[test]
    fn every_seed_has_category_prefix() {
        for scen in seed_scenarios() {
            assert!(
                scen.id.contains('.'),
                "{} must use `category.name` id form",
                scen.id
            );
        }
    }
}
