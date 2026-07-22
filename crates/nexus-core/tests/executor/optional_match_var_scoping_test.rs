//! Regression suite: OPTIONAL MATCH's set of nullable variables must be
//! computed from *binding state* (which variables were already bound by
//! prior clauses), not from the pattern's textual position.
//!
//! Before the fix, the planner unconditionally treated the FIRST node in
//! every OPTIONAL MATCH pattern as "the anchor" and skipped it when
//! building the nullable-variable set consumed by
//! `Operator::OptionalFilter`. That assumption breaks in two ways:
//!
//! 1. **Reverse-direction patterns** — `OPTIONAL MATCH (b)-[:KNOWS]->(a)`
//!    where `a` (not `b`) is the already-bound anchor from a prior MATCH.
//!    The genuinely-nullable `b` got skipped and the bound `a` was added
//!    to the nullable set instead, inverting which side of the LEFT
//!    OUTER JOIN gets nulled out on no-match.
//! 2. **Standalone OPTIONAL MATCH** — a pattern whose only node is new
//!    (no relationship, nothing bound). Skipping "the first node" left
//!    the nullable-variable set empty, so the WHERE clause following it
//!    was lowered as a plain `Filter` instead of an `OptionalFilter` —
//!    rows with no qualifying match were dropped outright instead of
//!    being preserved with the new variable bound to NULL.
//!
//! The fix computes the nullable set as
//! `(pattern variables) - (variables already bound by prior clauses)`,
//! which is correct regardless of anchor position.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

/// `None` for `Value::Null`, `Some(s)` for `Value::String(s)` — keeps the
/// row-shape assertions below readable.
fn opt_string(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        other => panic!("expected string or null, got {other:?}"),
    }
}

#[test]
fn reverse_direction_optional_match_nulls_the_new_node_not_the_anchor() {
    // `a` is bound by the leading MATCH; `b` is the pattern's FIRST node
    // but is the genuinely-new/nullable side because the relationship
    // points INTO the anchor. Alice has no incoming :KNOWS edge, so `b`
    // must resolve to NULL while `a` stays bound.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {name: 'Alice'})")
        .unwrap();

    let r = engine
        .execute_cypher(
            "MATCH (a:Person {name:'Alice'}) \
             OPTIONAL MATCH (b:Person)-[:KNOWS]->(a) WHERE b.age > 30 \
             RETURN a.name, b.name",
        )
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row (Alice preserved via LEFT OUTER JOIN), got {}",
        r.rows.len()
    );
    let row = &r.rows[0].values;
    assert_eq!(
        opt_string(&row[0]),
        Some("Alice".to_string()),
        "the BOUND anchor `a` must never be nulled out by the OPTIONAL MATCH, got {:?}",
        row[0]
    );
    assert_eq!(
        opt_string(&row[1]),
        None,
        "the genuinely-new `b` must be NULL when no matching edge exists, got {:?}",
        row[1]
    );
}

#[test]
fn standalone_optional_match_preserves_driver_row_when_filter_excludes_new_node() {
    // The OPTIONAL MATCH pattern here introduces a single, brand-new
    // node (`c`) with no relationship to anything already bound. A
    // Company row exists but fails the WHERE predicate, so `c` must
    // resolve to NULL and the driving `a` row must still come through —
    // NOT be dropped, which is what a plain (non-optional) `Filter`
    // would do.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {name: 'Alice'})")
        .unwrap();
    engine
        .execute_cypher("CREATE (:Company {name: 'Acme', rating: 2})")
        .unwrap();

    let r = engine
        .execute_cypher(
            "MATCH (a:Person) OPTIONAL MATCH (c:Company) WHERE c.rating > 4 \
             RETURN a.name, c.name",
        )
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));

    assert_eq!(
        r.rows.len(),
        1,
        "the `a` row must be preserved with c=NULL, not dropped, got {} rows",
        r.rows.len()
    );
    let row = &r.rows[0].values;
    assert_eq!(opt_string(&row[0]), Some("Alice".to_string()));
    assert_eq!(
        opt_string(&row[1]),
        None,
        "non-qualifying `c` must resolve to NULL, got {:?}",
        row[1]
    );
}

