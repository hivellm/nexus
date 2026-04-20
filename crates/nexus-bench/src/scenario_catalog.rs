//! Seed scenario catalogue. Pure static data — no queries fire at
//! build time. The catalogue grows as Phase 6 features land (QPP,
//! FTS, APOC, …); each one appends its own scenarios here or in a
//! sibling module.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

/// Built-in seed scenarios, targeting the 100-node [`crate::TinyDataset`].
#[must_use]
pub fn seed_scenarios() -> Vec<Scenario> {
    vec![
        // --- Scalar / evaluator fast path -------------------------
        ScenarioBuilder::new(
            "scalar.literal_int",
            "RETURN a literal integer",
            DatasetKind::Tiny,
            "RETURN 1 AS n",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.arithmetic",
            "1 + 2 * 3",
            DatasetKind::Tiny,
            "RETURN 1 + 2 * 3 AS n",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.to_upper",
            "toUpper on a literal string",
            DatasetKind::Tiny,
            "RETURN toUpper('hello') AS s",
        )
        .expected_rows(1)
        .build(),
        // --- Point reads over the tiny dataset -------------------
        ScenarioBuilder::new(
            "point_read.by_id",
            "MATCH node with id = 42",
            DatasetKind::Tiny,
            "MATCH (n {id: 42}) RETURN n.name AS name",
        )
        .expected_rows(1)
        .build(),
        // --- Label scans ------------------------------------------
        ScenarioBuilder::new(
            "label_scan.count_a",
            "COUNT of label A",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
        // --- Aggregations -----------------------------------------
        ScenarioBuilder::new(
            "aggregation.sum_score",
            "SUM over node.score",
            DatasetKind::Tiny,
            "MATCH (n) RETURN sum(n.score) AS s",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "aggregation.avg_score_a",
            "AVG score restricted to label A",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN avg(n.score) AS s",
        )
        .expected_rows(1)
        .build(),
        // --- Filter + projection ---------------------------------
        ScenarioBuilder::new(
            "filter.score_gt_half",
            "count nodes with score > 0.5",
            DatasetKind::Tiny,
            "MATCH (n) WHERE n.score > 0.5 RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
        // --- ORDER BY / LIMIT -------------------------------------
        ScenarioBuilder::new(
            "order.top_5_by_score",
            "top 5 nodes by score",
            DatasetKind::Tiny,
            "MATCH (n) RETURN n.name AS name ORDER BY n.score DESC LIMIT 5",
        )
        .expected_rows(5)
        .build(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_non_empty() {
        assert!(seed_scenarios().len() >= 5);
    }

    #[test]
    fn ids_unique() {
        let scenarios = seed_scenarios();
        let ids: HashSet<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids.len(), scenarios.len(), "scenario ids must be unique");
    }

    #[test]
    fn every_scenario_targets_tiny_dataset() {
        for s in seed_scenarios() {
            assert_eq!(
                s.dataset,
                DatasetKind::Tiny,
                "{} uses non-tiny dataset",
                s.id
            );
        }
    }

    #[test]
    fn every_scenario_declares_row_count() {
        for s in seed_scenarios() {
            assert!(s.expected_row_count > 0, "{}: expected_row_count = 0", s.id);
        }
    }

    #[test]
    fn every_id_has_category_prefix() {
        for s in seed_scenarios() {
            assert!(
                s.id.contains('.'),
                "{} must use `category.name` id form",
                s.id
            );
        }
    }
}
