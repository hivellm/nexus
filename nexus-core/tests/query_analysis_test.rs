use nexus_core::Engine;
use serde_json::Value;

fn create_engine() -> Engine {
    Engine::new().expect("Failed to create engine")
}

fn extract_first_row_value(result: nexus_core::executor::ResultSet) -> Option<Value> {
    result
        .rows
        .first()
        .and_then(|row| row.values.first().cloned())
}

#[test]
fn test_explain_simple_query() {
    let mut engine = create_engine();

    // Create some test data
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30})")
        .unwrap();

    // Test EXPLAIN
    let query = "EXPLAIN MATCH (n:Person) RETURN n";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["plan"]);
    assert_eq!(result.rows.len(), 1);

    // Check that plan is valid JSON
    let plan_value = extract_first_row_value(result).unwrap();
    assert!(plan_value.is_object());

    let plan_obj = plan_value.as_object().unwrap();
    assert!(plan_obj.contains_key("plan"));
}

#[test]
fn test_explain_with_where() {
    let mut engine = create_engine();

    // Create test data
    engine
        .execute_cypher("CREATE (n:Person {name: 'Bob', age: 25})")
        .unwrap();

    // Test EXPLAIN with WHERE clause
    let query = "EXPLAIN MATCH (n:Person) WHERE n.age > 20 RETURN n";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["plan"]);
    assert_eq!(result.rows.len(), 1);

    let plan_value = extract_first_row_value(result).unwrap();
    assert!(plan_value.is_object());
}

#[test]
fn test_profile_simple_query() {
    let mut engine = create_engine();

    // Create test data
    engine
        .execute_cypher("CREATE (n:Person {name: 'Charlie', age: 35})")
        .unwrap();

    // Test PROFILE
    let query = "PROFILE MATCH (n:Person) RETURN n";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["profile"]);
    assert_eq!(result.rows.len(), 1);

    // Check that profile is valid JSON with execution stats
    let profile_value = extract_first_row_value(result).unwrap();
    assert!(profile_value.is_object());

    let profile_obj = profile_value.as_object().unwrap();
    assert!(profile_obj.contains_key("plan"));
    assert!(profile_obj.contains_key("execution_time_ms"));
    assert!(profile_obj.contains_key("execution_time_us"));
    assert!(profile_obj.contains_key("rows_returned"));
    assert!(profile_obj.contains_key("columns_returned"));
}

#[test]
fn test_profile_with_where() {
    let mut engine = create_engine();

    // Create test data
    engine
        .execute_cypher("CREATE (n:Person {name: 'David', age: 40})")
        .unwrap();

    // Test PROFILE with WHERE clause
    let query = "PROFILE MATCH (n:Person) WHERE n.age > 30 RETURN n";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["profile"]);
    assert_eq!(result.rows.len(), 1);

    let profile_value = extract_first_row_value(result).unwrap();
    let profile_obj = profile_value.as_object().unwrap();

    // Verify execution stats are present
    assert!(profile_obj.contains_key("execution_time_ms"));
    assert!(profile_obj.contains_key("rows_returned"));
}

#[test]
fn test_explain_create_query() {
    let mut engine = create_engine();

    // Test EXPLAIN with CREATE (should show plan but not execute)
    let query = "EXPLAIN CREATE (n:Person {name: 'Test'})";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["plan"]);

    // Verify node was NOT created (EXPLAIN doesn't execute)
    let check_result = engine
        .execute_cypher("MATCH (n:Person {name: 'Test'}) RETURN n")
        .unwrap();
    assert_eq!(check_result.rows.len(), 0);
}

#[test]
fn test_profile_create_query() {
    let mut engine = create_engine();

    // Test PROFILE with CREATE (should execute and show stats)
    let query = "PROFILE CREATE (n:Person {name: 'ProfileTest'})";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["profile"]);

    // Verify node WAS created (PROFILE executes the query)
    let check_result = engine
        .execute_cypher("MATCH (n:Person {name: 'ProfileTest'}) RETURN n")
        .unwrap();
    assert_eq!(check_result.rows.len(), 1);
}
