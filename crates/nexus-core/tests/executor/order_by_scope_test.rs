//! An `ORDER BY` after a `WITH` sees only what that `WITH` projects.
//!
//! Two rules, both previously unenforced — the queries ran and answered:
//!
//! - it may not introduce an aggregation the projection did not compute
//!   (`WITH n.p AS foo ORDER BY count(1)` → `InvalidAggregation`);
//! - it may not name a variable the `WITH` dropped, even though that variable
//!   is bound elsewhere in the query (`WITH 1 AS a, 3 AS c WITH a ORDER BY c`
//!   → `UndefinedVariable`). The pre-existing reference checker cannot catch
//!   this: it asks whether a name is bound *anywhere*, and `c` is.

use nexus_core::testing::setup_isolated_test_engine;

fn engine() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:OrderScope {num1: 1, num2: 10})")
        .unwrap();
    engine
        .execute_cypher("CREATE (:OrderScope {num1: 2, num2: 20})")
        .unwrap();
    std::mem::forget(ctx);
    engine
}

#[track_caller]
fn assert_rejected(query: &str, detail: &str) {
    let mut engine = engine();
    match engine.execute_cypher(query) {
        Ok(rs) => panic!(
            "expected `{query}` to be rejected, got {} rows",
            rs.rows.len()
        ),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains(detail),
                "`{query}` raised the wrong error, wanted {detail}: {msg}"
            );
        }
    }
}

#[track_caller]
fn assert_accepted(query: &str) {
    let mut engine = engine();
    if let Err(e) = engine.execute_cypher(query) {
        panic!("`{query}` should be legal but was rejected: {e}");
    }
}

// ── aggregation in ORDER BY ────────────────────────────────────────────

#[test]
fn an_aggregation_the_projection_did_not_compute_is_rejected() {
    for sort in ["count(1)", "count(n.num1)", "max(n.num2)", "1 + count(1)"] {
        assert_rejected(
            &format!("MATCH (n:OrderScope) WITH n.num1 AS foo ORDER BY {sort} RETURN foo"),
            "InvalidAggregation",
        );
    }
}

#[test]
fn ordering_by_an_aggregate_the_with_projects_is_legal() {
    assert_accepted("MATCH (n:OrderScope) WITH count(*) AS c ORDER BY count(*) RETURN c");
    assert_accepted("MATCH (n:OrderScope) WITH count(*) AS c ORDER BY c RETURN c");
}

// ── out-of-scope variables ─────────────────────────────────────────────

#[test]
fn a_variable_the_with_dropped_is_rejected() {
    // `c` survives neither the WITH's output nor its input: the middle WITH
    // already dropped it, so by the ORDER BY it is two scopes gone.
    assert_rejected(
        "WITH 1 AS a, 'b' AS b, 3 AS c, true AS d WITH a, b WITH a ORDER BY a, c RETURN a",
        "UndefinedVariable",
    );
}

#[test]
fn sorting_by_a_name_the_with_consumed_but_did_not_forward_is_legal() {
    // The counterpart to the test above, and the line between them: `sum` is
    // in the WITH's *input*, so it may be sorted by even though the WITH does
    // not project it. Checking only the output rejects this valid query.
    assert_accepted(
        "MATCH (n:OrderScope) WITH n, n.num1 + n.num2 AS sum \
         WITH n, n.num2 % 3 AS m ORDER BY sum LIMIT 3 RETURN n.num1, m",
    );
}

#[test]
fn a_projected_aggregate_wrapped_in_arithmetic_is_legal() {
    // The aggregation is judged per sub-expression: `avg(...)` / `count(...)`
    // below were computed by the projection, and wrapping one in more
    // arithmetic does not make it a new aggregation.
    assert_accepted(
        "MATCH (n:OrderScope) WITH avg(n.num1) AS a ORDER BY 1 + avg(n.num1) - 1000 RETURN a",
    );
    assert_accepted(
        "MATCH (n:OrderScope) WITH n.num1 AS age, count(n.num2) AS cnt \
         ORDER BY age, age + count(n.num2) RETURN age",
    );
    assert_accepted(
        "MATCH (n:OrderScope) WITH n.num1 AS age, count(n.num2) AS cnt \
         ORDER BY n.num1 + count(n.num2) RETURN age",
    );
}

#[test]
fn a_variable_never_bound_at_all_is_rejected() {
    assert_rejected(
        "WITH 1 AS a WITH a ORDER BY zzz RETURN a",
        "UndefinedVariable",
    );
}

#[test]
fn a_dropped_variable_inside_a_compound_key_is_rejected() {
    // Three WITHs: the middle one drops `c`, so it is gone from both the last
    // one's output and its input. With only two, `c` would still be in the
    // input scope and the key would be legal.
    assert_rejected(
        "WITH 1 AS a, 3 AS c WITH a WITH a ORDER BY a + c RETURN a",
        "UndefinedVariable",
    );
}

#[test]
fn a_variable_one_scope_back_is_still_visible_to_a_compound_key() {
    assert_accepted("WITH 1 AS a, 3 AS c WITH a ORDER BY a + c RETURN a");
}

// ── still legal — the rule must not overreach ──────────────────────────

#[test]
fn ordering_by_a_projected_alias_is_legal() {
    assert_accepted("MATCH (n:OrderScope) WITH n.num1 AS foo ORDER BY foo RETURN foo");
}

#[test]
fn ordering_by_a_bare_passed_through_variable_is_legal() {
    assert_accepted("MATCH (n:OrderScope) WITH n ORDER BY n.num1 RETURN n.num1");
}

#[test]
fn ordering_by_an_expression_over_a_projected_alias_is_legal() {
    assert_accepted("MATCH (n:OrderScope) WITH n.num1 AS foo ORDER BY foo * -1 RETURN foo");
}

#[test]
fn ordering_by_the_same_expression_the_with_projects_is_legal() {
    assert_accepted(
        "MATCH (n:OrderScope) WITH n.num1 + n.num2 AS s ORDER BY n.num1 + n.num2 RETURN s",
    );
}

#[test]
fn a_return_order_by_may_still_name_a_dropped_variable() {
    // A RETURN's ORDER BY keeps the looser rule: sorting by a pre-projection
    // property the RETURN does not carry is legal and widely relied upon.
    assert_accepted("MATCH (n:OrderScope) RETURN n.num1 ORDER BY n.num2");
}

#[test]
fn a_literal_sort_key_is_legal() {
    assert_accepted("MATCH (n:OrderScope) WITH n.num1 AS foo ORDER BY 1 RETURN foo");
}
