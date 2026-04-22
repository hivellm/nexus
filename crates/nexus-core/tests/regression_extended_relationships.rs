//! Extended Regression Tests — relationship coverage.
//!
//! Split from `regression_extended.rs` (tier-3 oversized-module refactor).
//! Covers relationship CRUD, direction, property handling and filters.

use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;

#[test]
fn regression_rel_basic_creation() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

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
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_rel_with_one_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "KNOWS".to_string(), json!({"since": 2020}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN r.since AS since")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(2020));
}

#[test]
fn regression_rel_with_three_properties() {
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
        .create_relationship(a, b, "REL".to_string(), json!({"a": 1, "b": 2, "c": 3}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN keys(r) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 3);
}

#[test]
fn regression_rel_outgoing_direction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "TO".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (a {name: 'A'})-[r:TO]->(b) RETURN b.name AS name")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("B"));
}

#[test]
fn regression_rel_incoming_direction() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "TO".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (b {name: 'B'})<-[r:TO]-(a) RETURN a.name AS name")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("A"));
}

#[test]
fn regression_rel_bidirectional_pattern() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "LINKED".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (a {name: 'A'})-[r:LINKED]-(b) RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_rel_type_function() {
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
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN type(r) AS type")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("KNOWS"));
}

#[test]
fn regression_rel_id_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let rel_id = engine
        .create_relationship(a, b, "REL".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN id(r) AS id")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(rel_id));
}

#[test]
fn regression_rel_keys_function() {
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
        .create_relationship(a, b, "REL".to_string(), json!({"prop1": 1, "prop2": 2}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN keys(r) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 2);
}

#[test]
fn regression_rel_empty_properties() {
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
        .create_relationship(a, b, "REL".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN keys(r) AS keys")
        .unwrap();
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert_eq!(keys.len(), 0);
}

#[test]
fn regression_rel_string_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"desc": "test"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r.desc AS desc")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("test"));
}

#[test]
fn regression_rel_int_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"count": 42}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r.count AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(42));
}

#[test]
fn regression_rel_bool_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"active": true}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r.active AS active")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(true));
}

#[test]
fn regression_rel_float_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"weight": 0.75}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r.weight AS weight")
        .unwrap();
    assert!(result.rows[0].values[0].as_f64().is_some());
}

#[test]
fn regression_rel_null_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"exists": "yes"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r.nonexistent AS prop")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(null));
}

#[test]
fn regression_rel_match_any_type() {
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
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(2));
}

#[test]
fn regression_rel_with_labeled_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let person = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    let company = engine
        .create_node(vec!["Company".to_string()], json!({"name": "Acme"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(person, company, "WORKS_AT".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (p:Person)-[r:WORKS_AT]->(c:Company) RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_rel_return_source_target() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"id": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"id": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (a)-[r:REL]->(b) RETURN a.id AS source, b.id AS target")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!("A"));
    assert_eq!(result.rows[0].values[1], json!("B"));
}

#[test]
fn regression_rel_10_relationships() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    for i in 0..10 {
        engine
            .create_relationship(a, b, format!("REL_{}", i), json!({}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(10));
}

#[test]
fn regression_rel_self_loop() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, a, "SELF".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH (a)-[r:SELF]->(a) RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_rel_where_property() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"level": 5}))
        .unwrap();
    engine
        .create_relationship(a, b, "REL".to_string(), json!({"level": 10}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() WHERE r.level = 5 RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(1));
}

#[test]
fn regression_rel_where_greater() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    for i in 0..10 {
        engine
            .create_relationship(a, b, "REL".to_string(), json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() WHERE r.value > 5 RETURN count(r) AS count")
        .unwrap();
    assert_eq!(result.rows[0].values[0], json!(4));
}

#[test]
fn regression_rel_limit() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    for i in 0..20 {
        engine
            .create_relationship(a, b, "REL".to_string(), json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r:REL]->() RETURN r LIMIT 5")
        .unwrap();
    assert_eq!(result.rows.len(), 5);
}

#[test]
fn regression_rel_distinct_types() {
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
        .create_relationship(a, b, "TYPE1".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(a, b, "TYPE2".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(a, b, "TYPE3".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = engine
        .execute_cypher("MATCH ()-[r]->() RETURN DISTINCT type(r) AS type")
        .unwrap();
    assert!(result.rows.len() >= 3);
}
