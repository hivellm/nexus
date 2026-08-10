//! Regression coverage for aggregating-projection grouping keys and for
//! aggregates nested inside a larger expression.
//!
//! Two related bugs are locked in here:
//!
//! 1. In an aggregating projection, every non-aggregate item is an implicit
//!    grouping key and must keep its column — whatever its expression shape
//!    (bare variable, alias of a variable, or a literal). The non-MATCH
//!    planner path (`UNWIND` / bare `RETURN` / `WITH`) used to drop the key
//!    entirely, collapsing all rows into one group and emitting only the
//!    aggregate column.
//! 2. An aggregate nested inside a larger expression (`count(*) + 1`,
//!    `count(n) > 0`, `[count(*)]`, ...) is evaluated once per group, with
//!    the enclosing expression computed afterwards — including over an
//!    empty group, where the row must still be produced instead of being
//!    dropped.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

/// Engine with no seeded data — used by the `UNWIND`-only grouping tests.
fn seed() -> (TestContext, Engine) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    (ctx, engine)
}

/// Engine seeded with three `:P` nodes, `x` = 2, 1, 1.
fn seed_p() -> (TestContext, Engine) {
    let (ctx, mut engine) = seed();
    engine
        .execute_cypher("CREATE (:P {x: 2}), (:P {x: 1}), (:P {x: 1})")
        .unwrap();
    (ctx, engine)
}

/// Collect a two-column `(i64 key, i64 count)` result into a map for
/// order-insensitive comparison, asserting the output columns match the
/// projection aliases. Grouping is hash-based, so row order is never
/// stable and must never be asserted on directly.
fn grouped_i64(engine: &mut Engine, query: &str, expected_cols: &[&str]) -> HashMap<i64, i64> {
    let r = engine.execute_cypher(query).expect("query must execute");
    assert_eq!(
        r.columns, expected_cols,
        "output columns must be the projection aliases for `{query}`"
    );
    let mut map = HashMap::new();
    for row in &r.rows {
        let key = row.values[0]
            .as_i64()
            .unwrap_or_else(|| panic!("group key must be an integer, got {:?}", row.values[0]));
        let count = row.values[1]
            .as_i64()
            .unwrap_or_else(|| panic!("count must be an integer, got {:?}", row.values[1]));
        map.insert(key, count);
    }
    map
}

// ---------------------------------------------------------------------
// Grouping keys on the non-MATCH path
// ---------------------------------------------------------------------

#[test]
fn unwind_with_variable_alias_key_groups_and_counts() {
    let (_ctx, mut engine) = seed();
    let expected: HashMap<i64, i64> = [(1, 2), (2, 1)].into();
    assert_eq!(
        grouped_i64(
            &mut engine,
            "UNWIND [1, 1, 2] AS v WITH v AS k, count(*) AS c RETURN k, c",
            &["k", "c"],
        ),
        expected,
    );
}

#[test]
fn unwind_with_literal_key_single_group_not_dropped() {
    // The literal grouping key is the case that used to vanish entirely:
    // the whole result collapsed to a row with only the aggregate column.
    let (_ctx, mut engine) = seed();
    let r = engine
        .execute_cypher("UNWIND [1, 1, 2] AS v WITH 9 AS k, count(*) AS c RETURN k, c")
        .unwrap();
    assert_eq!(r.columns, vec!["k".to_string(), "c".to_string()]);
    assert_eq!(
        r.rows.len(),
        1,
        "a literal grouping key groups every row into a single group"
    );
    assert_eq!(r.rows[0].values[0].as_i64(), Some(9));
    assert_eq!(r.rows[0].values[1].as_i64(), Some(3));
}

#[test]
fn unwind_return_without_with_groups_and_counts() {
    let (_ctx, mut engine) = seed();
    let expected: HashMap<i64, i64> = [(1, 2), (2, 1)].into();
    assert_eq!(
        grouped_i64(
            &mut engine,
            "UNWIND [1, 1, 2] AS v RETURN v AS k, count(*) AS c",
            &["k", "c"],
        ),
        expected,
    );
}

