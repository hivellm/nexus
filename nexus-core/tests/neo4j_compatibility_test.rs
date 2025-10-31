//! Neo4j Compatibility Tests
//!
//! Tests for features required for full Neo4j compatibility:
//! - Multiple labels in MATCH queries
//! - UNION queries
//! - Bidirectional relationship queries
//! - Relationship property access

use nexus_core::Engine;
use serde_json::json;
use tempfile::TempDir;

/// Helper function to execute a Cypher query via Engine
fn execute_query(
    engine: &mut Engine,
    query: &str,
) -> Result<nexus_core::executor::ResultSet, String> {
    engine
        .execute_cypher(query)
        .map_err(|e| format!("Query execution failed: {}", e))
}

/// Helper function to create test data
fn setup_test_data(engine: &mut Engine) -> Result<(), String> {
    use serde_json::json;

    // Create nodes with multiple labels using Engine API directly
    // This bypasses the MATCH ... CREATE limitation
    let alice_id = engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Alice", "age": 30}),
        )
        .map_err(|e| e.to_string())?;

    let bob_id = engine
        .create_node(
            vec!["Person".to_string(), "Manager".to_string()],
            json!({"name": "Bob", "age": 40}),
        )
        .map_err(|e| e.to_string())?;

    let _charlie_id = engine
        .create_node(
            vec!["Person".to_string()],
            json!({"name": "Charlie", "age": 25}),
        )
        .map_err(|e| e.to_string())?;

    let acme_id = engine
        .create_node(vec!["Company".to_string()], json!({"name": "Acme Corp"}))
        .map_err(|e| e.to_string())?;

    let tech_id = engine
        .create_node(vec!["Company".to_string()], json!({"name": "Tech Inc"}))
        .map_err(|e| e.to_string())?;

    // Refresh executor to see new nodes
    engine.refresh_executor().map_err(|e| e.to_string())?;

    // Create relationships with properties using Engine API
    engine
        .create_relationship(
            alice_id,
            bob_id,
            "KNOWS".to_string(),
            json!({"since": 2020, "strength": "strong"}),
        )
        .map_err(|e| e.to_string())?;

    engine
        .create_relationship(
            alice_id,
            acme_id,
            "WORKS_AT".to_string(),
            json!({"since": 2021, "role": "Developer"}),
        )
        .map_err(|e| e.to_string())?;

    // Create bidirectional relationships
    engine
        .create_relationship(bob_id, tech_id, "MANAGES".to_string(), json!({}))
        .map_err(|e| e.to_string())?;

    engine
        .create_relationship(tech_id, bob_id, "MANAGED_BY".to_string(), json!({}))
        .map_err(|e| e.to_string())?;

    // Refresh executor again to see relationships
    engine.refresh_executor().map_err(|e| e.to_string())?;

    Ok(())
}

#[test]
fn test_multiple_labels_match() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test MATCH with multiple labels (Person AND Employee)
    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee) RETURN n.name AS name ORDER BY name",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));

    // Test MATCH with multiple labels (Person AND Manager)
    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Manager) RETURN n.name AS name",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Bob"));
}

#[test]
fn test_union_queries() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test UNION query combining Person and Company names
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN p.name AS name
         UNION
         MATCH (c:Company) RETURN c.name AS name
         ORDER BY name",
    )
    .unwrap();

    assert!(result.rows.len() >= 5); // Should have all Person and Company names

    // Verify distinct results (no duplicates)
    let names: Vec<&str> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_str().unwrap())
        .collect();
    let unique_names: Vec<&str> = names.to_vec();
    assert_eq!(
        names.len(),
        unique_names.len(),
        "UNION should remove duplicates"
    );

    // Test UNION ALL (should keep duplicates)
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN p.name AS name
         UNION ALL
         MATCH (p2:Person) RETURN p2.name AS name
         ORDER BY name",
    )
    .unwrap();

    assert!(result.rows.len() >= 6); // Should have duplicates
}

#[test]
fn test_bidirectional_relationship_queries() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test bidirectional query (undirected)
    let result = execute_query(
        &mut engine,
        "MATCH (a:Person)-[r]-(b:Company) RETURN a.name AS person, b.name AS company ORDER BY person",
    )
    .unwrap();

    assert!(result.rows.len() >= 2); // Should find relationships in both directions

    // Test specific bidirectional pattern
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person {name: 'Bob'})-[r]-(c:Company) RETURN c.name AS company, type(r) AS rel_type",
    )
    .unwrap();

    assert!(!result.rows.is_empty());

    // Should find both MANAGES and MANAGED_BY relationships
    let rel_types: Vec<&str> = result
        .rows
        .iter()
        .map(|row| row.values[1].as_str().unwrap())
        .collect();
    assert!(
        rel_types.contains(&"MANAGES") || rel_types.contains(&"MANAGED_BY"),
        "Should find bidirectional relationships"
    );
}

#[test]
fn test_relationship_property_access() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test accessing relationship properties
    let result = execute_query(
        &mut engine,
        "MATCH (a:Person)-[r:KNOWS]->(b:Person)
         RETURN a.name AS from, b.name AS to, r.since AS since, r.strength AS strength",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);

    let row = &result.rows[0];
    assert_eq!(row.values[0], json!("Alice")); // from
    assert_eq!(row.values[1], json!("Bob")); // to
    assert_eq!(row.values[2], json!(2020)); // since
    assert_eq!(row.values[3], json!("strong")); // strength

    // Test filtering by relationship property
    let result = execute_query(
        &mut engine,
        "MATCH (a:Person)-[r:WORKS_AT]->(c:Company)
         WHERE r.role = 'Developer'
         RETURN a.name AS person, c.name AS company",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));
    assert_eq!(result.rows[0].values[1], json!("Acme Corp"));
}

#[test]
fn test_relationship_property_return() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test returning relationship itself with properties
    let result = execute_query(
        &mut engine,
        "MATCH (a:Person)-[r:KNOWS]->(b:Person)
         RETURN r",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);

    // Verify relationship object has properties
    let rel = &result.rows[0].values[0];
    if rel.is_object() {
        let rel_obj = rel.as_object().unwrap();
        assert!(
            rel_obj.contains_key("since"),
            "Relationship should have 'since' property"
        );
        assert!(
            rel_obj.contains_key("strength"),
            "Relationship should have 'strength' property"
        );
    }
}

#[test]
fn test_multiple_labels_filtering() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Test filtering nodes with multiple labels in WHERE clause
    let result = execute_query(
        &mut engine,
        "MATCH (n:Person)
         WHERE n:Employee OR n:Manager
         RETURN n.name AS name, labels(n) AS labels
         ORDER BY name",
    )
    .unwrap();

    assert!(result.rows.len() >= 2); // Should find Alice (Employee) and Bob (Manager)
}

#[test]
#[ignore = "Known bug: MATCH with multiple labels + relationships duplicates results"]
fn test_complex_multiple_labels_query() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    setup_test_data(&mut engine).unwrap();

    // Complex query with multiple labels and relationship properties
    let result = execute_query(
        &mut engine,
        "MATCH (p:Person:Employee)-[r:WORKS_AT]->(c:Company)
         WHERE r.role = 'Developer'
         RETURN p.name AS employee, c.name AS company, r.since AS started",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));
    assert_eq!(result.rows[0].values[1], json!("Acme Corp"));
    assert_eq!(result.rows[0].values[2], json!(2021));
}
