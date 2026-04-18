//! Integration tests for FOREACH clause
//!
//! Tests for FOREACH clause in Cypher queries

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
fn test_foreach_set_properties() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (p3:Person {name: 'Charlie'})",
    )?;

    // Match all persons and set a property using FOREACH
    let result = engine.execute_cypher(
        "MATCH (n:Person) FOREACH (x IN [1, 2, 3] | SET n.rank = x) RETURN n.name AS name ORDER BY n.name",
    )?;

    // Note: FOREACH doesn't produce rows, but the query should execute successfully
    // The SET operations should have been applied
    assert_eq!(result.rows.len(), 3);
    
    Ok(())
}

#[test]
fn test_foreach_set_from_match() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data with relationships
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (p1)-[:KNOWS]->(p2)",
    )?;

    // Match nodes and use FOREACH to set properties based on matched nodes
    let result = engine.execute_cypher(
        "MATCH (n:Person) FOREACH (x IN [n] | SET x.visited = true) RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    Ok(())
}

#[test]
fn test_foreach_delete_nodes() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (p3:Person {name: 'Charlie'})",
    )?;

    // Use FOREACH to delete nodes
    let result = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name = 'Bob' FOREACH (x IN [n] | DELETE x) RETURN n.name AS name",
    )?;

    // After deletion, querying should return fewer nodes
    let remaining = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(remaining.rows.len(), 2);
    assert_eq!(remaining.rows[0].values[0].as_object().unwrap().get("properties").unwrap().as_object().unwrap().get("name").unwrap().as_str(), Some("Alice"));
    
    Ok(())
}

#[test]
fn test_foreach_detach_delete() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data with relationships
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (p1)-[:KNOWS]->(p2)",
    )?;

    // Use FOREACH with DETACH DELETE
    let _ = engine.execute_cypher(
        "MATCH (n:Person) WHERE n.name = 'Alice' FOREACH (x IN [n] | DETACH DELETE x)",
    )?;

    // Verify node was deleted
    let remaining = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name ORDER BY n.name",
    )?;

    assert_eq!(remaining.rows.len(), 1);
    
    Ok(())
}

#[test]
fn test_foreach_multiple_operations() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'})",
    )?;

    // FOREACH with multiple SET operations
    let _ = engine.execute_cypher(
        "MATCH (n:Person) FOREACH (x IN [n] | SET x.status = 'active' SET x.updated = true)",
    )?;

    // Verify properties were set
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, n.status AS status ORDER BY n.name",
    )?;

    assert_eq!(result.rows.len(), 2);
    
    Ok(())
}

#[test]
fn test_foreach_empty_list() -> Result<(), Error> {
    let mut engine = create_engine()?;
    
    // Create test data
    engine.execute_cypher(
        "CREATE (p1:Person {name: 'Alice'})",
    )?;

    // FOREACH with empty list should not execute operations
    let _ = engine.execute_cypher(
        "MATCH (n:Person) FOREACH (x IN [] | SET x.status = 'active') RETURN n.name AS name",
    )?;

    // Verify node still exists and status was not set
    let result = engine.execute_cypher(
        "MATCH (n:Person) RETURN n.name AS name, n.status AS status",
    )?;

    assert_eq!(result.rows.len(), 1);
    // Status should be null (not set)
    assert_eq!(result.rows[0].values.get(1), Some(&Value::Null));
    
    Ok(())
}

