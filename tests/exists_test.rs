//! Integration tests for EXISTS subqueries
//!
//! Tests for EXISTS expressions in WHERE clauses

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
fn test_exists_simple_pattern() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'})",
    )?;

    // Test EXISTS with simple pattern
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE EXISTS { (n) } RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0].values[0].as_object().unwrap().get("properties").unwrap().as_object().unwrap().get("name").unwrap().as_str(), Some("Alice"));
    
    Ok(())
}

#[test]
fn test_exists_with_relationship() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data with relationships
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (p1)-[:KNOWS]->(p2)",
    )?;

    // Test EXISTS with relationship pattern
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE EXISTS { (n)-[:KNOWS]->(m) } RETURN n.name AS name ORDER BY n.name",
    )?;

    // Should return only Alice (who has a KNOWS relationship)
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_object().unwrap().get("properties").unwrap().as_object().unwrap().get("name").unwrap().as_str(), Some("Alice"));
    
    Ok(())
}

#[test]
fn test_exists_filters_nodes() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', age: 30}), (p2:Person {name: 'Bob', age: 25}), (p3:Person {name: 'Charlie', age: 35})",
    )?;

    // Test EXISTS filters nodes that match the pattern
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE EXISTS { (n) } AND n.age > 28 RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    Ok(())
}

#[test]
fn test_exists_with_variable() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'})",
    )?;

    // Test EXISTS with variable reference
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE EXISTS { (n) } RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    Ok(())
}

#[test]
fn test_exists_combined_with_other_conditions() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice', active: true}), (p2:Person {name: 'Bob', active: false})",
    )?;

    // Test EXISTS combined with other WHERE conditions
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE EXISTS { (n) } AND n.active = true RETURN n.name AS name",
    )?;

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0].as_object().unwrap().get("properties").unwrap().as_object().unwrap().get("name").unwrap().as_str(), Some("Alice"));
    
    Ok(())
}

#[test]
fn test_exists_returns_boolean() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'})",
    )?;

    // Test that EXISTS returns a boolean value
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN EXISTS { (n) } AS exists_pattern ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 1);
    // EXISTS should return a boolean
    assert_eq!(result.rows[0].values[0].as_bool(), Some(true));
    
    Ok(())
}

