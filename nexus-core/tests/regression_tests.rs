//! Regression tests for Neo4j compatibility fixes
//!
//! These tests ensure that previously fixed bugs do not reappear.
//! Each test is named after the issue it prevents.

use nexus_core::Engine;
use serde_json::json;
use tempfile::TempDir;

/// Regression test: UNION operator returns Null values
/// Fixed in commit a4d399f
#[test]
fn regression_union_null_values() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine
        .create_node(vec!["Company".to_string()], json!({"name": "Acme"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Test UNION query
    let result = engine
        .execute_cypher(
            "MATCH (p:Person) RETURN p.name AS name
             UNION
             MATCH (c:Company) RETURN c.name AS name",
        )
        .unwrap();

    // Should have at least 2 rows with actual values (not Null)
    assert!(result.rows.len() >= 2);

    // Filter out null values and verify we have at least 2 non-null results
    let non_null_count = result
        .rows
        .iter()
        .filter(|row| row.values[0] != json!(null))
        .count();
    assert!(
        non_null_count >= 2,
        "Should have at least 2 non-null results, got {}",
        non_null_count
    );

    // Verify the actual values are present
    let values: Vec<String> = result
        .rows
        .iter()
        .filter_map(|row| row.values[0].as_str().map(|s| s.to_string()))
        .collect();
    assert!(
        values.contains(&"Alice".to_string()),
        "Should contain Alice"
    );
    assert!(values.contains(&"Acme".to_string()), "Should contain Acme");
}

/// Regression test: MATCH with multiple labels returns wrong count
/// Fixed in commit fdd3e76
///
/// NOTE: Uses with_isolated_catalog to prevent test contamination from
/// shared test catalog state (labels like Person, Employee may exist from other tests)
#[test]
fn regression_multiple_labels_intersection() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_isolated_catalog(dir.path()).unwrap();

    // Create nodes with different label combinations
    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Alice"}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Manager".to_string()],
            json!({"name": "Bob"}),
        )
        .unwrap();
    engine
        .create_node(vec!["Person".to_string()], json!({"name": "Charlie"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query for Person AND Employee (should be intersection, not union)
    let result = engine
        .execute_cypher("MATCH (n:Person:Employee) RETURN n.name AS name")
        .unwrap();

    // Should find only Alice (has both Person and Employee labels)
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));
}

/// Regression test: id() function returns Null
/// Fixed in commit a4d399f
#[test]
fn regression_id_function_null() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create a node
    let node_id = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query with id() function
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN id(n) AS id")
        .unwrap();

    // Should return actual ID, not Null
    assert_eq!(result.rows.len(), 1);
    assert_ne!(result.rows[0].values[0], json!(null));
    assert_eq!(result.rows[0].values[0], json!(node_id));
}

/// Regression test: keys() function returns empty array
/// Fixed in commit 28879da
#[test]
fn regression_keys_function_empty() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create a node with properties
    engine
        .create_node(
            vec!["Person".to_string()],
            json!({"name": "Alice", "age": 30, "city": "NYC"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query with keys() function
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN keys(n) AS keys")
        .unwrap();

    // Should return property keys, not empty array
    assert_eq!(result.rows.len(), 1);
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert!(keys.len() >= 3); // at least name, age, city

    // Keys should be sorted and not include internal fields
    let key_names: Vec<&str> = keys.iter().map(|v| v.as_str().unwrap()).collect();
    assert!(key_names.contains(&"name"));
    assert!(key_names.contains(&"age"));
    assert!(key_names.contains(&"city"));
    assert!(!key_names.iter().any(|k| k.starts_with('_')));
}

/// Regression test: Relationship properties not accessible
/// Fixed in commit 87a75fc
#[test]
fn regression_relationship_properties() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes and relationship with properties
    let alice_id = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    let bob_id = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Bob"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(
            alice_id,
            bob_id,
            "KNOWS".to_string(),
            json!({"since": 2020, "strength": "strong"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query relationship properties
    let result = engine
        .execute_cypher(
            "MATCH (a:Person)-[r:KNOWS]->(b:Person)
             RETURN r.since AS since, r.strength AS strength",
        )
        .unwrap();

    // Should return relationship properties, not Null
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!(2020));
    assert_eq!(result.rows[0].values[1], json!("strong"));
}

/// Regression test: CREATE clause doesn't persist data
/// Fixed in commits e6a15d3 and a4d399f
#[test]
fn regression_create_persistence() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create node via Cypher
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30})")
        .unwrap();

    // Query should find the created node
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN n.name AS name, n.age AS age")
        .unwrap();

    // May include nodes from previous tests - verify at least one row matches
    assert!(
        !result.rows.is_empty(),
        "Expected at least 1 Person node, got {}",
        result.rows.len()
    );
    // Verify that at least one row has the expected values
    let found_alice = result
        .rows
        .iter()
        .any(|row| row.values[0] == json!("Alice") && row.values[1] == json!(30));
    assert!(found_alice, "Should find Alice with age 30");
}

/// Regression test: CREATE with multiple labels fails
/// Fixed in commit e6a15d3
#[test]
fn regression_create_multiple_labels() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create node with multiple labels
    engine
        .execute_cypher("CREATE (n:Person:Employee {name: 'Alice'})")
        .unwrap();

    // Should be able to query with both labels
    let result1 = engine
        .execute_cypher("MATCH (n:Person) RETURN n.name AS name")
        .unwrap();
    // May include nodes from previous tests - accept >= 1
    assert!(
        !result1.rows.is_empty(),
        "Expected at least 1 Person node, got {}",
        result1.rows.len()
    );

    let result2 = engine
        .execute_cypher("MATCH (n:Employee) RETURN n.name AS name")
        .unwrap();
    // May include nodes from previous tests - accept >= 1
    assert!(
        !result2.rows.is_empty(),
        "Expected at least 1 Employee node, got {}",
        result2.rows.len()
    );

    let result3 = engine
        .execute_cypher("MATCH (n:Person:Employee) RETURN n.name AS name")
        .unwrap();
    // May include nodes from previous tests - accept >= 1
    assert!(
        !result3.rows.is_empty(),
        "Expected at least 1 Person:Employee node, got {}",
        result3.rows.len()
    );
}

/// Regression test: Bidirectional relationships not working
/// Fixed in commit 87a75fc
#[test]
fn regression_bidirectional_relationships() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes and bidirectional relationships
    let a_id = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b_id = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a_id, b_id, "LINKS".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(b_id, a_id, "LINKS".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query bidirectional pattern
    let result = engine
        .execute_cypher(
            "MATCH (a:Node)-[r:LINKS]-(b:Node) WHERE a.name = 'A' RETURN b.name AS name",
        )
        .unwrap();

    // Should find B in both directions
    assert!(!result.rows.is_empty());
}

/// Regression test: Engine::new() temporary directory bug
/// Fixed in commit (fix-engine-tests)
#[test]
fn regression_engine_tempdir_lifecycle() {
    // This test ensures Engine::new() keeps temp directory alive
    let mut engine = Engine::new().unwrap();

    // Create a node
    let node_id = engine
        .create_node(vec!["Test".to_string()], json!({"value": 42}))
        .unwrap();

    // Should be able to read it back (temp dir still exists)
    let node = engine.get_node(node_id).unwrap();
    assert!(node.is_some());
}
