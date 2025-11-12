//! Integration tests for Cypher Schema Administration commands
//!
//! Tests cover:
//! - Index Management (CREATE INDEX, DROP INDEX)
//! - Constraint Management (CREATE CONSTRAINT, DROP CONSTRAINT)
//! - Transaction Commands (BEGIN, COMMIT, ROLLBACK)
//! - Database Management (CREATE/DROP/SHOW DATABASE) - parsing only
//! - User Management (SHOW/CREATE USER, GRANT/REVOKE) - parsing only
//! - CALL Subquery Support (CALL {...}, CALL {...} IN TRANSACTIONS)
//! - Named Paths (p = (pattern))
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
    let query = "CREATE INDEX IF NOT EXISTS ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_index_basic() {
    let mut engine = create_engine();

    // Create index first
    let query = "CREATE INDEX ON :Person(name)";
    engine.execute_cypher(query).unwrap();

    // Drop index
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

    // Drop non-existent index with IF EXISTS - should succeed
    let query = "DROP INDEX IF EXISTS ON :Person(nonexistent)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_constraint_unique() {
    let mut engine = create_engine();

    // Create a node with a label and property first
    let query = "CREATE (n:Person {name: 'Alice', email: 'alice@example.com'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Create unique constraint on :Person(email)
    let query = "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_constraint_exists() {
    let mut engine = create_engine();

    // Create a node with a label and property first
    let query = "CREATE (n:Person {name: 'Alice', email: 'alice@example.com'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Create exists constraint on :Person(email)
    let query = "CREATE CONSTRAINT ON (n:Person) ASSERT EXISTS(n.email)";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_constraint_if_not_exists() {
    let mut engine = create_engine();

    // Create constraint first time
    let query = "CREATE CONSTRAINT IF NOT EXISTS ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create same constraint again with IF NOT EXISTS - should succeed
    let query = "CREATE CONSTRAINT IF NOT EXISTS ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_constraint() {
    let mut engine = create_engine();

    // Create constraint first
    let query = "CREATE CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE";
    engine.execute_cypher(query).unwrap();

    // Drop constraint
    let query = "DROP CONSTRAINT ON (n:Person) ASSERT n.email IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["status"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_drop_constraint_if_exists() {
    let mut engine = create_engine();

    // Drop non-existent constraint with IF EXISTS - should succeed
    let query = "DROP CONSTRAINT IF EXISTS ON (n:Person) ASSERT n.nonexistent IS UNIQUE";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_begin_commit_transaction() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );

    // Create a node
    let query = "CREATE (n:Person {name: 'Alice'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Commit transaction
    let query = "COMMIT TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_begin_rollback_transaction() {
    let mut engine = create_engine();

    // Begin transaction
    let query = "BEGIN TRANSACTION";
    engine.execute_cypher(query).unwrap();

    // Create a node
    let query = "CREATE (n:Person {name: 'Alice'}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Rollback transaction
    let query = "ROLLBACK TRANSACTION";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
}

#[test]
fn test_create_or_replace_index() {
    let mut engine = create_engine();

    // Create index first
    let query = "CREATE INDEX ON :Person(name)";
    engine.execute_cypher(query).unwrap();

    // Replace index with CREATE OR REPLACE
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

    // CREATE OR REPLACE on non-existent index should create it
    let query = "CREATE OR REPLACE INDEX ON :Person(name)";
    let result = engine.execute_cypher(query).unwrap();
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("ok".to_string()))
    );
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

#[test]
fn test_call_subquery_parsing() {
    use nexus_core::executor::parser::CypherParser;

    // Test CALL { subquery } parsing
    let query = "CALL { MATCH (n:Person) RETURN n.name AS name } RETURN name";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "CALL subquery should parse successfully");
    let ast = result.unwrap();
    
    // Should have CallSubquery clause
    assert!(ast.clauses.iter().any(|c| {
        matches!(c, nexus_core::executor::parser::Clause::CallSubquery(_))
    }), "Should contain CallSubquery clause");
}

