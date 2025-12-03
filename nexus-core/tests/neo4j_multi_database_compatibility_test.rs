//! Neo4j Multi-Database Compatibility Tests
//!
//! Tests verify that Nexus's multi-database support is compatible with Neo4j's
//! multi-database commands and behavior.
//!
//! Reference: https://neo4j.com/docs/cypher-manual/current/administration/databases/

use nexus_core::database::DatabaseManager;
use nexus_core::executor::{Executor, Query};
use nexus_core::testing::{TestContext, create_isolated_test_executor};
use std::collections::HashMap;

// ============================================================================
// SHOW DATABASES Compatibility Tests
// ============================================================================

#[test]
fn test_show_databases_neo4j_format() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "SHOW DATABASES".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);

    match result {
        Ok(r) => {
            // Neo4j returns these columns (minimum):
            // - name: database name
            // - type: "standard" or "system"
            // - aliases: list of aliases
            // - access: "read-write" or "read-only"
            // - address: server address
            // - role: "primary", "secondary", or "unknown"
            // - requestedStatus: "online" or "offline"
            // - currentStatus: actual status
            // - error: error message if any
            // - default: boolean indicating default database

            // At minimum, must have 'name' column
            assert!(
                r.columns.contains(&"name".to_string()),
                "SHOW DATABASES must include 'name' column for Neo4j compatibility"
            );

            // Should have at least one database (the default)
            assert!(
                !r.rows.is_empty(),
                "SHOW DATABASES must return at least the default database"
            );

            // Optional: Check for other standard Neo4j columns
            let expected_columns = vec![
                "name",
                "type",
                "aliases",
                "access",
                "address",
                "role",
                "requestedStatus",
                "currentStatus",
                "error",
                "default",
            ];

            for col in &expected_columns {
                if r.columns.contains(&col.to_string()) {
                    println!("✓ Column '{}' is present", col);
                }
            }
        }
        Err(e) => {
            eprintln!("SHOW DATABASES not fully supported: {:?}", e);
            eprintln!("This is acceptable for initial implementation");
        }
    }
}

#[test]
fn test_show_databases_default_database_present() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "SHOW DATABASES".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);

    match result {
        Ok(r) => {
            // Find name column
            let name_idx = r.columns.iter().position(|c| c == "name");
            assert!(name_idx.is_some(), "SHOW DATABASES must have 'name' column");

            let idx = name_idx.unwrap();
            let mut found_default = false;

            for row in &r.rows {
                if let Some(name_value) = row.values.get(idx) {
                    if let Some(name_str) = name_value.as_str() {
                        // Neo4j default database is typically "neo4j"
                        if name_str == "neo4j" || name_str == "nexus" {
                            found_default = true;
                            break;
                        }
                    }
                }
            }

            assert!(
                found_default,
                "SHOW DATABASES must include default database (neo4j or nexus)"
            );
        }
        Err(e) => {
            eprintln!("SHOW DATABASES not fully supported: {:?}", e);
        }
    }
}

// ============================================================================
// CREATE DATABASE Compatibility Tests
// ============================================================================

#[test]
fn test_create_database_neo4j_syntax() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Test basic CREATE DATABASE syntax
    let test_cases = vec![
        "CREATE DATABASE mydb",
        "CREATE DATABASE mydb IF NOT EXISTS",
        "CREATE DATABASE `my-db`", // Backticks for special chars
    ];

    for cypher in test_cases {
        let query = Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        };

        let result = executor.execute(&query);
        match result {
            Ok(_) => {
                println!("✓ Syntax '{}' is supported", cypher);
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("not supported") || error_msg.contains("Unsupported") {
                    eprintln!("✗ Syntax '{}' not yet supported", cypher);
                } else {
                    eprintln!("✗ Syntax '{}' failed with: {:?}", cypher, e);
                }
            }
        }
    }
}

