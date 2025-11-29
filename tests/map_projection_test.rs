//! Integration tests for Map Projections
//!
//! Tests for map projection syntax in Cypher queries

use nexus_core::{Engine, Error};
use nexus_core::executor::ResultSet;
use serde_json::Value;

fn create_engine() -> Result<Engine, Error> {
    let mut engine = Engine::new()?;
    // Ensure clean database for each test
    let _ = engine.execute_cypher("MATCH (n) DETACH DELETE n");
    Ok(engine)
}

#[test]
fn test_map_projection_simple() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30, city: 'NYC'})",
    )?;

    // Test simple map projection
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {.name, .age} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    // Check that the result is a map with name and age
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert!(map.contains_key("name"));
        assert!(map.contains_key("age"));
        assert_eq!(map.get("name").and_then(Value::as_str), Some("Alice"));
        assert_eq!(map.get("age").and_then(Value::as_u64), Some(30));
    } else {
        panic!("Expected map projection result");
    }
    
    Ok(())
}

#[test]
fn test_map_projection_with_alias() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30})",
    )?;

    // Test map projection with alias
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {.name AS fullName, .age AS years} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    // Check that aliases are used
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert!(map.contains_key("fullName"));
        assert!(map.contains_key("years"));
        assert_eq!(map.get("fullName").and_then(Value::as_str), Some("Alice"));
        assert_eq!(map.get("years").and_then(Value::as_u64), Some(30));
    } else {
        panic!("Expected map projection result");
    }
    
    Ok(())
}

#[test]
fn test_map_projection_with_virtual_keys() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30})",
    )?;

    // Test map projection with virtual keys
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {fullName: n.name, doubleAge: n.age * 2} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    // Check that virtual keys are evaluated
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert!(map.contains_key("fullName"));
        assert!(map.contains_key("doubleAge"));
        assert_eq!(map.get("fullName").and_then(Value::as_str), Some("Alice"));
        assert_eq!(map.get("doubleAge").and_then(Value::as_u64), Some(60));
    } else {
        panic!("Expected map projection result");
    }
    
    Ok(())
}

#[test]
fn test_map_projection_mixed() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30, city: 'NYC'})",
    )?;

    // Test map projection with mixed property and virtual keys
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {.name, .age, location: n.city, isAdult: n.age >= 18} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    // Check that all keys are present
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert!(map.contains_key("name"));
        assert!(map.contains_key("age"));
        assert!(map.contains_key("location"));
        assert!(map.contains_key("isAdult"));
        assert_eq!(map.get("location").and_then(Value::as_str), Some("NYC"));
        assert_eq!(map.get("isAdult").and_then(Value::as_bool), Some(true));
    } else {
        panic!("Expected map projection result");
    }
    
    Ok(())
}

#[test]
fn test_map_projection_multiple_nodes() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30}), (p2:Person {name: 'Bob', age: 25})",
    )?;

    // Test map projection with multiple nodes
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {.name, .age} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    // Check first row
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert_eq!(map.get("name").and_then(Value::as_str), Some("Alice"));
    }
    
    // Check second row
    if let Some(Value::Object(map)) = result.rows[1].values.get(0) {
        assert_eq!(map.get("name").and_then(Value::as_str), Some("Bob"));
    }
    
    Ok(())
}

#[test]
fn test_map_projection_missing_properties() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data with missing properties
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'})",
    )?;

    // Test map projection with missing properties (should return NULL)
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n {.name, .age, .city} AS person ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    
    // Check that missing properties are NULL
    if let Some(Value::Object(map)) = result.rows[0].values.get(0) {
        assert_eq!(map.get("name").and_then(Value::as_str), Some("Alice"));
        assert_eq!(map.get("age"), Some(&Value::Null));
        assert_eq!(map.get("city"), Some(&Value::Null));
    } else {
        panic!("Expected map projection result");
    }
    
    Ok(())
}

