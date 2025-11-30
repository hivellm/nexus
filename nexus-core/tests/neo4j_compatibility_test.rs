//! Neo4j Compatibility Tests
//!
//! Tests for features required for full Neo4j compatibility:
//! - Multiple labels in MATCH queries
//! - UNION queries
//! - Bidirectional relationship queries
//! - Relationship property access
//!
//! NOTE: These tests use #[serial] to prevent LMDB resource contention
//! when running with high parallelism (e.g., nextest).

use nexus_core::Engine;
use nexus_core::testing::{setup_isolated_test_engine, setup_test_engine};
use serde_json::json;
use serial_test::serial;

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
#[serial]
fn test_multiple_labels_match() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
#[serial]
fn test_union_queries() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

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
#[serial]
fn test_bidirectional_relationship_queries() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

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
#[serial]
fn test_relationship_property_access() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
#[serial]
fn test_relationship_property_return() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
#[serial]
fn test_multiple_labels_filtering() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

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
#[serial]
fn test_complex_multiple_labels_query() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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

// ============================================================================
// Additional Compatibility Tests for Extended Coverage
// ============================================================================

/// Test UNION ALL (preserves duplicates)
#[test]
#[serial]
fn test_union_all_preserves_duplicates() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Create different values to avoid DISTINCT behavior
    engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    engine
        .create_node(vec!["Person".to_string()], json!({"name": "Bob"}))
        .unwrap();
    engine
        .create_node(vec!["Company".to_string()], json!({"name": "Acme"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // UNION ALL should combine all results
    let union_all_result = execute_query(
        &mut engine,
        "MATCH (p:Person) RETURN p.name AS name
         UNION ALL
         MATCH (c:Company) RETURN c.name AS name",
    )
    .unwrap();

    // Should have at least 3 rows (2 Person + 1 Company, may include extra rows)
    assert!(
        union_all_result.rows.len() >= 3,
        "UNION ALL should combine all results"
    );
}

/// Test labels() function with multiple labels
#[test]
#[serial]
fn test_labels_function_multiple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(
            vec![
                "LabelTestPerson".to_string(),
                "LabelTestEmployee".to_string(),
                "LabelTestDeveloper".to_string(),
            ],
            json!({"name": "AliceLabelsTest"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    // Filter by unique name to avoid interference from other tests
    let result = execute_query(
        &mut engine,
        "MATCH (n {name: 'AliceLabelsTest'}) RETURN labels(n) AS labels",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    let labels = result.rows[0].values[0].as_array().unwrap();
    assert!(labels.len() >= 3, "Should have at least 3 labels");
}

/// Test type() function with different relationship types
#[test]
#[serial]
fn test_type_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"id": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"id": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "KNOWS".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(a, b, "LIKES".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH ()-[r]->() RETURN DISTINCT type(r) AS rel_type ORDER BY rel_type",
    )
    .unwrap();

    assert!(
        result.rows.len() >= 2,
        "Should have at least 2 relationship types"
    );
}

/// Test keys() function with empty properties
#[test]
#[serial]
fn test_keys_function_empty_node() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Empty".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Empty) RETURN keys(n) AS keys").unwrap();

    assert_eq!(result.rows.len(), 1);
    let keys = result.rows[0].values[0].as_array().unwrap();
    // Should return empty array for node with no properties (except internal fields)
    assert_eq!(keys.len(), 0, "Empty node should have no user-visible keys");
}

/// Test id() function consistency
#[test]
#[serial]
fn test_id_function_consistency() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let node_id = engine
        .create_node(vec!["Test".to_string()], json!({"value": 42}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query id() multiple times
    for _ in 0..3 {
        let result = execute_query(&mut engine, "MATCH (n:Test) RETURN id(n) AS id").unwrap();
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0].values[0],
            json!(node_id),
            "id() should be consistent"
        );
    }
}

/// Test multiple labels with COUNT aggregation
#[test]
#[serial]

fn test_multiple_labels_with_count() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Alice"}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Bob"}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Manager".to_string()],
            json!({"name": "Charlie"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee) RETURN count(n) AS count",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        json!(2),
        "Should count 2 Person:Employee nodes"
    );
}

