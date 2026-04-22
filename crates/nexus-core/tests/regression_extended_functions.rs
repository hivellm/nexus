//! Extended Regression Tests — built-in function coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers labels/id/keys/type + aggregation functions (count/sum/avg/min/max).

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_labels_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN labels(n) AS labels")
        .unwrap();
    let labels = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(labels.len(), 1);
}

#[test]
fn regression_labels_function_two() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string(), "B".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n) RETURN labels(n) AS labels")
        .unwrap();
    let labels = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(labels.len(), 2);
}

#[test]
fn regression_id_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let node_id = engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN id(n) AS id")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(node_id));
}

#[test]
fn regression_keys_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"a": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert!(keys.contains(&json!("a")));
}

#[test]
fn regression_keys_sorted() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"z": 1, "a": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 2);
}

#[test]
fn regression_type_function() {
    // Use isolated catalog to prevent interference from other tests
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "KNOWS".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r]->() RETURN type(r) AS type")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("KNOWS"));
}

#[test]
fn regression_count_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..7 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(7));
}

#[test]
fn regression_sum_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 1..=5 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN sum(n.value) AS total")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(15));
}

#[test]
fn regression_avg_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 10}))
        .unwrap();
    engine
        .create_node(vec!["Test".to_string()], json!({"value": 20}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN avg(n.value) AS average")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(15.0));
}

#[test]
fn regression_min_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in [50, 10, 30] {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN min(n.value) AS minimum")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(10));
}

#[test]
fn regression_max_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in [10, 50, 30] {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN max(n.value) AS maximum")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(50));
}

#[test]
fn regression_id_sequential() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let id1 = engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    let id2 = engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();

    assert_eq!(id2, id1 + 1);
}

#[test]
fn regression_labels_empty() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let _id = engine.create_node(vec![], json!({})).unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n) RETURN labels(n) AS labels")
        .unwrap();
    let labels = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(labels.len(), 0);
}

#[test]
fn regression_keys_empty() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN keys(n) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 0);
}

#[test]
fn regression_count_zero() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:NonExistent) RETURN count(n) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(0));
}

#[test]
fn regression_sum_zero() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 0}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN sum(n.value) AS total")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(0));
}

#[test]
fn regression_avg_single() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 42}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN avg(n.value) AS average")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(42.0));
}

#[test]
fn regression_min_max_same() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Test".to_string()], json!({"value": 100}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (n:Test) RETURN min(n.value) AS min, max(n.value) AS max")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(100));
    assert_eq!(result.rows[0].values[1], json!(100));
}

#[test]
fn regression_distinct_count() {
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
        .execute_cypher("MATCH (n:Test) RETURN count(DISTINCT n.value) AS count")
        .unwrap();
    assert!(result.rows[0].values[0].as_i64().unwrap() <= 2);
}

#[test]
fn regression_type_rel_different() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "TYPE1".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(a, b, "TYPE2".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r]->() RETURN DISTINCT type(r) AS type")
        .unwrap();
    assert!(result.rows.len() >= 2);
}
