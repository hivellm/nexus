//! Planner test suite. Attached via `#[cfg(test)] mod tests;` in the
//! parent module; super::* pulls in every planner type.

#![allow(unused_imports)]
use super::*;
use crate::catalog::Catalog;
use crate::executor::JoinType;
use crate::executor::parser::{
    BinaryOperator, Clause, CypherParser, CypherQuery, Expression, LimitClause, Literal,
    MatchClause, NodePattern, Pattern, PatternElement, QuantifiedGroup, RelationshipDirection,
    RelationshipPattern, RelationshipQuantifier, ReturnClause, ReturnItem, WhereClause,
};
use crate::executor::planner::queries::{
    qpp_legacy_rewrite_enabled, set_qpp_legacy_rewrite_enabled,
};
use crate::executor::types::RangeSeekOp;
use crate::index::{KnnIndex, LabelIndex};
use crate::testing::TestContext;

/// Helper to create a test catalog with guaranteed directory existence
pub(super) fn create_test_catalog() -> (Catalog, TestContext) {
    let ctx = TestContext::new();
    let catalog = Catalog::with_isolated_path(
        ctx.path().join("catalog.mdb"),
        crate::catalog::CATALOG_MMAP_INITIAL_SIZE,
    )
    .expect("Failed to create catalog");
    (catalog, ctx)
}

// ── submodules ───────────────────────────────────────────────────────────────

mod aggregation_optimization;
mod basic_planning;
mod composite_index;
mod expression_and_cost;
mod index_hints;
mod join_cost;
mod quantified_path_patterns;
mod unindexed_property_notifications;