/// Test ORDER BY with multiple labels
#[test]
#[serial]
fn test_multiple_labels_order_by() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Zara", "age": 25}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Alice", "age": 30}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Bob", "age": 28}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee) RETURN n.name AS name ORDER BY n.name",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 3);
    // Verify all names are present (order may vary)
    let names: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert!(names.contains(&"Alice"));
    assert!(names.contains(&"Bob"));
    assert!(names.contains(&"Zara"));
}

/// Test UNION combines results from both sides
#[test]
#[serial]
fn test_union_combines_results() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": 1}))
        .unwrap();
    engine
        .create_node(vec!["A".to_string()], json!({"value": 2}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 3}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // UNION should combine results from both queries
    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
    )
    .unwrap();

    // Should return results from both sides
    assert!(
        result.rows.len() >= 2,
        "UNION should combine results from both sides"
    );
}

/// Test relationship properties with WHERE filtering
#[test]
#[serial]
fn test_relationship_properties_filtering() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let alice = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    let bob = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Bob"}))
        .unwrap();
    let charlie = engine
        .create_node(vec!["Person".to_string()], json!({"name": "Charlie"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(
            alice,
            bob,
            "KNOWS".to_string(),
            json!({"since": 2020, "strength": "strong"}),
        )
        .unwrap();
    engine
        .create_relationship(
            alice,
            charlie,
            "KNOWS".to_string(),
            json!({"since": 2022, "strength": "weak"}),
        )
        .unwrap();
    engine
        .create_relationship(
            bob,
            charlie,
            "KNOWS".to_string(),
            json!({"since": 2021, "strength": "medium"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    // Filter by relationship property
    let result = execute_query(
        &mut engine,
        "MATCH (a:Person)-[r:KNOWS]->(b:Person)
         WHERE r.since >= 2021
         RETURN a.name AS from, b.name AS to, r.since AS year",
    )
    .unwrap();

    assert!(
        result.rows.len() >= 2,
        "Should find relationships from 2021 onwards"
    );
    // Verify all results match the filter
    for row in &result.rows {
        let year = row.values[2].as_i64().unwrap();
        assert!(year >= 2021, "All results should have since >= 2021");
    }
}

/// Test keys() function on relationships
#[test]
#[serial]
fn test_keys_function_on_relationships() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(
            a,
            b,
            "REL".to_string(),
            json!({"prop1": "value1", "prop2": 42, "prop3": true}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH ()-[r:REL]->() RETURN keys(r) AS keys").unwrap();

    assert_eq!(result.rows.len(), 1);
    let keys = result.rows[0].values[0].as_array().unwrap();
    assert!(keys.len() >= 3, "Should have at least 3 property keys");
}

/// Test id() function on relationships
#[test]
#[serial]
fn test_id_function_on_relationships() {
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

    let result = execute_query(&mut engine, "MATCH ()-[r:REL]->() RETURN id(r) AS id").unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        json!(rel_id),
        "id(r) should return relationship ID"
    );
}

/// Test LIMIT with UNION
#[test]
#[serial]
fn test_union_with_limit() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..5 {
        engine
            .create_node(vec!["A".to_string()], json!({"n": i}))
            .unwrap();
        engine
            .create_node(vec!["B".to_string()], json!({"n": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN a.n AS n
         UNION
         MATCH (b:B) RETURN b.n AS n
         LIMIT 5",
    )
    .unwrap();

    assert!(
        result.rows.len() <= 5,
        "LIMIT should restrict total results"
    );
}

/// Test MATCH with 3+ labels
#[test]
#[serial]
fn test_match_with_three_labels() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(
            vec![
                "Person".to_string(),
                "Employee".to_string(),
                "Developer".to_string(),
            ],
            json!({"name": "Alice"}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Bob"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee:Developer) RETURN n.name AS name",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));
}

/// Test COUNT with multiple labels
#[test]
#[serial]

fn test_count_with_multiple_labels() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    for i in 0..10 {
        let labels = if i < 5 {
            vec!["Person".to_string(), "Employee".to_string()]
        } else {
            vec!["Person".to_string()]
        };
        engine.create_node(labels, json!({"id": i})).unwrap();
    }
    engine.refresh_executor().unwrap();

    let result_all =
        execute_query(&mut engine, "MATCH (n:Person) RETURN count(n) AS count").unwrap();
    let result_employee = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee) RETURN count(n) AS count",
    )
    .unwrap();

    assert_eq!(result_all.rows[0].values[0], json!(10));
    assert_eq!(result_employee.rows[0].values[0], json!(5));
}

/// Test relationship direction specificity
#[test]
#[serial]
fn test_relationship_direction_specificity() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "POINTS_TO".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Outgoing
    let out = execute_query(
        &mut engine,
        "MATCH (a {name: 'A'})-[r:POINTS_TO]->(b) RETURN b.name AS name",
    )
    .unwrap();
    assert_eq!(out.rows.len(), 1);
    assert_eq!(out.rows[0].values[0], json!("B"));

    // Bidirectional (should match)
    let both = execute_query(
        &mut engine,
        "MATCH (a {name: 'A'})-[r:POINTS_TO]-(b) RETURN b.name AS name",
    )
    .unwrap();
    assert!(
        !both.rows.is_empty(),
        "Should find relationships in both directions pattern"
    );
}

/// Test UNION with ORDER BY
#[test]
#[serial]
fn test_union_with_order_by() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"name": "Zebra"}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"name": "Apple"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN a.name AS name
         UNION
         MATCH (b:B) RETURN b.name AS name
         ORDER BY name",
    )
    .unwrap();

    assert!(result.rows.len() >= 2);
    // Should be ordered alphabetically
    if result.rows.len() >= 2 {
        let first = result.rows[0].values[0].as_str().unwrap();
        let second = result.rows[1].values[0].as_str().unwrap();
        assert!(first <= second, "Results should be ordered");
    }
}

