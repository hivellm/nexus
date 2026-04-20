//! Seed scenario catalogue. Pure static data — no queries fire at
//! build time. The catalogue grows as Phase 6 features land (QPP,
//! FTS, APOC, …); each one appends its own scenarios here or in a
//! sibling module.
//!
//! Ordering: entries are grouped by category (`scalar`, `point_read`,
//! `label_scan`, `aggregation`, `filter`, `order`, `traversal`,
//! `subquery`, `procedure`) and alphabetised within each category so
//! diffs that add a scenario land next to the others in its family
//! instead of drifting to the tail of the list.
//!
//! # What is *not* here
//!
//! Writes (CREATE/MERGE/SET/DELETE) are deliberately omitted until
//! the harness grows a per-iteration reset hook — without it, a
//! write scenario would mutate the dataset between iterations and
//! break the divergence guard on the second call. Tracked in the
//! parent task's §11 and in the companion task
//! `phase6_bench-scenario-expansion`.
//!
//! Index / constraint / full-text / geospatial / hybrid scenarios
//! are also out of scope here until the underlying Phase 6 features
//! (B-tree composite, constraint enforcement, FTS, R-tree, vector
//! indexes) ship in nexus-core; see the same follow-up task.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

/// Built-in seed scenarios, targeting the 100-node [`crate::TinyDataset`].
#[must_use]
pub fn seed_scenarios() -> Vec<Scenario> {
    vec![
        // --- Aggregations ----------------------------------------
        ScenarioBuilder::new(
            "aggregation.avg_score_a",
            "AVG score restricted to label A",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN avg(n.score) AS s",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "aggregation.count_all",
            "COUNT of all nodes",
            DatasetKind::Tiny,
            "MATCH (n) RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "aggregation.min_max_score",
            "MIN and MAX score over the dataset",
            DatasetKind::Tiny,
            "MATCH (n) RETURN min(n.score) AS lo, max(n.score) AS hi",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "aggregation.sum_score",
            "SUM over node.score",
            DatasetKind::Tiny,
            "MATCH (n) RETURN sum(n.score) AS s",
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
        ScenarioBuilder::new(
            "filter.score_range",
            "count nodes whose score is in [0.25, 0.75]",
            DatasetKind::Tiny,
            "MATCH (n) WHERE n.score >= 0.25 AND n.score <= 0.75 RETURN count(n) AS c",
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
        ScenarioBuilder::new(
            "label_scan.count_e_with_filter",
            "COUNT of label E with score filter",
            DatasetKind::Tiny,
            "MATCH (n:E) WHERE n.score > 0.9 RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
        // --- ORDER BY / LIMIT -------------------------------------
        ScenarioBuilder::new(
            "order.bottom_5_by_score",
            "bottom 5 nodes by score (ASC order)",
            DatasetKind::Tiny,
            "MATCH (n) RETURN n.name AS name ORDER BY n.score ASC LIMIT 5",
        )
        .expected_rows(5)
        .build(),
        ScenarioBuilder::new(
            "order.top_5_by_score",
            "top 5 nodes by score",
            DatasetKind::Tiny,
            "MATCH (n) RETURN n.name AS name ORDER BY n.score DESC LIMIT 5",
        )
        .expected_rows(5)
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
        ScenarioBuilder::new(
            "point_read.by_name",
            "MATCH node with name = 'n99'",
            DatasetKind::Tiny,
            "MATCH (n {name: 'n99'}) RETURN n.id AS id",
        )
        .expected_rows(1)
        .build(),
        // --- Procedures (catalog introspection, read-only) -------
        ScenarioBuilder::new(
            "procedure.db_labels",
            "db.labels procedure",
            DatasetKind::Tiny,
            "CALL db.labels() YIELD label RETURN count(label) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.db_relationship_types",
            "db.relationshipTypes procedure",
            DatasetKind::Tiny,
            "CALL db.relationshipTypes() YIELD relationshipType RETURN count(relationshipType) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "procedure.db_property_keys",
            "db.propertyKeys procedure",
            DatasetKind::Tiny,
            "CALL db.propertyKeys() YIELD propertyKey RETURN count(propertyKey) AS c",
        )
        .expected_rows(1)
        .build(),
        // --- Scalar / evaluator fast path ------------------------
        ScenarioBuilder::new(
            "scalar.arithmetic",
            "1 + 2 * 3",
            DatasetKind::Tiny,
            "RETURN 1 + 2 * 3 AS n",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.coalesce",
            "coalesce over mixed null literal",
            DatasetKind::Tiny,
            "RETURN coalesce(null, null, 42) AS n",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.literal_int",
            "RETURN a literal integer",
            DatasetKind::Tiny,
            "RETURN 1 AS n",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.string_length",
            "size() of a literal string",
            DatasetKind::Tiny,
            "RETURN size('benchmark') AS n",
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
        // --- Subqueries ------------------------------------------
        ScenarioBuilder::new(
            "subquery.collect_names",
            "COLLECT subquery — names of label A",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN collect(n.name) AS names",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "subquery.exists_high_score",
            "EXISTS — is there any node with score > 0.99",
            DatasetKind::Tiny,
            "MATCH (n) WITH count(n) AS total, max(n.score) AS hi RETURN hi > 0.99 AS any_high",
        )
        .expected_rows(1)
        .build(),
        // --- Traversals (KNOWS chain in TinyDataset) -------------
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_non_empty() {
        assert!(seed_scenarios().len() >= 15);
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

    #[test]
    fn expected_category_coverage() {
        let scenarios = seed_scenarios();
        let prefixes: HashSet<&str> = scenarios
            .iter()
            .map(|s| s.id.split('.').next().unwrap_or(""))
            .collect();
        // Each of these categories must be exercised by at least one
        // scenario so a future edit that accidentally drops a whole
        // family trips this test.
        for cat in [
            "aggregation",
            "filter",
            "label_scan",
            "order",
            "point_read",
            "procedure",
            "scalar",
            "subquery",
            "traversal",
        ] {
            assert!(
                prefixes.contains(cat),
                "category `{cat}` has no scenario in the seed catalogue"
            );
        }
    }

    #[test]
    fn every_query_avoids_write_clauses() {
        // Writes are deliberately out of scope until the harness
        // grows a per-iteration reset hook. Catch a regression at
        // the seed-catalogue level rather than at run-time when a
        // second iteration's CREATE fails the divergence guard.
        for s in seed_scenarios() {
            let q = s.query.to_uppercase();
            for forbidden in [
                " CREATE ",
                " MERGE ",
                " SET ",
                " DELETE ",
                " REMOVE ",
                " DETACH DELETE ",
                " FOREACH ",
            ] {
                // Guard against false positives on procedures that
                // contain the word inside a name, e.g. "DELETE" inside
                // "DETACH DELETE" — the space-padded match above
                // already avoids that.
                assert!(
                    !q.contains(forbidden),
                    "{}: query contains forbidden write clause `{}`",
                    s.id,
                    forbidden.trim()
                );
            }
        }
    }
}
