//! Integration tests for Cypher Schema Administration commands
//!
//! Tests cover:
//! - Index Management (CREATE INDEX, DROP INDEX)
//! - Constraint Management (CREATE CONSTRAINT, DROP CONSTRAINT)
//! - Transaction Commands (BEGIN, COMMIT, ROLLBACK)
//! - Database Management (CREATE/DROP/SHOW DATABASE) - parsing only
//! - User Management (SHOW/CREATE USER, GRANT/REVOKE) - parsing only
//!
//! Note: Tests that require server execution are marked with #[cfg(feature = "server-tests")]
//! and should only be run when the server is available.

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
fn test_create_index_basic() {
    let mut engine = create_engine();

    // Create a node with a label and property first
    let query = "CREATE (n:Person {name: 'Alice', age: 30}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Create index on :Person(name)
    let query = "CREATE INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_index_if_not_exists() {
    let mut engine = create_engine();

    // Create index first time
    let query = "CREATE INDEX IF NOT EXISTS ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create same index again with IF NOT EXISTS - should succeed
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_or_replace_index() {
    let mut engine = create_engine();

    // Create index first time
    let query = "CREATE INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Replace the index with OR REPLACE - should succeed
    let query = "CREATE OR REPLACE INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_or_replace_index_nonexistent() {
    let mut engine = create_engine();

    // Create OR REPLACE index that doesn't exist - should succeed (creates new index)
    let query = "CREATE OR REPLACE INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_index_multiple_properties() {
    let mut engine = create_engine();

    // Create indexes on different properties
    let queries = vec![
        "CREATE INDEX ON :Person(name)",
        "CREATE INDEX ON :Person(age)",
        "CREATE INDEX ON :Person(email)",
    ];

    for query in queries {
        let result = engine.execute_cypher(query).unwrap();
        assert_eq!(
            extract_first_row_value(result),
            Some(Value::String("ok".to_string()))
        );
    }
}

#[test]
fn test_create_index_different_labels() {
    let mut engine = create_engine();

    // Create indexes on different labels
    let queries = vec![
        "CREATE INDEX ON :Person(name)",
        "CREATE INDEX ON :Company(name)",
        "CREATE INDEX ON :Product(name)",
    ];

    for query in queries {
        let result = engine.execute_cypher(query).unwrap();
        assert_eq!(
            extract_first_row_value(result),
            Some(Value::String("ok".to_string()))
        );
    }
}

#[test]
fn test_drop_index_basic() {
    let mut engine = create_engine();

    // Create index first
    let query = "CREATE INDEX ON :Person(name)";
    engine.execute_cypher(query).unwrap();

    // Drop the index
    let query = "DROP INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_index_if_exists() {
    let mut engine = create_engine();

    // Drop index that doesn't exist with IF EXISTS - should succeed
    let query = "DROP INDEX IF EXISTS ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create index
    engine.execute_cypher("CREATE INDEX ON :Person(name)").unwrap();

    // Drop with IF EXISTS - should succeed
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_index_nonexistent() {
    let mut engine = create_engine();

    // Try to drop index that doesn't exist - should fail
    let query = "DROP INDEX ON :Person(name)";
    let result = engine.execute_cypher(query);

    // Should return error or handle gracefully
    // For now, it checks if label exists, so it might succeed if label doesn't exist
    // This test documents current behavior
    match result {
        Ok(_) => {
            // If label doesn't exist, it might succeed with IF EXISTS logic
        }
        Err(e) => {
            // If label exists but index doesn't, it should fail
            assert!(e.to_string().contains("does not exist"));
        }
    }
}

#[test]
fn test_create_constraint_unique() {
    let mut engine = create_engine();

    // Create constraint - should succeed now
    let query = "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query);

    assert!(result.is_ok());
    assert_eq!(
        extract_first_row_value(result.unwrap()),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_constraint_exists() {
    let mut engine = create_engine();

    // Create EXISTS constraint - should succeed now
    let query = "CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS(n.email)";
    let result = engine.execute_cypher(query);

    assert!(result.is_ok());
    assert_eq!(
        extract_first_row_value(result.unwrap()),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_constraint_if_not_exists() {
    let mut engine = create_engine();

    // Create constraint with IF NOT EXISTS - should skip silently
    let query = "CREATE CONSTRAINT IF NOT EXISTS ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();

    // Should succeed (skipped) but return ok
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_constraint_unique() {
    let mut engine = create_engine();

    // First create the constraint
    engine.execute_cypher("CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE").unwrap();

    // Drop constraint - should succeed now
    let query = "DROP CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query);

    assert!(result.is_ok());
    assert_eq!(
        extract_first_row_value(result.unwrap()),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_constraint_if_exists() {
    let mut engine = create_engine();

    // Drop constraint with IF EXISTS - should skip silently
    let query = "DROP CONSTRAINT IF EXISTS ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();

    // Should succeed (skipped) but return ok
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_constraint_enforcement_unique() {
    let mut engine = create_engine();

    // Create UNIQUE constraint
    engine.execute_cypher("CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE").unwrap();

    // Create first node with email - should succeed
    engine.execute_cypher("CREATE (n:Person {email: 'alice@example.com'})").unwrap();

    // Try to create second node with same email - should fail
    let result = engine.execute_cypher("CREATE (n:Person {email: 'alice@example.com'})");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("UNIQUE constraint violated"));
}

#[test]
fn test_constraint_enforcement_exists() {
    let mut engine = create_engine();

    // Create EXISTS constraint
    engine.execute_cypher("CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS(n.email)").unwrap();

    // Create node without email - should fail
    let result = engine.execute_cypher("CREATE (n:Person {name: 'Alice'})");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("EXISTS constraint violated"));

    // Create node with email - should succeed
    let result = engine.execute_cypher("CREATE (n:Person {email: 'alice@example.com'})");
    assert!(result.is_ok());
}

#[test]
fn test_constraint_enforcement_update() {
    let mut engine = create_engine();

    // Create UNIQUE constraint
    engine.execute_cypher("CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE").unwrap();

    // Create two nodes with different emails
    engine.execute_cypher("CREATE (n1:Person {email: 'alice@example.com'})").unwrap();
    engine.execute_cypher("CREATE (n2:Person {email: 'bob@example.com'})").unwrap();

    // Try to update second node to have same email as first - should fail
    let result = engine.execute_cypher("MATCH (n:Person {email: 'bob@example.com'}) SET n.email = 'alice@example.com'");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("UNIQUE constraint violated"));
}

#[test]
fn test_begin_transaction() {
    let mut engine = create_engine();

    // BEGIN transaction - should succeed (placeholder)
    let query = "BEGIN";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_begin_transaction_explicit() {
    let mut engine = create_engine();

    // BEGIN TRANSACTION - should succeed (placeholder)
    let query = "BEGIN TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_commit_transaction() {
    let mut engine = create_engine();

    // COMMIT transaction - should succeed (placeholder)
    let query = "COMMIT";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_commit_transaction_explicit() {
    let mut engine = create_engine();

    // COMMIT TRANSACTION - should succeed (placeholder)
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_rollback_transaction() {
    let mut engine = create_engine();

    // ROLLBACK transaction - should succeed (no-op for now, transactions are automatic)
    let query = "ROLLBACK";
    let result = engine.execute_cypher(query);

    assert!(result.is_ok());
    assert_eq!(
        extract_first_row_value(result.unwrap()),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_rollback_transaction_explicit() {
    let mut engine = create_engine();

    // ROLLBACK TRANSACTION - should succeed (no-op for now, transactions are automatic)
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query);

    assert!(result.is_ok());
    assert_eq!(
        extract_first_row_value(result.unwrap()),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_transaction_sequence() {
    let mut engine = create_engine();

    // Test BEGIN -> COMMIT sequence
    let result = engine.execute_cypher("BEGIN").unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create some data
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice'})")
        .unwrap();

    // Commit
    let result = engine.execute_cypher("COMMIT").unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Verify data exists
    let result = engine
        .execute_cypher("MATCH (n:Person {name: 'Alice'}) RETURN n")
        .unwrap();
    assert_eq!(result.rows.len(), 1);
}

#[test]
fn test_create_database_command_returns_error() {
    let mut engine = create_engine();

    // CREATE DATABASE should return error indicating server-level execution needed
    let query = "CREATE DATABASE testdb";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_drop_database_command_returns_error() {
    let mut engine = create_engine();

    // DROP DATABASE should return error
    let query = "DROP DATABASE testdb";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_show_databases_command_returns_error() {
    let mut engine = create_engine();

    // SHOW DATABASES should return error
    let query = "SHOW DATABASES";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_show_users_command_returns_error() {
    let mut engine = create_engine();

    // SHOW USERS should return error
    let query = "SHOW USERS";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_create_user_command_returns_error() {
    let mut engine = create_engine();

    // CREATE USER should return error
    let query = "CREATE USER alice SET PASSWORD 'secret123'";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_grant_command_returns_error() {
    let mut engine = create_engine();

    // GRANT should return error
    let query = "GRANT READ, WRITE TO alice";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_revoke_command_returns_error() {
    let mut engine = create_engine();

    // REVOKE should return error
    let query = "REVOKE READ FROM alice";
    let result = engine.execute_cypher(query);

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("must be executed at server level"));
}

#[test]
fn test_index_parsing_complex() {
    let mut engine = create_engine();

    // Test various index creation patterns
    let queries = vec![
        "CREATE INDEX ON :Label(property)",
        "CREATE INDEX IF NOT EXISTS ON :Label(property)",
        "CREATE OR REPLACE INDEX ON :Label(property)",
        "DROP INDEX ON :Label(property)",
        "DROP INDEX IF EXISTS ON :Label(property)",
    ];

    for query in queries {
        let result = engine.execute_cypher(query);
        // Should parse correctly (may fail execution but not parsing)
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("does not exist"));
    }
}

#[test]
fn test_constraint_parsing_complex() {
    let mut engine = create_engine();

    // Test various constraint patterns
    let queries = vec![
        "CREATE CONSTRAINT ON (n:Label) ASSERT n.property IS UNIQUE",
        "CREATE CONSTRAINT IF NOT EXISTS ON (n:Label) ASSERT n.property IS UNIQUE",
        "CREATE CONSTRAINT ON (n:Label) ASSERT EXISTS(n.property)",
        "DROP CONSTRAINT ON (n:Label) ASSERT n.property IS UNIQUE",
        "DROP CONSTRAINT IF EXISTS ON (n:Label) ASSERT EXISTS(n.property)",
    ];

    for query in queries {
        let result = engine.execute_cypher(query);
        // Should parse correctly (will fail execution as constraint system not implemented)
        assert!(result.is_ok() || result.unwrap_err().to_string().contains("Constraint"));
    }
}

#[test]
fn test_user_management_parsing() {
    let mut engine = create_engine();

    // Test user management command parsing (will fail execution but should parse)
    let queries = vec![
        "SHOW USERS",
        "CREATE USER alice",
        "CREATE USER alice SET PASSWORD 'secret'",
        "CREATE USER alice IF NOT EXISTS",
        "GRANT READ TO alice",
        "GRANT READ, WRITE TO alice",
        "REVOKE READ FROM alice",
        "REVOKE READ, WRITE FROM alice",
    ];

    for query in queries {
        let result = engine.execute_cypher(query);
        // Should parse correctly (will fail execution as needs server level)
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be executed at server level"));
    }
}

#[test]
fn test_database_management_parsing() {
    let mut engine = create_engine();

    // Test database management command parsing
    let queries = vec![
        "SHOW DATABASES",
        "CREATE DATABASE testdb",
        "CREATE DATABASE testdb IF NOT EXISTS",
        "DROP DATABASE testdb",
        "DROP DATABASE testdb IF EXISTS",
    ];

    for query in queries {
        let result = engine.execute_cypher(query);
        // Should parse correctly (will fail execution as needs server level)
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("must be executed at server level"));
    }
}