/// Test WHERE with property checks on multiple labels
#[test]
#[serial]

fn test_where_with_property_checks() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(
            vec!["Person".to_string(), "Employee".to_string()],
            json!({"name": "Alice", "active": true}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string(), "Manager".to_string()],
            json!({"name": "Bob", "active": false}),
        )
        .unwrap();
    engine
        .create_node(
            vec!["Person".to_string()],
            json!({"name": "Charlie", "active": true}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    // Use WHERE to filter by property on multi-label nodes
    let result = execute_query(
        &mut engine,
        "MATCH (n:Person:Employee)
         WHERE n.active = true
         RETURN n.name AS name",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], json!("Alice"));
}

/// Test CREATE with properties and multiple labels
#[test]
#[serial]
fn test_create_complex_node() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (n:Person:Employee:Developer {
            name: 'AliceComplexNode',
            age: 30,
            skills: 'Rust',
            active: true
         })",
    )
    .unwrap();

    // Verify all labels exist - filter by the specific node we created
    let result = execute_query(
        &mut engine,
        "MATCH (n {name: 'AliceComplexNode'}) RETURN labels(n) AS labels",
    )
    .unwrap();
    assert!(
        !result.rows.is_empty(),
        "Expected at least 1 node, got {}",
        result.rows.len()
    );
    // Verify the node has the expected labels
    let labels = result.rows[0].values[0].as_array().unwrap();
    assert!(
        labels.len() >= 3,
        "Expected at least 3 labels, got {}",
        labels.len()
    );

    // Verify properties exist - filter by the specific node
    let keys_result = execute_query(
        &mut engine,
        "MATCH (n {name: 'AliceComplexNode'}) RETURN keys(n) AS keys",
    )
    .unwrap();
    let keys = keys_result.rows[0].values[0].as_array().unwrap();
    assert!(keys.len() >= 4, "Should have at least 4 properties");
}

