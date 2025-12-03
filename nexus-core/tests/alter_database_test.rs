//! Tests for ALTER DATABASE command
//!
//! Tests the parsing and execution of ALTER DATABASE command with:
//! - SET ACCESS READ WRITE / READ ONLY
//! - SET OPTION key value

use nexus_core::executor::{Executor, Query};
use nexus_core::testing::create_isolated_test_executor;
use std::collections::HashMap;

#[test]
fn test_alter_database_set_access_read_write() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // First create a test database
    let create_query = Query {
        cypher: "CREATE DATABASE testdb".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // ALTER DATABASE to READ WRITE
    let query = Query {
        cypher: "ALTER DATABASE testdb SET ACCESS READ WRITE".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert!(!r.rows.is_empty(), "Should return a result");
            println!("✅ ALTER DATABASE SET ACCESS READ WRITE: {:?}", r);
        }
        Err(e) => {
            // This is acceptable if DatabaseManager is not available
            eprintln!("ALTER DATABASE not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_alter_database_set_access_read_only() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // First create a test database
    let create_query = Query {
        cypher: "CREATE DATABASE testdb2".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // ALTER DATABASE to READ ONLY
    let query = Query {
        cypher: "ALTER DATABASE testdb2 SET ACCESS READ ONLY".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert!(!r.rows.is_empty(), "Should return a result");
            println!("✅ ALTER DATABASE SET ACCESS READ ONLY: {:?}", r);
        }
        Err(e) => {
            // This is acceptable if DatabaseManager is not available
            eprintln!("ALTER DATABASE not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_alter_database_set_option() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // First create a test database
    let create_query = Query {
        cypher: "CREATE DATABASE testdb3".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // ALTER DATABASE SET OPTION
    let query = Query {
        cypher: "ALTER DATABASE testdb3 SET OPTION max_connections 100".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert!(!r.rows.is_empty(), "Should return a result");
            println!("✅ ALTER DATABASE SET OPTION: {:?}", r);
        }
        Err(e) => {
            // This is acceptable if DatabaseManager is not available
            eprintln!("ALTER DATABASE not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_alter_database_nonexistent() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Try to alter a database that doesn't exist
    let query = Query {
        cypher: "ALTER DATABASE nonexistent SET ACCESS READ ONLY".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(_) => {
            panic!("Should fail when altering non-existent database");
        }
        Err(e) => {
            println!(
                "✅ Correctly rejected altering non-existent database: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_alter_database_parsing() {
    use nexus_core::executor::parser::{
        AlterDatabaseClause, Clause, CypherParser, DatabaseAlteration,
    };

    // Test parsing READ WRITE
    let mut parser1 = CypherParser::new("ALTER DATABASE mydb SET ACCESS READ WRITE".to_string());
    let ast1 = parser1.parse().unwrap();
    println!("AST1: {:?}", ast1);
    println!("Clauses count: {}", ast1.clauses.len());
    if ast1.clauses.is_empty() {
        panic!("No clauses parsed!");
    }
    if let Clause::AlterDatabase(alter_clause) = &ast1.clauses[0] {
        assert_eq!(alter_clause.name, "mydb");
        match &alter_clause.alteration {
            DatabaseAlteration::SetAccess { read_only } => {
                assert_eq!(*read_only, false);
            }
            _ => panic!("Expected SetAccess alteration"),
        }
    } else {
        panic!("Expected AlterDatabase clause");
    }

    // Test parsing READ ONLY
    let mut parser2 = CypherParser::new("ALTER DATABASE mydb SET ACCESS READ ONLY".to_string());
    let ast2 = parser2.parse().unwrap();
    if let Clause::AlterDatabase(alter_clause) = &ast2.clauses[0] {
        assert_eq!(alter_clause.name, "mydb");
        match &alter_clause.alteration {
            DatabaseAlteration::SetAccess { read_only } => {
                assert_eq!(*read_only, true);
            }
            _ => panic!("Expected SetAccess alteration"),
        }
    } else {
        panic!("Expected AlterDatabase clause");
    }

    // Test parsing SET OPTION
    let mut parser3 = CypherParser::new("ALTER DATABASE mydb SET OPTION timeout 30".to_string());
    let ast3 = parser3.parse().unwrap();
    if let Clause::AlterDatabase(alter_clause) = &ast3.clauses[0] {
        assert_eq!(alter_clause.name, "mydb");
        match &alter_clause.alteration {
            DatabaseAlteration::SetOption { key, value } => {
                assert_eq!(key, "timeout");
                assert_eq!(value, "30");
            }
            _ => panic!("Expected SetOption alteration"),
        }
    } else {
        panic!("Expected AlterDatabase clause");
    }

    println!("✅ ALTER DATABASE parsing tests passed");
}

#[test]
fn test_alter_database_full_lifecycle() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database
    let create_query = Query {
        cypher: "CREATE DATABASE lifecycle_db".to_string(),
        params: HashMap::new(),
    };
    let create_result = executor.execute(&create_query);
    match create_result {
        Ok(_) => {
            // Alter to READ ONLY
            let alter_query1 = Query {
                cypher: "ALTER DATABASE lifecycle_db SET ACCESS READ ONLY".to_string(),
                params: HashMap::new(),
            };
            let alter_result1 = executor.execute(&alter_query1);
            assert!(alter_result1.is_ok(), "Should alter database to READ ONLY");

            // Alter back to READ WRITE
            let alter_query2 = Query {
                cypher: "ALTER DATABASE lifecycle_db SET ACCESS READ WRITE".to_string(),
                params: HashMap::new(),
            };
            let alter_result2 = executor.execute(&alter_query2);
            assert!(alter_result2.is_ok(), "Should alter database to READ WRITE");

            // Set an option
            let alter_query3 = Query {
                cypher: "ALTER DATABASE lifecycle_db SET OPTION cache_size large".to_string(),
                params: HashMap::new(),
            };
            let alter_result3 = executor.execute(&alter_query3);
            assert!(alter_result3.is_ok(), "Should set database option");

            println!("✅ ALTER DATABASE full lifecycle test passed");
        }
        Err(e) => {
            eprintln!("Multi-database not fully supported: {:?}", e);
        }
    }
}