#[test]
fn test_call_subquery_in_transactions_parsing() {
    use nexus_core::executor::parser::CypherParser;

    // Test CALL { subquery } IN TRANSACTIONS parsing
    let query = "CALL { MATCH (n:Person) RETURN n } IN TRANSACTIONS";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "CALL IN TRANSACTIONS should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::CallSubquery(call_subquery)) = ast.clauses.first() {
        assert!(call_subquery.in_transactions, "Should have in_transactions flag set");
        assert_eq!(call_subquery.batch_size, None, "Default batch size should be None");
    } else {
        panic!("Should contain CallSubquery clause");
    }
}

#[test]
fn test_call_subquery_in_transactions_with_batch_size_parsing() {
    use nexus_core::executor::parser::CypherParser;

    // Test CALL { subquery } IN TRANSACTIONS OF n ROWS parsing
    let query = "CALL { MATCH (n:Person) RETURN n } IN TRANSACTIONS OF 100 ROWS";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "CALL IN TRANSACTIONS OF n ROWS should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::CallSubquery(call_subquery)) = ast.clauses.first() {
        assert!(call_subquery.in_transactions, "Should have in_transactions flag set");
        assert_eq!(call_subquery.batch_size, Some(100), "Batch size should be 100");
    } else {
        panic!("Should contain CallSubquery clause");
    }
}

#[test]
fn test_call_subquery_execution() {
    let mut engine = create_engine();

    // Create test data
    let query = "CREATE (n:Person {name: 'Alice', age: 30}) RETURN n";
    engine.execute_cypher(query).unwrap();

    // Execute CALL subquery
    let query = "CALL { MATCH (n:Person) RETURN n.name AS name } RETURN name";
    let result = engine.execute_cypher(query).unwrap();

    assert_eq!(result.columns, vec!["name"]);
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        extract_first_row_value(result),
        Some(Value::String("Alice".to_string()))
    );
}

#[test]
fn test_named_path_parsing() {
    use nexus_core::executor::parser::CypherParser;

    // Test path variable assignment: p = (a)-[*]-(b)
    let query = "MATCH p = (a:Person)-[*]-(b:Person) RETURN p";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "Named path should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::Match(match_clause)) = ast.clauses.first() {
        assert_eq!(
            match_clause.pattern.path_variable,
            Some("p".to_string()),
            "Path variable should be 'p'"
        );
    } else {
        panic!("Should contain Match clause");
    }
}

#[test]
fn test_named_path_with_variable_length_parsing() {
    use nexus_core::executor::parser::CypherParser;

    // Test path variable with variable-length relationship
    let query = "MATCH path = (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN path";
    let mut parser = CypherParser::new(query.to_string());
    let result = parser.parse();
    
    assert!(result.is_ok(), "Named path with variable-length should parse successfully");
    let ast = result.unwrap();
    
    if let Some(nexus_core::executor::parser::Clause::Match(match_clause)) = ast.clauses.first() {
        assert_eq!(
            match_clause.pattern.path_variable,
            Some("path".to_string()),
            "Path variable should be 'path'"
        );
        
        // Check that pattern has variable-length relationship
        let has_var_length = match_clause.pattern.elements.iter().any(|e| {
            if let nexus_core::executor::parser::PatternElement::Relationship(rel) = e {
                rel.quantifier.is_some()
            } else {
                false
            }
        });
        assert!(has_var_length, "Pattern should have variable-length relationship");
    } else {
        panic!("Should contain Match clause");
    }
}

#[test]
fn test_named_path_execution() {
    let mut engine = create_engine();

    // Create test data with relationships
    let query = "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})-[:KNOWS]->(c:Person {name: 'Charlie'}) RETURN a, b, c";
    engine.execute_cypher(query).unwrap();

    // Execute query with named path
    let query = "MATCH p = (a:Person)-[:KNOWS*1..2]->(b:Person) RETURN a.name AS start, b.name AS end";
    let result = engine.execute_cypher(query).unwrap();

    assert!(result.rows.len() > 0, "Should return at least one path");
    // Path variable should be accessible in context (though not explicitly returned here)
    assert_eq!(result.columns, vec!["start", "end"]);
}
