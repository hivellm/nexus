//! `point_read.*` seed scenarios — single-node lookups.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
    ]
}
