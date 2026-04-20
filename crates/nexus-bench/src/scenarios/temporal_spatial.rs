//! `scalar.*` seed scenarios scoped to temporal + spatial
//! built-ins. Kept under the `scalar.` id prefix (not `temporal.`
//! / `spatial.`) because they still never touch the store and
//! sit on the evaluator's fast path — the category prefix tracks
//! "does this read the graph", not the namespace of the function
//! being exercised.

use crate::dataset::DatasetKind;
use crate::scenario::{Scenario, ScenarioBuilder};

pub(crate) fn scenarios() -> Vec<Scenario> {
    vec![
        ScenarioBuilder::new(
            "scalar.date_literal",
            "build a Date from a map literal",
            DatasetKind::Tiny,
            "RETURN date({year: 2026, month: 4, day: 20}) AS d",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.duration_between_days",
            "duration.between().days between two literal dates",
            DatasetKind::Tiny,
            "RETURN duration.between(date('2025-01-01'), \
             date('2026-04-20')).days AS days",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.point_distance_cartesian",
            "point.distance on a 2-D Cartesian triangle (expects 5.0)",
            DatasetKind::Tiny,
            "RETURN point.distance(point({x: 0, y: 0}), \
             point({x: 3, y: 4})) AS d",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.point_distance_wgs84",
            "point.distance across a WGS-84 point pair",
            DatasetKind::Tiny,
            "RETURN point.distance(\
             point({longitude: 0.0, latitude: 0.0}), \
             point({longitude: 1.0, latitude: 1.0})) AS d",
        )
        .expected_rows(1)
        .build(),
        ScenarioBuilder::new(
            "scalar.point_within_distance",
            "point.withinDistance predicate — §9.3 / index.rtree_*",
            DatasetKind::Tiny,
            // Standalone scalar form of `withinDistance`. The
            // R-tree-backed form over property-indexed points
            // lives in `index.rtree_within_distance`; this row
            // measures the pure function-call cost.
            "RETURN point.withinDistance(\
             point({longitude: 0.0, latitude: 0.0}), \
             point({longitude: 0.01, latitude: 0.01}), \
             5000) AS inside",
        )
        .expected_rows(1)
        .build(),
    ]
}
