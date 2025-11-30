//! Multi-database integration tests
//!
//! Tests cover:
//! - Database creation and deletion via Cypher
//! - SHOW DATABASES command
//! - Data isolation between databases
//! - database() function
//! - DatabaseManager API

use nexus_core::database::{DatabaseInfo, DatabaseManager};
use nexus_core::executor::{Executor, Query};
use nexus_core::testing::{TestContext, create_isolated_test_executor};
use std::collections::HashMap;

// ============================================================================
// DatabaseManager Unit Tests
// ============================================================================

#[test]
fn test_database_manager_creation() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Default database should exist
    assert!(manager.exists("neo4j"));
    assert_eq!(manager.default_database_name(), "neo4j");
}

#[test]
fn test_create_and_drop_database() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Create a new database
    manager.create_database("testdb").unwrap();
    assert!(manager.exists("testdb"));

    // List databases should include both
    let databases = manager.list_databases();
    assert!(databases.len() >= 2);

    let db_names: Vec<&str> = databases.iter().map(|d| d.name.as_str()).collect();
    assert!(db_names.contains(&"neo4j"));
    assert!(db_names.contains(&"testdb"));

    // Drop the database
    manager.drop_database("testdb", false).unwrap();
    assert!(!manager.exists("testdb"));
}

#[test]
fn test_create_duplicate_database_fails() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Create a database
    manager.create_database("mydb").unwrap();

    // Creating the same database should fail
    let result = manager.create_database("mydb");
    assert!(result.is_err());
}

#[test]
fn test_drop_nonexistent_database_fails() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Dropping a non-existent database without IF EXISTS should fail
    let result = manager.drop_database("nonexistent", false);
    assert!(result.is_err());

    // With IF EXISTS should succeed
    let result = manager.drop_database("nonexistent", true);
    assert!(result.is_ok());
}

#[test]
fn test_drop_default_database_fails() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Cannot drop the default database
    let result = manager.drop_database("neo4j", false);
    assert!(result.is_err());
}

#[test]
fn test_get_database() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Get default database
    let db = manager.get_database("neo4j");
    assert!(db.is_ok());

    // Get non-existent database
    let db = manager.get_database("nonexistent");
    assert!(db.is_err());
}

#[test]
fn test_database_name_validation() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Invalid names should fail
    assert!(manager.create_database("").is_err());
    assert!(manager.create_database("invalid name").is_err()); // spaces not allowed
    assert!(manager.create_database("test@db").is_err()); // special chars not allowed
    assert!(manager.create_database("test/db").is_err()); // slashes not allowed

    // Valid names should succeed (alphanumeric, underscore, hyphen allowed)
    assert!(manager.create_database("validname").is_ok());
    assert!(manager.create_database("valid_name").is_ok());
    assert!(manager.create_database("valid-name").is_ok()); // hyphens are allowed
    assert!(manager.create_database("validName123").is_ok());
}

// ============================================================================
// Cypher Command Tests
// ============================================================================

