//! `filter.*` seed scenarios — WHERE predicates.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
        ScenarioBuilder::new(
            "filter.label_and_id",
            "label + id range filter combo on A",
            DatasetKind::Tiny,
            "MATCH (n:A) WHERE n.id > 5 AND n.id < 15 RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "filter.composite_prefix_candidate",
            "composite (id, score) filter — candidate for a composite \
             B-tree index (§5.3); works today with full scan, gets \
             fast once the index lands",
            DatasetKind::Tiny,
            "MATCH (n:A) WHERE n.id > 5 AND n.id < 15 AND n.score > 0.08 \
             RETURN count(n) AS c",
        )
        .expected_rows(1)
        .build(),
    ]
}