#[test]
fn forward_anchor_optional_match_where_still_left_outer_joins() {
    // Baseline that must stay green: the anchor `a` genuinely IS the
    // pattern's first node here, and the target `b` is the new/nullable
    // side. One anchor's friend fails the WHERE (nulled), the other's
    // passes (kept) — exercised together so row order can't matter (the
    // `OptionalFilter` groups by a HashMap, so the fix must not depend
    // on iteration order).
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a1:Person {name: 'Alice1'}), (young:Person {name: 'Young', age: 25}), \
             (a1)-[:KNOWS]->(young)",
        )
        .unwrap();
    engine
        .execute_cypher(
            "CREATE (a2:Person {name: 'Alice2'}), (elder:Person {name: 'Elder', age: 40}), \
             (a2)-[:KNOWS]->(elder)",
        )
        .unwrap();

    let r = engine
        .execute_cypher(
            "MATCH (a:Person) WHERE NOT a.name IN ['Young', 'Elder'] \
             OPTIONAL MATCH (a)-[:KNOWS]->(b) WHERE b.age > 30 \
             RETURN a.name, b.name",
        )
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));

    let mut got: Vec<(Option<String>, Option<String>)> = r
        .rows
        .iter()
        .map(|row| (opt_string(&row.values[0]), opt_string(&row.values[1])))
        .collect();
    got.sort();

    let mut expected = vec![
        (Some("Alice1".to_string()), None),
        (Some("Alice2".to_string()), Some("Elder".to_string())),
    ];
    expected.sort();

    assert_eq!(got, expected, "forward-anchor LEFT OUTER JOIN regressed");
}

#[test]
fn chained_optional_match_does_not_leak_the_first_clauses_new_var_into_the_second() {
    // Two consecutive OPTIONAL MATCH clauses. `b` is genuinely new in
    // the first clause and genuinely already-bound (though nullable) in
    // the second. If the planner forgets to record `b` as bound after
    // the first OPTIONAL MATCH, the second clause's nullable set wrongly
    // becomes {b, c} instead of {c}, and `b` gets spuriously nulled out
    // even though it matched.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob', age: 40}), \
             (c:Company {name: 'Widgets', rating: 1}), \
             (a)-[:KNOWS]->(b), (b)-[:WORKS_AT]->(c)",
        )
        .unwrap();

    let r = engine
        .execute_cypher(
            "MATCH (a:Person {name:'Alice'}) \
             OPTIONAL MATCH (a)-[:KNOWS]->(b) WHERE b.age > 30 \
             OPTIONAL MATCH (b)-[:WORKS_AT]->(c) WHERE c.rating > 4 \
             RETURN a.name, b.name, c.name",
        )
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row, got {}",
        r.rows.len()
    );
    let row = &r.rows[0].values;
    assert_eq!(opt_string(&row[0]), Some("Alice".to_string()));
    assert_eq!(
        opt_string(&row[1]),
        Some("Bob".to_string()),
        "Bob matched b.age > 30 in the FIRST OPTIONAL MATCH and must not be nulled \
         out by the second clause's OptionalFilter, got {:?}",
        row[1]
    );
    assert_eq!(
        opt_string(&row[2]),
        None,
        "Widgets fails c.rating > 4 so `c` must resolve to NULL, got {:?}",
        row[2]
    );
}

#[test]
fn chained_optional_match_both_links_resolve() {
    // Sanity companion to the leak-guard above: when both OPTIONAL
    // MATCH clauses find a qualifying match, both new variables must
    // come through bound (not spuriously nulled by an over-eager fix).
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob', age: 40}), \
             (c:Company {name: 'Acme', rating: 5}), \
             (a)-[:KNOWS]->(b), (b)-[:WORKS_AT]->(c)",
        )
        .unwrap();

    let r = engine
        .execute_cypher(
            "MATCH (a:Person {name:'Alice'}) \
             OPTIONAL MATCH (a)-[:KNOWS]->(b) WHERE b.age > 30 \
             OPTIONAL MATCH (b)-[:WORKS_AT]->(c) WHERE c.rating > 4 \
             RETURN a.name, b.name, c.name",
        )
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row, got {}",
        r.rows.len()
    );
    let row = &r.rows[0].values;
    assert_eq!(opt_string(&row[0]), Some("Alice".to_string()));
    assert_eq!(opt_string(&row[1]), Some("Bob".to_string()));
    assert_eq!(opt_string(&row[2]), Some("Acme".to_string()));
}
