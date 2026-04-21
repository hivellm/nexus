//! Seed scenario catalogue — thin aggregator over the submodules
//! under [`crate::scenarios`].
//!
//! The catalogue used to live inline in this file until it grew
//! past 600 lines; now every category (`aggregation`, `scalar`,
//! `traversal`, `write`, …) owns its own file under
//! `src/scenarios/`, and this module just calls the private
//! `all()` aggregator and re-exports it.
//!
//! Callers outside the crate keep importing `seed_scenarios`
//! through this module so the file split stays an internal
//! organisation change.

use crate::scenario::Scenario;

/// Built-in seed scenarios across every category shipped today.
/// The dataset each scenario targets is declared on the
/// `Scenario` struct itself; the CLI + integration tests iterate
/// the dataset kinds via a `HashSet` and load each one once.
#[must_use]
pub fn seed_scenarios() -> Vec<Scenario> {
    crate::scenarios::all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataset::DatasetKind;
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
                matches!(
                    s.dataset,
                    DatasetKind::Tiny | DatasetKind::Small | DatasetKind::VectorSmall
                ),
                "{} uses unknown dataset {:?}",
                s.id,
                s.dataset
            );
        }
    }

    #[test]
    fn small_dataset_scenarios_present() {
        // Quick guard that the SmallDataset traversal block lands
        // and does not regress if someone reorganises the catalogue.
        let scenarios = seed_scenarios();
        let ids: HashSet<&str> = scenarios
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
            "constraint",
            "ecosystem",
            "filter",
            "hybrid",
            "index",
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
    fn write_scenarios_declare_mutation_prefix() {
        // Writes now run — `BenchClient::reset()` shipped in
        // phase6_bench-live-test-state-isolation and the harness's
        // divergence guard catches a scenario whose per-iteration
        // row count drifts from its `expected_row_count`. What
        // remains is an author-intent marker: every mutating
        // scenario must live under a prefix that flags the intent
        // (today: `write.`, `constraint.`, or `ecosystem.` — the
        // last because CALL-in-transactions / APOC map-merge-style
        // queries can still CREATE under the hood).
        //
        // Anything outside those prefixes must stay pure-read so a
        // harness run without `--load-dataset` cannot surprise the
        // operator by mutating state.
        const MUTATION_PREFIXES: &[&str] = &[
            "write.",
            "constraint.",
            "ecosystem.",
            // phase6 advanced-types bench scenarios legitimately
            // issue `CREATE INDEX FOR (n:L) ON (...)` DDL — the same
            // "author-intent marker" principle applies.
            "advanced_types.",
        ];
        for s in seed_scenarios() {
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
            let is_declared_write = MUTATION_PREFIXES.iter().any(|p| s.id.starts_with(p));
            assert!(
                !has_write || is_declared_write,
                "{}: query contains a write clause but its id prefix \
                 is not in {MUTATION_PREFIXES:?}",
                s.id
            );
        }
    }
}