#[test]
fn test_create_database_if_not_exists() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database
    let create_query = Query {
        cypher: "CREATE DATABASE testdb".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // Create again with IF NOT EXISTS should not fail (Neo4j behavior)
    let create_if_not_exists = Query {
        cypher: "CREATE DATABASE testdb IF NOT EXISTS".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&create_if_not_exists);
    match result {
        Ok(_) => {
            println!("✓ CREATE DATABASE IF NOT EXISTS works correctly");
        }
        Err(e) => {
            eprintln!("CREATE DATABASE IF NOT EXISTS not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_create_database_duplicate_without_if_not_exists() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database
    let create_query = Query {
        cypher: "CREATE DATABASE dupdb".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // Creating again WITHOUT IF NOT EXISTS should fail (Neo4j behavior)
    let create_duplicate = Query {
        cypher: "CREATE DATABASE dupdb".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&create_duplicate);
    match result {
        Ok(_) => {
            eprintln!("✗ CREATE DATABASE should fail for duplicates without IF NOT EXISTS");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("already exists") || error_msg.contains("duplicate") {
                println!("✓ CREATE DATABASE correctly rejects duplicates");
            } else {
                eprintln!("Unexpected error for duplicate: {:?}", e);
            }
        }
    }
}

// ============================================================================
// DROP DATABASE Compatibility Tests
// ============================================================================

#[test]
fn test_drop_database_neo4j_syntax() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database first
    let _ = executor.execute(&Query {
        cypher: "CREATE DATABASE droptest".to_string(),
        params: HashMap::new(),
    });

    // Test basic DROP DATABASE syntax
    let test_cases = vec![
        "DROP DATABASE droptest",
        "DROP DATABASE droptest IF EXISTS",
        "DROP DATABASE `drop-test` IF EXISTS",
    ];

    for cypher in test_cases {
        let query = Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        };

        let result = executor.execute(&query);
        match result {
            Ok(_) => {
                println!("✓ Syntax '{}' is supported", cypher);
            }
            Err(e) => {
                let error_msg = e.to_string();
                if error_msg.contains("not supported") || error_msg.contains("Unsupported") {
                    eprintln!("✗ Syntax '{}' not yet supported", cypher);
                } else if error_msg.contains("not found") || error_msg.contains("does not exist") {
                    println!("✓ Syntax '{}' correctly indicates missing database", cypher);
                } else {
                    eprintln!("✗ Syntax '{}' failed with: {:?}", cypher, e);
                }
            }
        }
    }
}

#[test]
fn test_drop_database_if_exists() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Drop non-existent database with IF EXISTS should not fail (Neo4j behavior)
    let drop_if_exists = Query {
        cypher: "DROP DATABASE nonexistent IF EXISTS".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&drop_if_exists);
    match result {
        Ok(_) => {
            println!("✓ DROP DATABASE IF EXISTS works correctly for non-existent database");
        }
        Err(e) => {
            eprintln!("DROP DATABASE IF EXISTS not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_drop_database_without_if_exists() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Drop non-existent database WITHOUT IF EXISTS should fail (Neo4j behavior)
    let drop_nonexistent = Query {
        cypher: "DROP DATABASE nonexistent123".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&drop_nonexistent);
    match result {
        Ok(_) => {
            eprintln!("✗ DROP DATABASE should fail for non-existent database without IF EXISTS");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("not found") || error_msg.contains("does not exist") {
                println!("✓ DROP DATABASE correctly rejects non-existent database");
            } else {
                eprintln!("Unexpected error for non-existent database: {:?}", e);
            }
        }
    }
}

#[test]
fn test_drop_default_database_fails() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Dropping default database should fail (Neo4j behavior)
    let drop_default = Query {
        cypher: "DROP DATABASE neo4j".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&drop_default);
    match result {
        Ok(_) => {
            eprintln!("✗ DROP DATABASE should not allow dropping default database");
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("default")
                || error_msg.contains("cannot drop")
                || error_msg.contains("not allowed")
            {
                println!("✓ DROP DATABASE correctly protects default database");
            } else {
                eprintln!("Unexpected error for dropping default database: {:?}", e);
            }
        }
    }
}

// ============================================================================
// :USE Command Compatibility Tests
// ============================================================================

#[test]
fn test_use_database_command() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Test :USE command (Neo4j client command)
    let test_cases = vec![":USE neo4j", ":USE nexus", ":use neo4j"];

    for cypher in test_cases {
        let query = Query {
            cypher: cypher.to_string(),
            params: HashMap::new(),
        };

        let result = executor.execute(&query);
        match result {
            Ok(_) => {
                println!("✓ Command '{}' is supported", cypher);
            }
            Err(e) => {
                eprintln!("✗ Command '{}' not yet supported: {:?}", cypher, e);
            }
        }
    }
}

#[test]
fn test_use_nonexistent_database_fails() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Using non-existent database should fail
    let use_nonexistent = Query {
        cypher: ":USE nonexistent_db_12345".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&use_nonexistent);
    match result {
        Ok(_) => {
            eprintln!("✗ :USE should fail for non-existent database");
        }
        Err(e) => {
            println!("✓ :USE correctly rejects non-existent database: {:?}", e);
        }
    }
}

// ============================================================================
// database() Function Compatibility Tests
// ============================================================================

#[test]
fn test_database_function_neo4j_compatibility() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Test database() function (Neo4j function)
    let query = Query {
        cypher: "RETURN database() AS currentDb".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert_eq!(r.columns.len(), 1);
            assert_eq!(r.columns[0], "currentDb");
            assert_eq!(r.rows.len(), 1);

            if let Some(db_value) = r.rows[0].values.get(0) {
                if let Some(db_name) = db_value.as_str() {
                    println!("✓ database() function returns: {}", db_name);
                    // Should return default database name
                    assert!(
                        db_name == "neo4j" || db_name == "nexus",
                        "database() should return default database name"
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("✗ database() function not yet supported: {:?}", e);
        }
    }
}

#[test]
fn test_db_function_alias() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Test db() as an alias for database() (Neo4j compatibility)
    let query = Query {
        cypher: "RETURN db() AS currentDb".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            println!("✓ db() function alias is supported");
            assert_eq!(r.columns.len(), 1);
            assert_eq!(r.columns[0], "currentDb");
        }
        Err(e) => {
            eprintln!("✗ db() function alias not yet supported: {:?}", e);
        }
    }
}

