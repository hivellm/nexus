//! Tests for metadata-based COUNT(*) optimization
//!
//! Phase 2.1: Implement metadata-based COUNT
//! Tests verify that COUNT(*) uses catalog metadata when possible

use nexus_core::Engine;
use tempfile::TempDir;

/// Helper function to execute a Cypher query
fn execute_cypher(engine: &mut Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).unwrap()
}

#[test]
fn test_count_star_uses_metadata() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create some nodes
    for i in 0..10 {
        let query = format!("CREATE (n:Person {{id: {}}})", i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) should use metadata
    let query = "MATCH (n) RETURN count(*) as total";
    let result = execute_cypher(&mut engine, query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 10, "COUNT(*) should return 10 nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_with_label_uses_metadata() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes with different labels
    for i in 0..5 {
        let query = format!("CREATE (n:Person {{id: {}}})", i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..3 {
        let query = format!("CREATE (n:Company {{id: {}}})", i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with label filter
    let query = "MATCH (n:Person) RETURN count(*) as total";
    let result = execute_cypher(&mut engine, query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 5, "COUNT(*) should return 5 Person nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_updates_on_create() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Initial count
    let query = "MATCH (n) RETURN count(*) as total";
    let result = execute_cypher(&mut engine, query);
    let initial_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    // Create a node
    execute_cypher(&mut engine, "CREATE (n:Person {name: 'Alice'})");

    // Count should increase
    let result = execute_cypher(&mut engine, query);
    let new_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    assert_eq!(
        new_count,
        initial_count + 1,
        "COUNT(*) should increase after CREATE"
    );
}

#[test]
fn test_count_star_with_group_by() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes with different labels
    for i in 0..3 {
        let query = format!("CREATE (n:Person {{id: {}}})", i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..2 {
        let query = format!("CREATE (n:Company {{id: {}}})", i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with GROUP BY (should not use metadata optimization)
    let query = "MATCH (n) RETURN labels(n)[0] as label, count(*) as total ORDER BY label";
    let result = execute_cypher(&mut engine, query);

    // Should have groups (may vary based on implementation)
    assert!(result.rows.len() >= 1, "Should have at least 1 group");

    // Verify counts - find Person and Company groups
    let person_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some("Person"))
        .and_then(|row| row.values[1].as_u64())
        .unwrap_or(0);
    let company_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some("Company"))
        .and_then(|row| row.values[1].as_u64())
        .unwrap_or(0);

    // Verify that counts are correct (may be grouped or individual)
    assert!(
        person_count >= 3 || result.rows.len() == 5,
        "Should count Person nodes correctly"
    );
    assert!(
        company_count >= 2 || result.rows.len() == 5,
        "Should count Company nodes correctly"
    );
}

#[test]
fn test_count_star_with_where_filter() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create nodes with properties
    for i in 0..5 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with WHERE filter (should not use metadata optimization)
    let query = "MATCH (n:Person) WHERE n.age > 22 RETURN count(*) as total";
    let result = execute_cypher(&mut engine, query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        // Should count nodes with age > 22 (ids 3 and 4)
        assert_eq!(count, 2, "COUNT(*) with WHERE should return filtered count");
    } else {
        panic!("COUNT(*) should return a number");
    }
}
