//! Extended Regression Tests — simple smoke coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Minimal happy-path smoke tests for each primary feature area.

use nexus_core::testing::setup_test_engine;
use serde_json::json;

#[test]
fn regression_simple_create() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine.execute_cypher("CREATE (n:T)").unwrap();
}

#[test]
fn regression_simple_match() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    engine.execute_cypher("MATCH (n:T) RETURN n").unwrap();
}

#[test]
fn regression_simple_count() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(r.rows[0].values[0], json!(1));
}

#[test]
fn regression_simple_prop() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({"n": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN n.n AS v")
        .unwrap();
    assert_eq!(r.rows[0].values[0], json!(1));
}

#[test]
fn regression_simple_two_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(r.rows[0].values[0], json!(2));
}

#[test]
fn regression_simple_rel() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let a = engine
        .create_node(vec!["N".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["N".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    engine
        .create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH ()-[r:R]->() RETURN count(r) AS c")
        .unwrap();
    assert_eq!(r.rows[0].values[0], json!(1));
}

#[test]
fn regression_simple_labels() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN labels(n) AS l")
        .unwrap();
    assert!(!r.rows.is_empty());
}

#[test]
fn regression_simple_keys() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({"a": 1}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN keys(n) AS k")
        .unwrap();
    assert!(!r.rows.is_empty());
}

#[test]
fn regression_simple_id() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) RETURN id(n) AS i")
        .unwrap();
    assert!(!r.rows.is_empty());
}

#[test]
fn regression_simple_where() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({"v": 1}))
        .unwrap();
    engine
        .create_node(vec!["T".to_string()], json!({"v": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();
    let r = engine
        .execute_cypher("MATCH (n:T) WHERE n.v = 1 RETURN count(n) AS c")
        .unwrap();
    assert_eq!(r.rows[0].values[0], json!(1));
}
