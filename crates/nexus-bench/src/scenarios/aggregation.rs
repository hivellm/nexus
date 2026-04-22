//! `aggregation.*` seed scenarios.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
        ScenarioBuilder::new(
            "aggregation.stdev_score",
            "stdev over node.score on label A",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN stdev(n.score) AS sd",
        )
        .expected_rows(1)
        .build(),
    ]
}
