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
            "MATCH node on label C with id = 42",
            DatasetKind::Tiny,
            // Label-scoped so the scenario still returns exactly 1
            // row when SmallDataset is loaded alongside — both
            // fixtures use an `id` property in the 0..49 range and
            // the harness must not see 2 matches. `n42` lives under
            // `:C` per `TinyDataset::load_statement`.
            "MATCH (n:C {id: 42}) RETURN n.name AS name",
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
        ScenarioBuilder::new(
            "procedure.db_indexes",
            "db.indexes procedure — catalogue of indexes",
            DatasetKind::Tiny,
            "CALL db.indexes() YIELD * RETURN count(*) AS c",
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
            "subquery.unwind_sum",
            "UNWIND + sum over a literal list (no graph read)",
            DatasetKind::Tiny,
            "UNWIND [1, 2, 3, 4, 5] AS x RETURN sum(x) AS s",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "subquery.with_filter_count",
            "MATCH → WITH → WHERE → RETURN pipeline",
            DatasetKind::Tiny,
            "MATCH (n:A) WITH n.score AS s WHERE s > 0.1 RETURN count(*) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "subquery.size_of_collect",
            "size() over a collected list",
            DatasetKind::Tiny,
            "MATCH (n:A) WITH collect(n.id) AS ids RETURN size(ids) AS s",
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
        // --- Traversals on SmallDataset (hub-plus-chain) ---------
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
        // --- Writes (idempotent or iteration-safe queries) -------
        // Every scenario in this group either CREATE-s a node per
        // iteration (count=1 each time) or MERGE-s/SET-s where the
        // result shape is stable across iterations. Non-stable
        // writes would trip the harness's divergence guard on
        // iteration 2. BenchClient::reset() is not called between
        // iterations — that would dominate latency.
        ScenarioBuilder::new(
            "write.create_singleton",
            "CREATE a new :BenchTemp node and return a literal mark",
            DatasetKind::Tiny,
            // Return a literal instead of `id(n)` — Nexus and Neo4j
            // allocate node ids independently, so the divergence
            // guard would otherwise flag this row on every run
            // even though both engines did the same work.
            "CREATE (n:BenchTemp {mark: 'bench'}) RETURN n.mark AS mark",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.merge_singleton",
            "MERGE a singleton :BenchSingleton — idempotent",
            DatasetKind::Tiny,
            "MERGE (n:BenchSingleton {key: 'bench'}) RETURN n.key AS k",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "write.set_property",
            "SET n.bench_visited = true on n0:A — idempotent",
            DatasetKind::Tiny,
            "MATCH (n:A {id: 0}) SET n.bench_visited = true RETURN n.id AS id",
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
    fn every_scenario_targets_a_known_dataset() {
        // Whitelist widens as new datasets land. Anything outside
        // this set is a typo or a stale reference to a fixture
        // that was renamed.
        for s in seed_scenarios() {
            assert!(
                matches!(s.dataset, DatasetKind::Tiny | DatasetKind::Small),
                "{} uses unknown dataset {:?}",
                s.id,
                s.dataset
            );
        }
    }

    #[test]
    fn small_dataset_scenarios_present() {
        // A quick guard that the SmallDataset traversal block lands
        // and does not regress if someone reorganises the catalogue.
        let scenarios = seed_scenarios();
        let ids: std::collections::HashSet<&str> = scenarios
            .iter()
            .filter(|s| matches!(s.dataset, DatasetKind::Small))
            .map(|s| s.id.as_str())
            .collect();
        for expected in [
            "traversal.small_one_hop_hub",
            "traversal.small_two_hop_from_hub",
            "traversal.small_var_length_1_to_3",
        ] {
            assert!(
                ids.contains(expected),
                "missing SmallDataset scenario {expected}"
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
            "write",
        ] {
            assert!(
                prefixes.contains(cat),
                "category `{cat}` has no scenario in the seed catalogue"
            );
        }
    }

    #[test]
    fn write_scenarios_declare_write_prefix() {
        // Writes now run — `BenchClient::reset()` shipped in
        // phase6_bench-live-test-state-isolation and the harness's
        // divergence guard catches a scenario whose per-iteration
        // row count drifts from its `expected_row_count`. What
        // remains is an author-intent marker: every write scenario
        // must sit under the `write.` id prefix, and every
        // non-`write.` scenario must stay pure-read. That way the
        // prefix alone tells the operator whether a run will mutate
        // state.
        for s in seed_scenarios() {
            // Pad with spaces so a keyword at the start or end of
            // the query still matches the space-delimited search
            // pattern — a query that opens with `CREATE (n...` would
            // otherwise slip past ` CREATE `.
            let q = format!(" {} ", s.query.to_uppercase());
            let has_write = [
                " CREATE ",
                " MERGE ",
                " SET ",
                " DELETE ",
                " REMOVE ",
                " DETACH DELETE ",
                " FOREACH ",
            ]
            .iter()
            .any(|w| q.contains(w));
            let is_declared_write = s.id.starts_with("write.");
            assert_eq!(
                has_write, is_declared_write,
                "{}: write-clause presence ({has_write}) must match \
                 id prefix ({is_declared_write})",
                s.id
            );
        }
    }
}