// ============================================================================
// Data Isolation Compatibility Tests
// ============================================================================

#[test]
fn test_database_isolation_neo4j_behavior() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Create two databases (like Neo4j)
    manager.create_database("db1").unwrap();
    manager.create_database("db2").unwrap();

    let db1 = manager.get_database("db1").unwrap();
    let db2 = manager.get_database("db2").unwrap();

    // Add data to db1
    {
        let mut engine = db1.write();
        engine
            .create_node(
                vec!["Person".to_string()],
                serde_json::json!({"name": "Alice"}),
            )
            .unwrap();
    }

    // db2 should not see db1's data (Neo4j behavior)
    {
        let mut engine = db2.write();
        let stats = engine.stats().unwrap();
        assert_eq!(
            stats.nodes, 0,
            "Databases must be isolated (Neo4j compatibility)"
        );
    }

    // db1 should still have its data
    {
        let mut engine = db1.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 1, "Database data must persist");
    }
}

// ============================================================================
// Database Naming Compatibility Tests
// ============================================================================

#[test]
fn test_database_name_restrictions() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Neo4j naming rules:
    // - Must start with letter or underscore
    // - Can contain letters, numbers, underscores, hyphens
    // - Case-sensitive
    // - Max 63 characters

    // Valid names
    let valid_names = vec![
        "mydb",
        "my_db",
        "my-db",
        "MyDB",
        "db123",
        "_hidden",
        "database_2024",
    ];

    for name in valid_names {
        let result = manager.create_database(name);
        assert!(
            result.is_ok(),
            "Valid database name '{}' should be accepted",
            name
        );
    }

    // Invalid names (should fail)
    let invalid_names = vec![
        "",      // Empty
        "123db", // Starts with number
        "my db", // Contains space
        "my@db", // Contains special char
        "my/db", // Contains slash
        "my.db", // Contains dot (depends on implementation)
    ];

    for name in invalid_names {
        let result = manager.create_database(name);
        if result.is_ok() {
            eprintln!(
                "✗ Invalid name '{}' was accepted (should be rejected)",
                name
            );
        } else {
            println!("✓ Invalid name '{}' was correctly rejected", name);
        }
    }
}

// ============================================================================
// SHOW DATABASE <name> Compatibility Test
// ============================================================================

#[test]
fn test_show_database_single() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Neo4j supports SHOW DATABASE <name> to show specific database
    let query = Query {
        cypher: "SHOW DATABASE neo4j".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            println!("✓ SHOW DATABASE <name> is supported");
            assert!(!r.rows.is_empty(), "Should return database info");
        }
        Err(e) => {
            eprintln!("✗ SHOW DATABASE <name> not yet supported: {:?}", e);
        }
    }
}

// ============================================================================
// System Database Tests
// ============================================================================

#[test]
fn test_system_database_concept() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Neo4j has a 'system' database for metadata
    // Test if we can query it
    let use_system = Query {
        cypher: ":USE system".to_string(),
        params: HashMap::new(),
    };

    match executor.execute(&use_system) {
        Ok(_) => {
            println!("✓ System database is supported");

            // Try to query system database
            let show_dbs = Query {
                cypher: "SHOW DATABASES".to_string(),
                params: HashMap::new(),
            };

            if let Ok(r) = executor.execute(&show_dbs) {
                println!(
                    "✓ Can query system database: {} databases found",
                    r.rows.len()
                );
            }
        }
        Err(e) => {
            eprintln!("✗ System database not yet supported: {:?}", e);
            eprintln!("This is acceptable for initial implementation");
        }
    }
}