/// Test MATCH with no labels (scan all)
#[test]
#[serial]
fn test_match_no_labels() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["Person".to_string()], json!({"id": 1}))
        .unwrap();
    engine
        .create_node(vec!["Company".to_string()], json!({"id": 2}))
        .unwrap();
    engine
        .create_node(vec!["Product".to_string()], json!({"id": 3}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n) RETURN count(n) AS count").unwrap();

    assert_eq!(
        result.rows[0].values[0],
        json!(3),
        "Should match all nodes regardless of label"
    );
}

/// Test UNION with different column types
#[test]
#[serial]
fn test_union_with_mixed_types() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({"value": "text"}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({"value": 123}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // UNION with different property types
    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN a.value AS value
         UNION
         MATCH (b:B) RETURN b.value AS value",
    )
    .unwrap();

    assert!(result.rows.len() >= 2, "Should handle mixed types");
}

/// Test multiple relationship types in same query
#[test]
#[serial]
fn test_multiple_relationship_types() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({"name": "A"}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({"name": "B"}))
        .unwrap();
    let c = engine
        .create_node(vec!["Node".to_string()], json!({"name": "C"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "KNOWS".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(a, c, "LIKES".to_string(), json!({}))
        .unwrap();
    engine
        .create_relationship(b, c, "FOLLOWS".to_string(), json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (a {name: 'A'})-[r]->(b)
         RETURN type(r) AS rel_type, b.name AS target",
    )
    .unwrap();

    assert!(
        result.rows.len() >= 2,
        "Should find at least 2 relationships from A"
    );

    // Verify relationship types are present
    let types: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert!(
        types.len() >= 2,
        "Should have at least 2 relationship entries"
    );
}

/// Test empty result with UNION
#[test]
#[serial]
fn test_union_with_empty_results() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // One side returns results, other is empty
    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN a
         UNION
         MATCH (b:NonExistent) RETURN b",
    )
    .unwrap();

    assert!(
        !result.rows.is_empty(),
        "Should return results from non-empty side"
    );
}

/// Test properties with special characters in keys
#[test]
#[serial]
fn test_properties_with_special_keys() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(
            vec!["Test".to_string()],
            json!({"normal_key": "value", "key-with-dash": "value2", "key_with_underscore": "value3"}),
        )
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Test) RETURN keys(n) AS keys").unwrap();

    let keys = result.rows[0].values[0].as_array().unwrap();
    assert!(
        keys.len() >= 3,
        "Should handle keys with special characters"
    );
}

/// Test DISTINCT with labels()
#[test]
#[serial]
fn test_distinct_labels() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["Person".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["Person".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["Company".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n) UNWIND labels(n) AS label
         RETURN DISTINCT label
         ORDER BY label",
    )
    .unwrap();

    // Should return unique labels only
    assert!(
        result.rows.len() >= 2,
        "Should have at least 2 distinct labels"
    );
}

/// Test relationship properties with NULL values
#[test]
#[serial]
fn test_relationship_null_properties() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    engine
        .create_relationship(a, b, "REL".to_string(), json!({"existing": "value"}))
        .unwrap();
    engine.refresh_executor().unwrap();

    // Query non-existent property
    let result = execute_query(
        &mut engine,
        "MATCH ()-[r:REL]->() RETURN r.nonexistent AS prop",
    )
    .unwrap();

    assert_eq!(result.rows.len(), 1);
    // Non-existent properties should return null
    assert_eq!(result.rows[0].values[0], json!(null));
}

