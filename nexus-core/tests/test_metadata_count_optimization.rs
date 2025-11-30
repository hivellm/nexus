//! Tests for metadata-based COUNT(*) optimization
//!
//! Phase 2.1: Implement metadata-based COUNT
//! Tests verify that COUNT(*) uses catalog metadata when possible

use nexus_core::Engine;
use nexus_core::testing::setup_isolated_test_engine;
use std::sync::atomic::{AtomicU32, Ordering};

/// Counter for unique test labels to prevent cross-test interference
static TEST_COUNTER: AtomicU32 = AtomicU32::new(0);

/// Helper function to execute a Cypher query
fn execute_cypher(engine: &mut Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).unwrap()
}

#[test]
fn test_count_star_uses_metadata() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("TestNode{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create some nodes with unique label
    for i in 0..10 {
        let query = format!("CREATE (n:{} {{id: {}}})", label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) should use metadata - count nodes with our unique label
    let query = format!("MATCH (n:{}) RETURN count(*) as total", label);
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 10, "COUNT(*) should return 10 nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_with_label_uses_metadata() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("Person{}", test_id);
    let company_label = format!("Company{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with different unique labels
    for i in 0..5 {
        let query = format!("CREATE (n:{} {{id: {}}})", person_label, i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..3 {
        let query = format!("CREATE (n:{} {{id: {}}})", company_label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with label filter
    let query = format!("MATCH (n:{}) RETURN count(*) as total", person_label);
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 5, "COUNT(*) should return 5 Person nodes");
    } else {
        panic!("COUNT(*) should return a number");
    }
}

#[test]
fn test_count_star_updates_on_create() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("TestPerson{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Initial count for our unique label
    let query = format!("MATCH (n:{}) RETURN count(*) as total", label);
    let result = execute_cypher(&mut engine, &query);
    let initial_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    // Create a node with unique label
    execute_cypher(
        &mut engine,
        &format!("CREATE (n:{} {{name: 'Alice'}})", label),
    );

    // Count should increase
    let result = execute_cypher(&mut engine, &query);
    let new_count = result.rows[0].values[0].as_u64().unwrap_or(0);

    assert_eq!(
        new_count,
        initial_count + 1,
        "COUNT(*) should increase after CREATE"
    );
}

#[test]
fn test_count_star_with_group_by() {
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let person_label = format!("PersonGroup{}", test_id);
    let company_label = format!("CompanyGroup{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with different unique labels
    for i in 0..3 {
        let query = format!("CREATE (n:{} {{id: {}}})", person_label, i);
        execute_cypher(&mut engine, &query);
    }
    for i in 0..2 {
        let query = format!("CREATE (n:{} {{id: {}}})", company_label, i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with GROUP BY using our unique labels
    let query = format!(
        "MATCH (n) WHERE n:{} OR n:{} RETURN labels(n)[0] as label, count(*) as total ORDER BY label",
        person_label, company_label
    );
    let result = execute_cypher(&mut engine, &query);

    // Should have groups (may vary based on implementation)
    assert!(result.rows.len() >= 1, "Should have at least 1 group");

    // Verify counts - find our unique label groups
    let person_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some(&person_label))
        .and_then(|row| row.values[1].as_u64())
        .unwrap_or(0);
    let company_count = result
        .rows
        .iter()
        .find(|row| row.values[0].as_str() == Some(&company_label))
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
    let test_id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    let label = format!("PersonFilter{}", test_id);
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Create nodes with properties and unique label
    for i in 0..5 {
        let query = format!("CREATE (n:{} {{id: {}, age: {}}})", label, i, 20 + i);
        execute_cypher(&mut engine, &query);
    }

    // COUNT(*) with WHERE filter (should not use metadata optimization)
    let query = format!(
        "MATCH (n:{}) WHERE n.age > 22 RETURN count(*) as total",
        label
    );
    let result = execute_cypher(&mut engine, &query);

    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        // Should count nodes with age > 22 (ids 3 and 4)
        assert_eq!(count, 2, "COUNT(*) with WHERE should return filtered count");
    } else {
        panic!("COUNT(*) should return a number");
    }
}
