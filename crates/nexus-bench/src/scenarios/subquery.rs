//! `subquery.*` seed scenarios — WITH pipelines + subquery
//! expressions.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
            "subquery.count_subquery",
            "COUNT { } subquery predicate (Cypher 5)",
            DatasetKind::Tiny,
            "MATCH (n:A) RETURN COUNT { MATCH (n)-[:KNOWS]->() } AS deg",
        )
        .expected_rows(20)
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
    ]
}
