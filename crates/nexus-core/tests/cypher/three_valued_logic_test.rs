//! Three-valued (Kleene) logic for Cypher's logical operators and
//! null-propagating predicates. Logical operators are strict about operand
//! type (`123 AND true` is an error); NULL propagates through AND/OR/NOT,
//! comparison, IN, string predicates, and list slices. WHERE-filter
//! truthiness is preserved (a NULL predicate drops the row).

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn val(query: &str) -> Value {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = engine.execute_cypher(query).expect("query should succeed");
    r.rows
        .first()
        .and_then(|row| row.values.first().cloned())
        .expect("one value")
}

fn is_err(query: &str) -> bool {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher(query).is_err()
}

// ── AND / OR / NOT (Kleene) ──────────────────────────────────────────────────

#[test]
fn and_propagates_null_unless_false_dominates() {
    assert!(val("RETURN true AND null AS v").is_null());
    assert_eq!(val("RETURN null AND false AS v"), Value::Bool(false));
    assert_eq!(val("RETURN false AND null AS v"), Value::Bool(false));
    assert_eq!(val("RETURN true AND true AS v"), Value::Bool(true));
}

#[test]
fn or_propagates_null_unless_true_dominates() {
    assert_eq!(val("RETURN true OR null AS v"), Value::Bool(true));
    assert!(val("RETURN null OR false AS v").is_null());
    assert_eq!(val("RETURN false OR false AS v"), Value::Bool(false));
}

#[test]
fn not_null_is_null() {
    assert!(val("RETURN NOT null AS v").is_null());
    assert_eq!(val("RETURN NOT true AS v"), Value::Bool(false));
}

#[test]
fn xor_is_three_valued() {
    assert_eq!(val("RETURN true XOR false AS v"), Value::Bool(true));
    assert_eq!(val("RETURN true XOR true AS v"), Value::Bool(false));
    assert_eq!(val("RETURN false XOR false AS v"), Value::Bool(false));
    assert!(val("RETURN true XOR null AS v").is_null());
    assert!(val("RETURN null XOR false AS v").is_null());
}

#[test]
fn xor_binds_tighter_than_or_looser_than_and() {
    // OR < XOR < AND: `true OR true XOR true AND false`
    //   = true OR (true XOR (true AND false))
    //   = true OR (true XOR false) = true OR true = true
    assert_eq!(
        val("RETURN true OR true XOR true AND false AS v"),
        Value::Bool(true)
    );
    // `false XOR true AND true` = false XOR (true AND true) = false XOR true = true
    assert_eq!(
        val("RETURN false XOR true AND true AS v"),
        Value::Bool(true)
    );
}

// ── Operand type-guards ──────────────────────────────────────────────────────

#[test]
fn non_boolean_logical_operand_is_an_error() {
    assert!(is_err("RETURN 123 AND true AS v"));
    assert!(is_err("RETURN 'x' OR false AS v"));
    assert!(is_err("RETURN NOT 5 AS v"));
}

// ── Comparison / IN / string predicates ──────────────────────────────────────

#[test]
fn comparison_with_null_is_null() {
    assert!(val("RETURN null < 5 AS v").is_null());
    assert!(val("RETURN 5 >= null AS v").is_null());
}

#[test]
fn in_is_three_valued() {
    assert!(val("RETURN 1 IN [null, 2] AS v").is_null());
    assert_eq!(val("RETURN 2 IN [null, 2] AS v"), Value::Bool(true));
    assert_eq!(val("RETURN 3 IN [1, 2] AS v"), Value::Bool(false));
    assert!(val("RETURN null IN [1, 2] AS v").is_null());
}

#[test]
fn string_predicates_with_null_are_null() {
    assert!(val("RETURN 'x' STARTS WITH null AS v").is_null());
    assert!(val("RETURN null ENDS WITH 'x' AS v").is_null());
    assert!(val("RETURN 'x' CONTAINS null AS v").is_null());
}

#[test]
fn list_slice_with_null_bound_is_null() {
    assert!(val("RETURN [1, 2, 3][null..2] AS v").is_null());
    assert!(val("RETURN [1, 2, 3][0..null] AS v").is_null());
}

// ── WHERE-filter truthiness preserved (NULL predicate drops the row) ─────────

#[test]
fn where_null_predicate_drops_the_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .create_node(vec!["N".to_string()], serde_json::json!({ "a": 1 }))
        .unwrap();
    engine.refresh_executor().unwrap();
    // n.missing is NULL → NOT null → NULL → row dropped.
    let r = engine
        .execute_cypher("MATCH (n:N) WHERE NOT n.missing RETURN n")
        .unwrap();
    assert_eq!(r.rows.len(), 0, "NOT NULL predicate must drop the row");
    // true AND null → NULL → dropped.
    let r2 = engine
        .execute_cypher("MATCH (n:N) WHERE true AND n.missing RETURN n")
        .unwrap();
    assert_eq!(r2.rows.len(), 0);
    // Sanity: a satisfiable predicate keeps the row.
    let r3 = engine
        .execute_cypher("MATCH (n:N) WHERE n.a = 1 RETURN n")
        .unwrap();
    assert_eq!(r3.rows.len(), 1);
}
