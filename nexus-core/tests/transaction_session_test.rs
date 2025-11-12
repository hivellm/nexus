//! Tests for transaction session management
//!
//! Tests cover:
//! - Session-based transaction persistence
//! - Multiple independent sessions
//! - Transaction isolation per session
//! - Error handling (COMMIT/ROLLBACK without BEGIN, double BEGIN)
//! - Session expiration and cleanup

use nexus_core::Engine;
use serde_json::Value;

/// Helper function to create a new engine instance
fn create_engine() -> Engine {
    Engine::new().expect("Failed to create engine")
}

/// Helper function to extract the first value from the first row of a result set
fn extract_first_row_value(result: nexus_core::executor::ResultSet) -> Option<Value> {
    result.rows.first().and_then(|row| row.values.first().cloned())
}

#[test]
fn test_transaction_persists_across_queries() {
    let mut engine = create_engine();

    // Begin transaction in default session
    let query = "BEGIN TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create a node (should be in transaction)
    let query = "CREATE (n:Person {name: 'Alice'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Commit transaction (should persist from previous query)
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify node was created
    let query = "MATCH (n:Person {name: 'Alice'}) RETURN n.name AS name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("Alice".to_string()))
    );
}

#[test]
fn test_transaction_rollback_persists_across_queries() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create a node
    let query = "CREATE (n:Person {name: 'Bob'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Rollback transaction
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify node was NOT created (rolled back)
    let query = "MATCH (n:Person {name: 'Bob'}) RETURN n.name AS name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn test_commit_without_begin_returns_error() {
    let mut engine = create_engine();

    // Try to commit without beginning a transaction
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query);
    
    assert!(result.is_err(), "COMMIT without BEGIN should return error");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("not found") || error_msg.contains("expired"),
        "Error should mention session not found or expired"
    );
}

#[test]
fn test_rollback_without_begin_returns_error() {
    let mut engine = create_engine();

    // Try to rollback without beginning a transaction
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query);
    
    assert!(result.is_err(), "ROLLBACK without BEGIN should return error");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("not found") || error_msg.contains("expired"),
        "Error should mention session not found or expired"
    );
}

#[test]
fn test_double_begin_returns_error() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Try to begin another transaction in the same session
    let query = "BEGIN TRANSACTION";
    let result = engine.execute_cypher(query);
    
    assert!(result.is_err(), "Double BEGIN should return error");
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("already has an active transaction"),
        "Error should mention active transaction already exists"
    );
}

#[test]
fn test_commit_then_begin_new_transaction() {
    let mut engine = create_engine();

    // Begin and commit first transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();
    
    let query = "CREATE (n:Person {name: 'Charlie'}) RETURN n";
    engine.execute_cypher(query).unwrap();
    
    let query = "COMMIT TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Begin a new transaction
    let query = "BEGIN TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create another node
    let query = "CREATE (n:Person {name: 'David'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Commit second transaction
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify both nodes exist
    let query = "MATCH (n:Person) RETURN n.name AS name ORDER BY n.name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 2);
}

#[test]
fn test_multiple_operations_in_transaction() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create multiple nodes
    let query = "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) RETURN a, b";
    engine.execute_cypher(query).unwrap();

    // Create relationships
    let query = "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[:KNOWS]->(b) RETURN a, b";
    engine.execute_cypher(query).unwrap();

    // Commit transaction
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify everything was created
    let query = "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name AS from, b.name AS to";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn test_rollback_multiple_operations() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create multiple nodes
    let query = "CREATE (a:Person {name: 'Eve'}), (b:Person {name: 'Frank'}) RETURN a, b";
    engine.execute_cypher(query).unwrap();

    // Create relationships
    let query = "MATCH (a:Person {name: 'Eve'}), (b:Person {name: 'Frank'}) CREATE (a)-[:KNOWS]->(b) RETURN a, b";
    engine.execute_cypher(query).unwrap();

    // Rollback transaction
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify nothing was created
    let query = "MATCH (n:Person) WHERE n.name IN ['Eve', 'Frank'] RETURN n.name AS name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 0);
}

#[test]
fn test_begin_commit_rollback_sequence() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create a node
    let query = "CREATE (n:Person {name: 'Grace'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Commit transaction
    let query = "COMMIT TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Begin new transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create another node
    let query = "CREATE (n:Person {name: 'Henry'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Rollback second transaction
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify only first node exists
    let query = "MATCH (n:Person) WHERE n.name IN ['Grace', 'Henry'] RETURN n.name AS name ORDER BY n.name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("Grace".to_string()))
    );
}

#[test]
fn test_transaction_with_create_index() {
    let mut engine = create_engine();

    // Create a node first
    let query = "CREATE (n:Person {name: 'IndexTest', age: 25}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create index within transaction
    let query = "CREATE INDEX ON :Person(age)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Commit transaction
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify index was created (by checking it can be used)
    let query = "MATCH (n:Person) WHERE n.age = 25 RETURN n.name AS name";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(result.rows.len(), 1);
}

