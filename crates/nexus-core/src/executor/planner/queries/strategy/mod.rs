//! `plan_execution_strategy`, `synthesise_anonymous_source_anchors`,
//! `select_start_pattern`, and `node_index_seek_for` — the pattern-to-operator
//! lowering pass.
//!
//! Directory module. Previously the monolithic `strategy.rs` (2054 lines).
//! Split into cohesive submodules by responsibility; zero logic changes,
//! only `use`-path adjustments and visibility annotations.
//!
//! - `pattern_lowering` — [`pattern_lowering`]'s `plan_execution_strategy`,
//!   the main pattern/clause lowering pass: NodeByLabel/AllNodesScan/Expand
//!   emission for MATCH patterns, WHERE-clause Filter/OptionalFilter
//!   lowering, RETURN projection/aggregation build, and the trailing
//!   ORDER BY/SKIP/LIMIT operators.
//! - `start_pattern` — `select_start_pattern` (start-pattern selection) and
//!   `synthesise_anonymous_source_anchors` (anonymous anchor synthesis for
//!   labelled/filtered source nodes).
//! - `index_seek` — `composite_index_seek_for` and `node_index_seek_for`,
//!   the inline-property (`MATCH (n:L {a: 1})`) index-seek lowering.
//! - `where_lowering` — the WHERE-form index-seek lowering family:
//!   AND-conjunct flatten/rebuild helpers, the `where_equality_index_seek_for`
//!   dispatcher, and the individual seek-operand lowerings (equality, range,
//!   `IN`, `STARTS WITH`, `$parameter`).

mod index_seek;
mod pattern_lowering;
mod start_pattern;
mod where_lowering;

pub(super) use super::*;
