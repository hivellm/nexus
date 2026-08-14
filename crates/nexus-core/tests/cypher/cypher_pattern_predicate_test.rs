//! Integration coverage for a relationship pattern accepted as a boolean
//! predicate anywhere an expression is legal — not only after `NOT` or
//! inside `exists { … }`.
//!
//! Every accepted form desugars to
//! `Expression::Exists { inner: ExistsInner::Pattern { where_clause: None, .. } }`
//! and routes through the same machinery the pre-existing `NOT (pattern)`
//! form already used (parser-level coverage of the AST shape itself lives in
//! `crates/nexus-core/src/executor/parser/tests/patterns.rs`). This file
//! locks in the end-to-end execution behavior: bare `WHERE`, both sides of
//! `AND`, the `exists { … }` / `exists(pattern)` forms, the `RETURN … AS`
//! projection form, and two controls proving ordinary parenthesized
//! expressions are unaffected.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

/// Engine seeded with the fixture used by every test in this file:
/// `(:A {n:1})-[:T]->(:B {n:2})` plus a disconnected `(:A {n:3})`.
fn seed() -> (TestContext, Engine) {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:A {n:1})-[:T]->(:B {n:2}), (:A {n:3})")
        .unwrap();
    (ctx, engine)
}

/// Collect a single-column `n.n` (i64) result into a sorted vec for
/// order-insensitive comparison.
fn i64_column(engine: &mut Engine, query: &str) -> Vec<i64> {
    let r = engine.execute_cypher(query).expect("query must execute");
    let mut got: Vec<i64> = r
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_i64()
                .unwrap_or_else(|| panic!("expected an integer, got {:?}", row.values[0]))
        })
        .collect();
    got.sort_unstable();
    got
}

#[test]
fn where_bare_pattern_predicate_filters_to_connected_node() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(&mut engine, "MATCH (n:A) WHERE (n)-[:T]->() RETURN n.n"),
        vec![1],
    );
}

#[test]
fn where_not_pattern_predicate_filters_to_disconnected_node() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(&mut engine, "MATCH (n:A) WHERE NOT (n)-[:T]->() RETURN n.n"),
        vec![3],
    );
}

#[test]
fn where_exists_block_form_still_works() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(
            &mut engine,
            "MATCH (n:A) WHERE exists { (n)-[:T]->() } RETURN n.n",
        ),
        vec![1],
    );
}

#[test]
fn where_exists_function_call_form_filters_to_connected_node() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(
            &mut engine,
            "MATCH (n:A) WHERE exists((n)-[:T]->()) RETURN n.n",
        ),
        vec![1],
    );
}

#[test]
fn where_pattern_predicate_on_left_of_and_filters_to_connected_node() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(
            &mut engine,
            "MATCH (n:A) WHERE (n)-[:T]->() AND n.n = 1 RETURN n.n",
        ),
        vec![1],
    );
}

#[test]
fn where_pattern_predicate_on_right_of_and_filters_to_connected_node() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(
            &mut engine,
            "MATCH (n:A) WHERE n.n = 1 AND (n)-[:T]->() RETURN n.n",
        ),
        vec![1],
    );
}

#[test]
fn return_projection_form_yields_boolean_per_row() {
    let (_ctx, mut engine) = seed();
    let r = engine
        .execute_cypher("MATCH (n:A) RETURN n.n, (n)-[:T]->() AS has")
        .unwrap();
    assert_eq!(r.columns, vec!["n.n".to_string(), "has".to_string()]);
    assert_eq!(r.rows.len(), 2, "both :A nodes must be projected");

    let mut got: HashMap<i64, bool> = HashMap::new();
    for row in &r.rows {
        let n = row.values[0]
            .as_i64()
            .unwrap_or_else(|| panic!("expected an integer, got {:?}", row.values[0]));
        let has = row.values[1]
            .as_bool()
            .unwrap_or_else(|| panic!("expected a boolean, got {:?}", row.values[1]));
        got.insert(n, has);
    }
    let expected: HashMap<i64, bool> = [(1, true), (3, false)].into();
    assert_eq!(got, expected);
}

// ── Controls: ordinary parenthesized expressions must keep behaving as
// plain expressions, not pattern predicates. ──

#[test]
fn parenthesized_property_access_still_behaves_as_a_plain_expression() {
    let (_ctx, mut engine) = seed();
    assert_eq!(
        i64_column(&mut engine, "MATCH (n:A) WHERE (n.n) = 1 RETURN n.n"),
        vec![1],
    );
}

#[test]
fn parenthesized_arithmetic_still_behaves_as_a_plain_expression() {
    let (_ctx, mut engine) = seed();
    let r = engine.execute_cypher("RETURN (1 + 2) = 3 AS ok").unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_bool(), Some(true));
}
