//! `order.*` seed scenarios — ORDER BY / LIMIT.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
    ]
}