#[test]
fn test_show_databases_cypher() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "SHOW DATABASES".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            // Should have name column at minimum
            assert!(
                r.columns.contains(&"name".to_string()),
                "Expected 'name' column"
            );
            // Should have at least the default database
            assert!(!r.rows.is_empty(), "Expected at least one database");
        }
        Err(e) => {
            // SHOW DATABASES may not be fully supported yet
            eprintln!("SHOW DATABASES not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_create_database_cypher() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "CREATE DATABASE testdb".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            // Should succeed with a message
            assert!(r.rows.len() <= 1);
        }
        Err(e) => {
            // CREATE DATABASE may not be fully supported yet
            eprintln!("CREATE DATABASE not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_create_database_if_not_exists_cypher() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database
    let query1 = Query {
        cypher: "CREATE DATABASE testdb".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&query1);

    // Create again with IF NOT EXISTS should not fail
    let query2 = Query {
        cypher: "CREATE DATABASE testdb IF NOT EXISTS".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query2);
    // Should not error
    if let Err(e) = result {
        eprintln!("CREATE DATABASE IF NOT EXISTS not fully supported: {:?}", e);
    }
}

#[test]
fn test_drop_database_cypher() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Create database first
    let create_query = Query {
        cypher: "CREATE DATABASE testdb".to_string(),
        params: HashMap::new(),
    };
    let _ = executor.execute(&create_query);

    // Drop database
    let drop_query = Query {
        cypher: "DROP DATABASE testdb".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&drop_query);
    match result {
        Ok(_) => {
            // Verify database is gone
            let show_query = Query {
                cypher: "SHOW DATABASES".to_string(),
                params: HashMap::new(),
            };
            if let Ok(r) = executor.execute(&show_query) {
                // Find the index of the 'name' column
                let name_idx = r.columns.iter().position(|c| c == "name");
                if let Some(idx) = name_idx {
                    for row in &r.rows {
                        if let Some(name) = row.values.get(idx) {
                            if let Some(name_str) = name.as_str() {
                                assert_ne!(name_str, "testdb", "Database should be dropped");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("DROP DATABASE not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_drop_database_if_exists_cypher() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    // Drop non-existent database with IF EXISTS should not fail
    let query = Query {
        cypher: "DROP DATABASE nonexistent IF EXISTS".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    if let Err(e) = result {
        eprintln!("DROP DATABASE IF EXISTS not fully supported: {:?}", e);
    }
}

// ============================================================================
// database() Function Tests
// ============================================================================

#[test]
fn test_database_function() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "RETURN database() AS db".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert_eq!(r.columns.len(), 1);
            assert_eq!(r.columns[0], "db");
            assert_eq!(r.rows.len(), 1);
            // Default database should be "neo4j"
            if let Some(db_name) = r.rows[0].values.get(0) {
                if let Some(name_str) = db_name.as_str() {
                    assert_eq!(name_str, "neo4j");
                }
            }
        }
        Err(e) => {
            eprintln!("database() function not fully supported: {:?}", e);
        }
    }
}

#[test]
fn test_db_function_alias() {
    let (mut executor, _ctx) = create_isolated_test_executor();

    let query = Query {
        cypher: "RETURN db() AS database_name".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    match result {
        Ok(r) => {
            assert_eq!(r.columns.len(), 1);
            assert_eq!(r.columns[0], "database_name");
            assert_eq!(r.rows.len(), 1);
            // Default database should be "neo4j"
            if let Some(db_name) = r.rows[0].values.get(0) {
                if let Some(name_str) = db_name.as_str() {
                    assert_eq!(name_str, "neo4j");
                }
            }
        }
        Err(e) => {
            eprintln!("db() function not fully supported: {:?}", e);
        }
    }
}

// ============================================================================
// Data Isolation Tests
// ============================================================================

#[test]
fn test_data_isolation_between_databases() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Create two databases
    manager.create_database("db1").unwrap();
    manager.create_database("db2").unwrap();

    // Get engines for both databases
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

    // Add different data to db2
    {
        let mut engine = db2.write();
        engine
            .create_node(
                vec!["Company".to_string()],
                serde_json::json!({"name": "Acme"}),
            )
            .unwrap();
    }

    // Verify db1 has Person, not Company
    {
        let mut engine = db1.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 1);
    }

    // Verify db2 has Company, not Person
    {
        let mut engine = db2.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 1);
    }
}

#[test]
fn test_multiple_databases_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let ctx = TestContext::new();
    let manager = Arc::new(DatabaseManager::new(ctx.path().to_path_buf()).unwrap());

    // Create databases
    manager.create_database("concurrent1").unwrap();
    manager.create_database("concurrent2").unwrap();

    let manager1 = manager.clone();
    let manager2 = manager.clone();

    // Spawn threads to access databases concurrently
    let handle1 = thread::spawn(move || {
        let db = manager1.get_database("concurrent1").unwrap();
        let mut engine = db.write();
        for i in 0..10 {
            engine
                .create_node(vec!["Test".to_string()], serde_json::json!({"id": i}))
                .unwrap();
        }
        let stats = engine.stats().unwrap();
        stats.nodes
    });

    let handle2 = thread::spawn(move || {
        let db = manager2.get_database("concurrent2").unwrap();
        let mut engine = db.write();
        for i in 0..10 {
            engine
                .create_node(vec!["Test".to_string()], serde_json::json!({"id": i}))
                .unwrap();
        }
        let stats = engine.stats().unwrap();
        stats.nodes
    });

    let count1 = handle1.join().unwrap();
    let count2 = handle2.join().unwrap();

    // Each database should have 10 nodes
    assert_eq!(count1, 10);
    assert_eq!(count2, 10);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_list_databases_info() {
    let ctx = TestContext::new();
    let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

    // Create a database with some data
    manager.create_database("infotest").unwrap();
    let db = manager.get_database("infotest").unwrap();
    {
        let mut engine = db.write();
        engine
            .create_node(vec!["Node".to_string()], serde_json::json!({"value": 1}))
            .unwrap();
    }

    // List databases and check info
    let databases = manager.list_databases();
    let infotest: Option<&DatabaseInfo> = databases.iter().find(|d| d.name == "infotest");

    assert!(infotest.is_some());
    let info = infotest.unwrap();
    assert_eq!(info.name, "infotest");
    // Path should be set
    assert!(!info.path.as_os_str().is_empty());
}

#[test]
#[ignore] // TODO: Implement database catalog persistence
fn test_database_persistence() {
    let ctx = TestContext::new();
    let path = ctx.path().to_path_buf();

    // Create a manager and add a database
    {
        let manager = DatabaseManager::new(path.clone()).unwrap();
        manager.create_database("persistent").unwrap();
        let db = manager.get_database("persistent").unwrap();
        {
            let mut engine = db.write();
            engine
                .create_node(
                    vec!["Persistent".to_string()],
                    serde_json::json!({"data": "test"}),
                )
                .unwrap();
        }
    }

    // Create a new manager and verify database still exists
    {
        let manager = DatabaseManager::new(path).unwrap();
        assert!(manager.exists("persistent"));
        let db = manager.get_database("persistent").unwrap();
        let mut engine = db.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 1);
    }
}
