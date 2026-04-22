//! `label_scan.*` seed scenarios.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
    ]
}
