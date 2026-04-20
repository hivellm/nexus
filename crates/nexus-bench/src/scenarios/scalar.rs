//! `scalar.*` seed scenarios — evaluator fast path + expressions
//! that never touch the store.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
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
        ScenarioBuilder::new(
            "scalar.string_concat",
            "string + string concatenation",
            DatasetKind::Tiny,
            "RETURN 'hello' + ' world' AS greet",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.list_indexing",
            "zero-indexed list access",
            DatasetKind::Tiny,
            "RETURN [10, 20, 30][1] AS second",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.case_simple",
            "CASE WHEN expression",
            DatasetKind::Tiny,
            "RETURN CASE WHEN 1 > 0 THEN 'yes' ELSE 'no' END AS verdict",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.unwind_range_count",
            "UNWIND range(1, 10) + count — basic iterator",
            DatasetKind::Tiny,
            "UNWIND range(1, 10) AS x RETURN count(x) AS c",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.list_reverse",
            "reverse() of a literal list",
            DatasetKind::Tiny,
            "RETURN reverse([1, 2, 3, 4, 5]) AS rev",
        )
        .expected_rows(1)
        .build(),
    ]
}