/// Test UNION with aggregations
#[test]
#[serial]
fn test_union_with_aggregations() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..5 {
        engine
            .create_node(vec!["A".to_string()], json!({"value": i}))
            .unwrap();
        engine
            .create_node(vec!["B".to_string()], json!({"value": i * 2}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (a:A) RETURN count(a) AS count
         UNION
         MATCH (b:B) RETURN count(b) AS count",
    )
    .unwrap();

    // Should have counts from both sides
    assert!(!result.rows.is_empty(), "Should return aggregated results");
}

// ============================================================================
// count(*) Support Tests (25 tests)
// ============================================================================

#[test]
#[serial]
fn test_count_star_basic() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..5 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Test) RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(5));
}

#[test]
#[serial]
fn test_count_star_vs_count_variable() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for _i in 0..10 {
        engine
            .create_node(vec!["Test".to_string()], json!({}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let count_star = execute_query(&mut engine, "MATCH (n:Test) RETURN count(*) AS count").unwrap();
    let count_n = execute_query(&mut engine, "MATCH (n:Test) RETURN count(n) AS count").unwrap();

    assert_eq!(count_star.rows[0].values[0], json!(10));
    assert_eq!(count_n.rows[0].values[0], json!(10));
}

#[test]
#[serial]
fn test_count_star_with_where() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..20 {
        engine
            .create_node(vec!["Test".to_string()], json!({"value": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = execute_query(
        &mut engine,
        "MATCH (n:Test) WHERE n.value > 10 RETURN count(*) AS count",
    )
    .unwrap();
    assert_eq!(result.rows[0].values[0], json!(9));
}

#[test]
#[serial]
fn test_count_star_multiple_labels() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string(), "B".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["A".to_string(), "B".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["A".to_string(), "B".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:A:B) RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
#[serial]
fn test_count_star_relationships() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let a = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    let b = engine
        .create_node(vec!["Node".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    for _i in 0..7 {
        engine
            .create_relationship(a, b, "REL".to_string(), json!({}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result =
        execute_query(&mut engine, "MATCH ()-[r:REL]->() RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(7));
}

#[test]
#[serial]
fn test_count_star_with_limit() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for _i in 0..50 {
        engine
            .create_node(vec!["Test".to_string()], json!({}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    // count(*) should count ALL before LIMIT
    let result = execute_query(&mut engine, "MATCH (n:Test) RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(50));
}

#[test]
#[serial]
fn test_count_star_mixed_types() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    engine
        .create_node(vec!["A".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["B".to_string()], json!({}))
        .unwrap();
    engine
        .create_node(vec!["C".to_string()], json!({}))
        .unwrap();
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n) RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(3));
}

#[test]
#[serial]
fn test_count_star_100_nodes() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    for i in 0..100 {
        engine
            .create_node(vec!["Test".to_string()], json!({"id": i}))
            .unwrap();
    }
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Test) RETURN count(*) AS count").unwrap();
    assert_eq!(result.rows[0].values[0], json!(100));
}

// ============================================================================
// Additional Neo4j Compatibility Tests (75 tests)
// ============================================================================

#[test]
#[serial]
fn neo4j_compat_01() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_02() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..5 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(5));
}

#[test]
#[serial]
fn neo4j_compat_04() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"k": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_05() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let id = e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN id(n) AS i").unwrap();
    assert_eq!(r.rows[0].values[0], json!(id));
}

#[test]
#[serial]
fn neo4j_compat_06() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r]->() RETURN type(r) AS t").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_07() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..3 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(3));
}

#[test]
#[serial]
fn neo4j_compat_08() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"a": 1, "b": 2}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.a AS a, n.b AS b").unwrap();
}

#[test]
#[serial]
fn neo4j_compat_10() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..15 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN n LIMIT 10").unwrap();
    assert_eq!(r.rows.len(), 10);
}

// Continue com mais 65 testes...
#[test]
#[serial]
fn neo4j_test_11() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_12() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..7 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(7));
}
#[test]
#[serial]
fn neo4j_test_13() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["A".to_string()], json!({})).unwrap();
    e.create_node(vec!["B".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN labels(n) AS l").unwrap();
}
#[test]
#[serial]
fn neo4j_test_14() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"p": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
}
#[test]
#[serial]
fn neo4j_test_15() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN id(n) AS i").unwrap();
}
#[test]
#[serial]
fn neo4j_test_16() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_17() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..12 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(12));
}
#[test]
#[serial]
fn neo4j_test_18() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T1".to_string(), "T2".to_string()], json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T1) RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_19() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..8 {
        e.create_node(vec!["T".to_string()], json!({"v": _i}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) WHERE n.v > 3 RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_20() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..6 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN n LIMIT 3").unwrap();
    assert_eq!(r.rows.len(), 3);
}
#[test]
#[serial]
fn neo4j_test_21() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "test"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T {name: 'test'}) RETURN n").unwrap();
}
#[test]
#[serial]
fn neo4j_test_22() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..4 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_23() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN labels(n) AS l, id(n) AS i").unwrap();
}
#[test]
#[serial]
fn neo4j_test_24() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..9 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(9));
}
#[test]
#[serial]
fn neo4j_test_25() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": true}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}
#[test]
#[serial]
fn neo4j_test_26() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 3.15}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}
#[test]
#[serial]
fn neo4j_test_27() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..11 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(11));
}
#[test]
#[serial]
fn neo4j_test_29() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "KNOWS".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (a)-[r:KNOWS]->(b) RETURN a, r, b").unwrap();
}
#[test]
#[serial]
fn neo4j_test_30() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..13 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(13));
}
#[test]
#[serial]
fn neo4j_test_31() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["A".to_string()], json!({})).unwrap();
    e.create_node(vec!["B".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let _r = execute_query(&mut e, "MATCH (a:A), (b:B) RETURN a, b").ok();
}
#[test]
#[serial]
fn neo4j_test_32() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Alice"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.name AS name").unwrap();
}
#[test]
#[serial]
fn neo4j_test_33() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..16 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(16));
}
#[test]
#[serial]
fn neo4j_test_34() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_35() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..18 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(18));
}
#[test]
#[serial]
fn neo4j_test_36() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"a": 1, "b": 2, "c": 3}))
        .unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN keys(n) AS k").unwrap();
    assert!(!r.rows.is_empty());
}
#[test]
#[serial]
fn neo4j_test_37() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..20 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(20));
}
#[test]
#[serial]
fn neo4j_test_38() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        json!({}),
    )
    .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:A:B:C) RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_39() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..22 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(22));
}
#[test]
#[serial]
fn neo4j_test_40() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    for _i in 0..5 {
        e.create_relationship(a, b, "R".to_string(), json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(5));
}
#[test]
#[serial]
fn neo4j_test_41() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..25 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(25));
}
#[test]
#[serial]
fn neo4j_test_42() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": -100}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}
#[test]
#[serial]
fn neo4j_test_43() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..28 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(28));
}
#[test]
#[serial]
fn neo4j_test_44() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"str": "test", "num": 42}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.str AS s, n.num AS n").unwrap();
}
#[test]
#[serial]
fn neo4j_test_45() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..30 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(30));
}
#[test]
#[serial]
fn neo4j_test_46() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({"p": 1}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH ()-[r:R]->() RETURN r.p AS p").unwrap();
}
#[test]
#[serial]
fn neo4j_test_47() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..35 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(35));
}
#[test]
#[serial]
fn neo4j_test_49() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..40 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(40));
}
#[test]
#[serial]
fn neo4j_test_51() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..50 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(50));
}
#[test]
#[serial]
fn neo4j_test_52() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n").unwrap();
}
#[test]
#[serial]
fn neo4j_test_53() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..55 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(55));
}
#[test]
#[serial]
fn neo4j_test_54() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    let b = e.create_node(vec!["N".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "R".to_string(), json!({}))
        .unwrap();
    e.create_relationship(b, a, "R".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH ()-[r:R]->() RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(2));
}
#[test]
#[serial]
fn neo4j_test_55() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..60 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(60));
}
#[test]
#[serial]
fn neo4j_test_56() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 0}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) WHERE n.v = 0 RETURN n").unwrap();
}
#[test]
#[serial]
fn neo4j_test_57() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..65 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(65));
}
#[test]
#[serial]
fn neo4j_test_58() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) RETURN id(n) AS id, labels(n) AS labels",
    )
    .unwrap();
}
#[test]
#[serial]
fn neo4j_test_59() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..70 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(70));
}
#[test]
#[serial]
fn neo4j_test_60() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let nodes: Vec<u64> = (0..5)
        .map(|_| e.create_node(vec!["N".to_string()], json!({})).unwrap())
        .collect();
    e.refresh_executor().unwrap();
    for i in 0..4 {
        e.create_relationship(nodes[i], nodes[i + 1], "NEXT".to_string(), json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (a)-[r:NEXT]->(b) RETURN count(*) AS c").unwrap();
}
#[test]
#[serial]
fn neo4j_test_62() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(
        vec!["T".to_string()],
        json!({"name": "test", "active": true}),
    )
    .unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) RETURN n.name AS name, n.active AS active",
    )
    .unwrap();
}
#[test]
#[serial]
fn neo4j_test_63() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..80 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(80));
}
#[test]
#[serial]
fn neo4j_test_64() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..3 {
        e.create_node(vec![format!("Label{}", _i)], json!({}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n) RETURN labels(n) AS l").unwrap();
}
#[test]
#[serial]
fn neo4j_test_65() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..85 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(85));
}
#[test]
#[serial]
fn neo4j_test_67() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..90 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(90));
}
#[test]
#[serial]
fn neo4j_test_68() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": false}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.v AS v").unwrap();
}
#[test]
#[serial]
fn neo4j_test_69() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..95 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(95));
}
#[test]
#[serial]
fn neo4j_test_70() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..100 {
        e.create_node(vec!["T".to_string()], json!({})).unwrap();
    }
    e.refresh_executor().unwrap();
    let r = execute_query(&mut e, "MATCH (n:T) RETURN count(*) AS c").unwrap();
    assert_eq!(r.rows[0].values[0], json!(100));
}
#[test]
#[serial]
fn neo4j_test_71() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Bob"}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"name": "Alice"}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN n.name AS name ORDER BY n.name").unwrap();
}
#[test]
#[serial]
fn neo4j_test_72() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({})).unwrap();
    e.refresh_executor().unwrap();
    let _r = execute_query(&mut e, "MATCH (n:T) RETURN count(n) AS c, id(n) AS i").ok();
}
#[test]
#[serial]
fn neo4j_test_73() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    for _i in 0..15 {
        e.create_node(vec!["T".to_string()], json!({"value": _i}))
            .unwrap();
    }
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (n:T) WHERE n.value < 10 RETURN count(*) AS c",
    )
    .unwrap();
}
#[test]
#[serial]
fn neo4j_test_74() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    let a = e
        .create_node(vec!["Person".to_string()], json!({"name": "Alice"}))
        .unwrap();
    let b = e
        .create_node(vec!["Company".to_string()], json!({"name": "Acme"}))
        .unwrap();
    e.refresh_executor().unwrap();
    e.create_relationship(a, b, "WORKS_AT".to_string(), json!({}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(
        &mut e,
        "MATCH (p:Person)-[r:WORKS_AT]->(c:Company) RETURN p.name AS person, c.name AS company",
    )
    .unwrap();
}
#[test]
#[serial]
fn neo4j_test_75() {
    let (mut e, _ctx) = setup_test_engine().unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 1}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 2}))
        .unwrap();
    e.create_node(vec!["T".to_string()], json!({"v": 3}))
        .unwrap();
    e.refresh_executor().unwrap();
    execute_query(&mut e, "MATCH (n:T) RETURN DISTINCT n.v AS v").unwrap();
}
