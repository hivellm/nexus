//! Extended Regression Tests — MATCH clause coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers MATCH with filters, ordering, distinct and edge cases.

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_match_single_label() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_match_with_where_equals() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 42}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 100}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.value = 42 RETURN n.value AS value")
        .unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn regression_match_with_where_greater() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.value > 5 RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(4));
}

#[test]
fn regression_match_with_where_less() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.value < 3 RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
fn regression_match_with_where_gte() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.value >= 7 RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
fn regression_match_with_where_lte() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.value <= 2 RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
fn regression_match_with_limit() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..20 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n LIMIT 5")
        .unwrap();
    assert_eq!(result.rows.len(), 5);
}

#[test]
fn regression_match_with_order_by() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Charlie"}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Bob"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.name AS name ORDER BY n.name")
        .unwrap();
    assert_eq!(result.rows.len(), 3);
}

#[test]
fn regression_match_return_distinct() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN DISTINCT n.value AS value")
        .unwrap();
    assert!(result.rows.len() <= 2);
}

#[test]
fn regression_match_count_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..15 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(15));
}

#[test]
fn regression_match_property_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"name": "Bob"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test {name: 'Alice'}) RETURN n.name AS name")
        .unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn regression_match_all_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["C".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
fn regression_match_return_multiple_cols() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"a": 1, "b": 2, "c": 3}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.a AS a, n.b AS b, n.c AS c")
        .unwrap();
    assert_eq!(result.columns.len(), 3);
}

#[test]
fn regression_match_with_and_condition() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"a": 10, "b": 20}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"a": 10, "b": 30}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"a": 15, "b": 20}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) WHERE n.a = 10 AND n.b = 20 RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_match_with_or_condition() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 10}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 20}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 30}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (n:Test) WHERE n.value = 10 OR n.value = 30 RETURN count(n) AS count",
        )
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(2));
}

#[test]
fn regression_match_nonexistent_label() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:NonExistent) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(0));
}

#[test]
fn regression_match_nonexistent_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"name": "test"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN n.nonexistent AS prop")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(null));
}
