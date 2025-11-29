//! Integration tests for Cypher write operations
//!
//! Tests for MERGE, SET, DELETE, and REMOVE clauses

use nexus_core::{Engine, Error};
use nexus_core::executor::ResultSet;
use serde_json::Value;

fn create_engine() -> Result<Engine, Error> {
    let mut engine = Engine::new()?;
    // Ensure clean database for each test
    let _ = engine.execute_cypher("MATCH (n) DETACH DELETE n");
    Ok(engine)
}

fn extract_first_row_value(result: &ResultSet, column_idx: usize) -> Option<&Value> {
    result.rows.get(0).and_then(|row| row.values.get(column_idx))
}

#[test]
fn merge_creates_node_when_missing_and_reuses_existing() -> Result<(), Error> {
    let mut engine = create_engine()?;

    // First MERGE should create the node
    let result = engine.execute_cypher(
        "MERGE (n:Person {email: 'alice@example.com'})\n         ON CREATE SET n.created = true\n         RETURN n",
    )?;
    assert_eq!(result.rows.len(), 1, "MERGE should return one row on creation");
    let node = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .expect("MERGE should return node object");
    assert_eq!(node.get("email"), Some(&Value::String("alice@example.com".into())));
    assert_eq!(node.get("created"), Some(&Value::Bool(true)));

    // Second MERGE should match existing node without creating a duplicate
    let result = engine.execute_cypher(
        "MERGE (n:Person {email: 'alice@example.com'})\n         ON MATCH SET n.last_seen = '2025-11-11'\n         RETURN n",
    )?;
    assert_eq!(result.rows.len(), 1, "MERGE should still return one row when matching");
    let node = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .expect("MERGE match should return node");
    assert_eq!(node.get("email"), Some(&Value::String("alice@example.com".into())));
    assert_eq!(node.get("last_seen"), Some(&Value::String("2025-11-11".into())));
    assert_eq!(node.get("created"), Some(&Value::Bool(true)), "created flag should be preserved");

    // Verify no duplicate nodes exist
    let result = engine.execute_cypher("MATCH (n:Person) RETURN count(n) AS total")?;
    let total = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert_eq!(total, 1, "MERGE should keep data idempotent");

    Ok(())
}

#[test]
fn set_updates_properties_and_labels() -> Result<(), Error> {
    let mut engine = create_engine()?;
    engine.execute_cypher("CREATE (n:Person {name: 'Alice', age: 30})")?;

    let result = engine.execute_cypher(
        "MATCH (n:Person {name: 'Alice'})\n         SET n.age = 31, n.city = 'NYC', n:Employee\n         RETURN n",
    )?;

    let node = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .expect("SET should return the updated node");
    assert_eq!(node.get("age"), Some(&Value::Number(31.into())));
    assert_eq!(node.get("city"), Some(&Value::String("NYC".into())));
    let labels = node
        .get("_nexus_labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(labels.iter().any(|label| label == "Employee"), "Employee label should be attached");

    Ok(())
}

#[test]
fn delete_and_detach_delete_remove_nodes() -> Result<(), Error> {
    let mut engine = create_engine()?;
    engine.execute_cypher(
        "CREATE (a:Person {name: 'A'})-[:KNOWS]->(b:Person {name: 'B'})",
    )?;

    // Regular DELETE should fail on node with relationship but work on isolated node
    let delete_result = engine.execute_cypher("MATCH (n:Person {name: 'B'}) DELETE n");
    assert!(delete_result.is_err(), "DELETE should fail when relationships exist");

    // DETACH DELETE should remove both nodes
    engine.execute_cypher("MATCH (n:Person) DETACH DELETE n")?;
    let result = engine.execute_cypher("MATCH (n) RETURN count(n) AS total")?;
    let total = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .and_then(|obj| obj.get("total"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    assert_eq!(total, 0, "DETACH DELETE should clear all nodes");

    Ok(())
}

#[test]
fn remove_properties_and_labels() -> Result<(), Error> {
    let mut engine = create_engine()?;
    engine.execute_cypher("CREATE (n:Person:Employee {name: 'Carol', age: 40})")?;

    let result = engine.execute_cypher(
        "MATCH (n:Person {name: 'Carol'})\n         REMOVE n.age, n:Employee\n         RETURN n",
    )?;

    let node = extract_first_row_value(&result, 0)
        .and_then(Value::as_object)
        .expect("REMOVE should return the altered node");
    assert!(node.get("age").is_none(), "age property should be removed");

    let labels = node
        .get("_nexus_labels")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    assert!(labels.iter().all(|label| label != "Employee"), "Employee label should be removed");

    Ok(())
}
