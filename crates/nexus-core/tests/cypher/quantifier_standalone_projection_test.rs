//! A list-predicate quantifier must work in a STANDALONE projection — the most
//! common way these are written.
//!
//! `RETURN all(x IN [1,2] WHERE x > 0)` returned ZERO ROWS. A standalone
//! projection has no upstream rows, so `execute_project` / `execute_with` seed one
//! synthetic unit row, but only when every projected item is evaluable without
//! variables. The parser lowers a quantifier to three discrete args — the bound
//! variable's name as a STRING LITERAL, the list, the predicate — so the
//! evaluability walk saw a free `Variable("x")` in the predicate, refused to seed
//! the row, and the query silently produced nothing.
//!
//! The loss needed BOTH conditions, which is what made the shape look so narrow:
//! no reading clause upstream (otherwise rows already exist) AND a predicate that
//! mentions the bound variable (otherwise there is no `Variable` to trip over).

use nexus_core::testing::setup_isolated_test_engine;

fn one(query: &str) -> serde_json::Value {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = engine.execute_cypher(query).expect("query should execute");
    assert_eq!(rs.rows.len(), 1, "expected exactly one row from {query}");
    rs.rows[0].values[0].clone()
}

#[test]
fn all_in_a_standalone_return() {
    assert_eq!(one("RETURN all(x IN [1,2] WHERE x > 0) AS a"), true);
    assert_eq!(one("RETURN all(x IN [1,2] WHERE x > 1) AS a"), false);
}

#[test]
fn any_in_a_standalone_return() {
    assert_eq!(one("RETURN any(x IN [1,2] WHERE x > 1) AS a"), true);
    assert_eq!(one("RETURN any(x IN [1,2] WHERE x > 5) AS a"), false);
}

#[test]
fn none_in_a_standalone_return() {
    assert_eq!(one("RETURN none(x IN [1,2] WHERE x > 5) AS a"), true);
    assert_eq!(one("RETURN none(x IN [1,2] WHERE x > 1) AS a"), false);
}

#[test]
fn single_in_a_standalone_return() {
    assert_eq!(one("RETURN single(x IN [1,2] WHERE x > 1) AS a"), true);
    assert_eq!(one("RETURN single(x IN [1,2] WHERE x > 0) AS a"), false);
}

#[test]
fn empty_list_keeps_the_vacuous_truth_rules() {
    // Vacuous quantification: `all`/`none` hold over an empty list, `any`/`single`
    // do not. These reached the evaluator even before the fix (no rows to lose
    // when the predicate never runs) — pinned so the seeding change cannot
    // disturb them.
    assert_eq!(one("RETURN all(x IN [] WHERE x > 0) AS a"), true);
    assert_eq!(one("RETURN none(x IN [] WHERE x > 0) AS a"), true);
    assert_eq!(one("RETURN any(x IN [] WHERE x > 0) AS a"), false);
    assert_eq!(one("RETURN single(x IN [] WHERE x > 0) AS a"), false);
}

#[test]
fn a_predicate_that_ignores_the_bound_variable_still_works() {
    // Control: this shape never broke, because the predicate holds no `Variable`.
    assert_eq!(one("RETURN all(x IN [1,2] WHERE true) AS a"), true);
}

#[test]
fn an_upstream_clause_still_works() {
    // Control for the other half of the condition: with rows already in hand the
    // seeding gate is not consulted at all.
    assert_eq!(
        one("UNWIND [1] AS z RETURN all(x IN [1,2] WHERE x > 0) AS a"),
        true
    );
}

#[test]
fn a_quantifier_in_a_standalone_with_also_works() {
    // `execute_with` carries the same seeding gate as `execute_project`, so the
    // fix has to hold on both.
    assert_eq!(one("WITH all(x IN [1,2] WHERE x > 0) AS a RETURN a"), true);
}

#[test]
fn a_quantifier_nested_in_a_larger_expression() {
    // The gate is applied to the whole projected item, so a quantifier buried in
    // an operand must not veto seeding either.
    assert_eq!(
        one("RETURN all(x IN [1,2] WHERE x > 0) AND any(y IN [3] WHERE y > 2) AS a"),
        true
    );
    assert_eq!(one("RETURN NOT all(x IN [1,2] WHERE x > 1) AS a"), true);
}

#[test]
fn the_filter_legacy_form_is_unaffected() {
    // `filter(x IN list WHERE pred)` was never broken: the parser folds it into a
    // real `ListComprehension`, whose evaluability rule already excluded the loop
    // variable. Pinned to prove the two forms now agree.
    assert_eq!(
        one("RETURN filter(x IN [1,2] WHERE x > 1) AS a"),
        serde_json::json!([2])
    );
    assert_eq!(
        one("RETURN [x IN [1,2] WHERE x > 1] AS a"),
        serde_json::json!([2])
    );
}
