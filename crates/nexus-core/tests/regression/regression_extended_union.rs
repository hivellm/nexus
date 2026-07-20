//! Extended Regression Tests — UNION clause coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers UNION / UNION ALL, empty sides, column preservation, multi-part unions.

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_union_basic() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn regression_union_all() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.value AS value
         UNION ALL
         MATCH (b:B) RETURN b.value AS value",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn regression_union_empty_left() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["B".to_string()], json!({"value": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:NonExistent) RETURN a
         UNION
         MATCH (b:B) RETURN b",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn regression_union_empty_right() {
    // Use isolated engine to avoid interference from parallel tests
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a
         UNION
         MATCH (b:NonExistent) RETURN b",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn regression_union_both_empty() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:NonExistent1) RETURN a
         UNION
         MATCH (b:NonExistent2) RETURN b",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn regression_union_different_types() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": "text"}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 123}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn regression_union_with_count() {
    // Use isolated engine to avoid interference from parallel tests
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..3 {
        engine
            .create_node(vec!["A".to_string()], json!({"id": i}))
            .unwrap();
    }
    for i in 0..2 {
        engine
            .create_node(vec!["B".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN count(a) AS count
         UNION
         MATCH (b:B) RETURN count(b) AS count",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn regression_union_preserves_columns() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
        )
        .unwrap();
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "value");
}

#[test]
fn regression_union_multiple() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"n": 1}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"n": 2}))
        .unwrap();
    engine
        .create_node(vec!["C".to_string()], json!({"n": 3}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.n AS n
         UNION
         MATCH (b:B) RETURN b.n AS n
         UNION
         MATCH (c:C) RETURN c.n AS n",
        )
        .unwrap();
    assert!(result.rows.len() >= 3);
}

#[test]
fn regression_union_with_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 2);
}