#[test]
fn unwind_with_where_after_aggregation_filters_groups() {
    let (_ctx, mut engine) = seed();
    let r = engine
        .execute_cypher("UNWIND [1, 1, 2] AS v WITH v AS k, count(*) AS c WHERE c > 1 RETURN k, c")
        .unwrap();
    assert_eq!(r.columns, vec!["k".to_string(), "c".to_string()]);
    assert_eq!(r.rows.len(), 1, "WHERE must be applied after aggregation");
    assert_eq!(r.rows[0].values[0].as_i64(), Some(1));
    assert_eq!(r.rows[0].values[1].as_i64(), Some(2));
}

// ---------------------------------------------------------------------
// Aggregate nested in an expression (seeded `:P` nodes, x = 2, 1, 1)
// ---------------------------------------------------------------------

#[test]
fn match_return_count_comparison_expression() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (n:P) RETURN count(n) > 0")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_bool(), Some(true));
}

#[test]
fn match_return_count_star_plus_literal() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (n:P) RETURN count(*) + 1")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_i64(), Some(4));
}

#[test]
fn match_return_sum_times_literal() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (n:P) RETURN sum(n.x) * 2")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_i64(), Some(8));
}

#[test]
fn match_return_count_star_wrapped_in_list_literal() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (n:P) RETURN [count(*)]")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    let arr = r.rows[0].values[0]
        .as_array()
        .expect("expected a list value");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0].as_i64(), Some(3));
}

#[test]
fn match_return_count_star_inside_case_expression() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (n:P) RETURN CASE WHEN count(*) > 0 THEN 1 ELSE 0 END")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_i64(), Some(1));
}

#[test]
fn unwind_return_count_comparison_collapses_to_single_row() {
    // Previously produced one row per UNWIND input instead of a single
    // aggregated row.
    let (_ctx, mut engine) = seed();
    let r = engine
        .execute_cypher("UNWIND [1, 2] AS v RETURN count(v) > 0")
        .unwrap();
    assert_eq!(
        r.rows.len(),
        1,
        "an aggregate over UNWIND input must collapse to exactly one row"
    );
    assert_eq!(r.rows[0].values[0].as_bool(), Some(true));
}

#[test]
fn bare_return_count_star_comparison() {
    let (_ctx, mut engine) = seed();
    let r = engine.execute_cypher("RETURN count(*) > 0").unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_bool(), Some(true));
}

// ---------------------------------------------------------------------
// Empty input keeps the row
// ---------------------------------------------------------------------

#[test]
fn match_absent_label_count_returns_zero_row() {
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (a:Absent) RETURN count(a)")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_i64(), Some(0));
}

#[test]
fn match_absent_label_count_comparison_keeps_single_row() {
    // Previously the enclosing comparison over an empty group produced zero
    // rows instead of a single `false` row.
    let (_ctx, mut engine) = seed_p();
    let r = engine
        .execute_cypher("MATCH (a:Absent) RETURN count(a) > 0")
        .unwrap();
    assert_eq!(
        r.rows.len(),
        1,
        "an aggregate over empty input must still emit exactly one row"
    );
    assert_eq!(r.rows[0].values[0].as_bool(), Some(false));
}

// ---------------------------------------------------------------------
// Grouping key survives a post-aggregation projection
// ---------------------------------------------------------------------

#[test]
fn match_group_key_survives_aggregate_plus_literal_projection() {
    let (_ctx, mut engine) = seed_p();
    let expected: HashMap<i64, i64> = [(1, 3), (2, 2)].into();
    assert_eq!(
        grouped_i64(
            &mut engine,
            "MATCH (n:P) RETURN n.x AS k, count(*) + 1 AS c",
            &["k", "c"],
        ),
        expected,
    );
}

#[test]
fn match_group_key_survives_head_collect_projection() {
    let (_ctx, mut engine) = seed_p();
    let expected: HashMap<i64, i64> = [(1, 1), (2, 2)].into();
    assert_eq!(
        grouped_i64(
            &mut engine,
            "MATCH (n:P) RETURN n.x AS k, head(collect(n.x)) AS h",
            &["k", "h"],
        ),
        expected,
    );
}
