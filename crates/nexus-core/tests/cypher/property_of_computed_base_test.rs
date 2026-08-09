//! `.property` read off a COMPUTED base — `(expr).prop`, `f(x).prop`.
//!
//! `Expression::PropertyAccess` names its base by VARIABLE, so it can only spell
//! `n.prop`; a base that is itself an expression had nowhere to go and the parser
//! simply stopped at the `.`, letting the projection list be truncated there
//! (silently, before the leftover-input guard). Resolved with a sibling AST
//! variant, `PropertyOf { base, property }`, rather than by widening the shared
//! one — 75 call sites read `PropertyAccess.variable` directly.

use nexus_core::testing::setup_isolated_test_engine;

fn run(query: &str) -> nexus_core::executor::ResultSet {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher(query).expect("query should execute")
}

/// The shape from the task's proposal: two items, both reading off `list[1]`.
#[test]
fn property_of_an_indexed_element() {
    let rs = run(
        "WITH [123, {existing: 42}] AS list RETURN (list[1]).missing AS m, (list[1]).existing AS e",
    );
    assert_eq!(rs.columns, vec!["m".to_string(), "e".to_string()]);
    assert!(rs.rows[0].values[0].is_null(), "absent key reads as null");
    assert_eq!(rs.rows[0].values[1].as_i64(), Some(42));
}

#[test]
fn property_of_a_map_literal() {
    let rs = run("RETURN ({a: 1, b: 2}).b AS x");
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(2));
}

#[test]
fn property_of_a_parenthesised_property_access() {
    // Chained: the base is itself a property read.
    let rs = run("WITH {a: {b: 7}} AS m RETURN (m.a).b AS deep");
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(7));
}

#[test]
fn property_of_a_non_container_base_is_null() {
    // Cypher reads a property off a non-container as NULL rather than erroring,
    // the same as `null.prop`.
    let rs = run("RETURN (1 + 2).foo AS x");
    assert!(rs.rows[0].values[0].is_null());
}

#[test]
fn property_of_a_null_base_is_null() {
    let rs = run("RETURN (null).foo AS x");
    assert!(rs.rows[0].values[0].is_null());
}

#[test]
fn property_of_a_function_call_result() {
    // `startNode(r).id` — the second shape the proposal lists. The suffix loop
    // runs after the call's own `[index]` loop, so both compose.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:P {id: 1})-[:KNOWS]->(b:P {id: 2})")
        .expect("fixture");
    engine.refresh_executor().unwrap();

    let rs = engine
        .execute_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN startNode(r).id AS s, endNode(r).id AS e")
        .expect("both items must parse");
    assert_eq!(rs.columns, vec!["s".to_string(), "e".to_string()]);
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(1));
    assert_eq!(rs.rows[0].values[1].as_i64(), Some(2));
}

#[test]
fn the_plain_variable_form_is_unchanged() {
    // Control: `n.prop` keeps using `PropertyAccess`, untouched.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:P {name: 'x'})")
        .expect("fixture");
    engine.refresh_executor().unwrap();
    let rs = engine
        .execute_cypher("MATCH (n:P) RETURN n.name AS name")
        .expect("query");
    assert_eq!(rs.rows[0].values[0].as_str(), Some("x"));
}

/// Two adjacent parser gaps found while testing this one, pinned as KNOWN gaps
/// rather than as desired behaviour. Both now fail loudly through the
/// leftover-input guard instead of truncating the query silently, which is
/// exactly what that guard was for. Delete these when the index/slice loop is
/// shared across postfix positions.
#[test]
fn slicing_is_a_separate_pre_existing_gap() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A slice on a bare variable: rejected by the index parser itself.
    let err = engine
        .execute_cypher("WITH [1,2,3,4] AS l RETURN l[1..3] AS s")
        .expect_err("list slicing in this form is not supported yet");
    assert!(err.to_string().contains("Expected ']'"), "got: {err}");

    // Indexing a PARENTHESISED expression: the parenthesised branch has no index
    // loop, so the leftover `[1..3]` is caught by the guard. This change added
    // only `.prop` there.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("WITH [1,2,3,4] AS l RETURN (l)[1] AS s")
        .expect_err("indexing a parenthesised expression is not supported yet");
    assert!(
        err.to_string()
            .contains("unexpected input after the last clause"),
        "must fail loudly rather than truncate, got: {err}"
    );
}
