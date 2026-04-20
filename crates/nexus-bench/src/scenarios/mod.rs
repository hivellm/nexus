//! Seed scenario submodules, grouped by the openCypher feature
//! area each one exercises.
//!
//! Every module here exports a private `scenarios() -> Vec<Scenario>`
//! function. The aggregator in [`crate::scenario_catalog::seed_scenarios`]
//! concatenates all of them; callers outside this crate should
//! continue to import through `scenario_catalog` so the file split
//! stays an internal organisation detail.
//!
//! File layout mirrors §10-§17 of the parent roadmap:
//!
//! * [`scalar`] — evaluator fast path, expression literals.
//! * [`aggregation`] — count / sum / avg / min / max / collect / stdev.
//! * [`filter`] — `WHERE` predicates with property + label combos.
//! * [`label_scan`] — label-only anchors.
//! * [`order`] — `ORDER BY … LIMIT …`.
//! * [`point_read`] — id / name point reads.
//! * [`procedure`] — `CALL db.*` / `CALL dbms.*`.
//! * [`subquery`] — `WITH` pipelines, `COLLECT { }`, `COUNT { }`.
//! * [`traversal`] — 1-hop, 2-hop, variable-length, cartesian.
//! * [`write`] — `CREATE`, `MERGE`, `SET`, `UNWIND`+write, cycle.
//! * [`temporal_spatial`] — `date`/`duration` + `point.distance`.

use crate::scenario::Scenario;

pub(crate) mod aggregation;
pub(crate) mod filter;
pub(crate) mod label_scan;
pub(crate) mod order;
pub(crate) mod point_read;
pub(crate) mod procedure;
pub(crate) mod scalar;
pub(crate) mod subquery;
pub(crate) mod temporal_spatial;
pub(crate) mod traversal;
pub(crate) mod write;

/// Concatenate every category's scenarios in a stable order. The
/// order inside each submodule is preserved so `cargo test`'s
/// output listing stays predictable.
pub(crate) fn all() -> Vec<Scenario> {
    let mut out = Vec::new();
    out.extend(aggregation::scenarios());
    out.extend(filter::scenarios());
    out.extend(label_scan::scenarios());
    out.extend(order::scenarios());
    out.extend(point_read::scenarios());
    out.extend(procedure::scenarios());
    out.extend(scalar::scenarios());
    out.extend(subquery::scenarios());
    out.extend(temporal_spatial::scenarios());
    out.extend(traversal::scenarios());
    out.extend(write::scenarios());
    out
}
