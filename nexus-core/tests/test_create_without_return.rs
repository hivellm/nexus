//! Test CREATE with and without RETURN clause
use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Duration;
use tempfile::TempDir;

fn create_test_executor() -> (Executor, TempDir) {
    let dir = TempDir::new().unwrap();
    let catalog = Catalog::new(dir.path()).unwrap();
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new_default(128).unwrap();

    let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
    (executor, dir)
}

fn run_with_timeout<F>(name: &str, f: F)
where
    F: FnOnce() + Send + 'static,
{
    const TIMEOUT_SECS: u64 = 10;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = panic::catch_unwind(AssertUnwindSafe(f));
        let _ = tx.send(result);
    });

    match rx.recv_timeout(Duration::from_secs(TIMEOUT_SECS)) {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            if let Some(msg) = err.downcast_ref::<&str>() {
                panic!("Test '{}' panicked: {}", name, msg);
            } else if let Some(msg) = err.downcast_ref::<String>() {
                panic!("Test '{}' panicked: {}", name, msg);
            } else {
                panic!("Test '{}' panicked with unknown error", name);
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("Test '{}' exceeded {} seconds timeout", name, TIMEOUT_SECS);
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("Test '{}' panicked (channel disconnected)", name);
        }
    }
}

#[test]
fn test_create_single_node_without_return() {
    run_with_timeout("test_create_single_node_without_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (n:Person {name: 'Alice'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return the created node even without RETURN
        assert!(
            !result.rows.is_empty(),
            "CREATE without RETURN should return created node"
        );
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0], "n");

        // Verify the node has the correct properties
        if let Some(Value::Object(obj)) = result.rows[0].values.first() {
            if let Some(Value::String(name)) = obj.get("name") {
                assert_eq!(name, "Alice");
            } else {
                panic!("Node should have 'name' property");
            }
        } else {
            panic!(
                "Expected node object, got: {:?}",
                result.rows[0].values.first()
            );
        }
    });
}

#[test]
fn test_create_multiple_nodes_without_return() {
    run_with_timeout("test_create_multiple_nodes_without_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return both created nodes
        assert!(
            !result.rows.is_empty(),
            "CREATE without RETURN should return created nodes"
        );
        assert_eq!(result.columns.len(), 2);
        assert!(result.columns.contains(&"a".to_string()));
        assert!(result.columns.contains(&"b".to_string()));
    });
}

#[test]
fn test_create_node_with_multiple_labels_without_return() {
    run_with_timeout(
        "test_create_node_with_multiple_labels_without_return",
        || {
            let (mut executor, _dir) = create_test_executor();

            let query = Query {
                cypher: "CREATE (n:Person:Employee {name: 'Alice', role: 'Developer'})".to_string(),
                params: std::collections::HashMap::new(),
            };

            let result = executor.execute(&query).unwrap();
            // Should return the created node even without RETURN
            assert!(
                !result.rows.is_empty(),
                "CREATE without RETURN should return created node"
            );
            assert_eq!(result.columns.len(), 1);
            assert_eq!(result.columns[0], "n");

            // Verify the node has the correct properties
            if let Some(Value::Object(obj)) = result.rows[0].values.first() {
                if let Some(Value::String(name)) = obj.get("name") {
                    assert_eq!(name, "Alice");
                } else {
                    panic!("Node should have 'name' property");
                }
                if let Some(Value::String(role)) = obj.get("role") {
                    assert_eq!(role, "Developer");
                } else {
                    panic!("Node should have 'role' property");
                }
            } else {
                panic!(
                    "Expected node object, got: {:?}",
                    result.rows[0].values.first()
                );
            }
        },
    );
}

#[test]
fn test_create_relationship_without_return() {
    run_with_timeout("test_create_relationship_without_return", || {
        let (mut executor, _dir) = create_test_executor();

        // First create two nodes
        let create_nodes_query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
            params: std::collections::HashMap::new(),
        };
        executor.execute(&create_nodes_query).unwrap();

        // Now create a relationship between them
        let query = Query {
            cypher: "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[r:KNOWS {since: 2020}]->(b)".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return the created relationship even without RETURN
        assert!(
            !result.rows.is_empty(),
            "CREATE without RETURN should return created relationship"
        );
        // Should have at least the relationship variable
        assert!(
            result.columns.contains(&"r".to_string())
                || result.columns.contains(&"a".to_string())
                || result.columns.contains(&"b".to_string())
        );
    });
}

#[test]
fn test_create_path_without_return() {
    run_with_timeout("test_create_path_without_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'})-[r1:KNOWS]->(b:Person {name: 'Bob'})-[r2:KNOWS]->(c:Person {name: 'Charlie'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return created entities (nodes and relationships)
        assert!(
            !result.rows.is_empty(),
            "CREATE without RETURN should return created path"
        );
        // Should have variables for nodes and relationships
        assert!(
            result.columns.len() >= 3,
            "Should have at least 3 variables (a, b, c or r1, r2)"
        );
    });
}

// Tests for CREATE WITH RETURN clause (to ensure both cases work)

#[test]
fn test_create_single_node_with_return() {
    run_with_timeout("test_create_single_node_with_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (n:Person {name: 'Alice'}) RETURN n".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return the created node with RETURN clause
        assert!(
            !result.rows.is_empty(),
            "CREATE with RETURN should return created node"
        );
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0], "n");

        // Verify the node has the correct properties
        if let Some(Value::Object(obj)) = result.rows[0].values.first() {
            if let Some(Value::String(name)) = obj.get("name") {
                assert_eq!(name, "Alice");
            } else {
                panic!("Node should have 'name' property");
            }
        } else {
            panic!(
                "Expected node object, got: {:?}",
                result.rows[0].values.first()
            );
        }
    });
}

#[test]
fn test_create_multiple_nodes_with_return() {
    run_with_timeout("test_create_multiple_nodes_with_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) RETURN a, b"
                .to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return both created nodes with RETURN clause
        assert!(
            !result.rows.is_empty(),
            "CREATE with RETURN should return created nodes"
        );
        assert_eq!(result.columns.len(), 2);
        assert!(result.columns.contains(&"a".to_string()));
        assert!(result.columns.contains(&"b".to_string()));
    });
}

#[test]
fn test_create_node_with_multiple_labels_with_return() {
    run_with_timeout("test_create_node_with_multiple_labels_with_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (n:Person:Employee {name: 'Alice', role: 'Developer'}) RETURN n"
                .to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return the created node with RETURN clause
        assert!(
            !result.rows.is_empty(),
            "CREATE with RETURN should return created node"
        );
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.columns[0], "n");

        // Verify the node has the correct properties
        if let Some(Value::Object(obj)) = result.rows[0].values.first() {
            if let Some(Value::String(name)) = obj.get("name") {
                assert_eq!(name, "Alice");
            } else {
                panic!("Node should have 'name' property");
            }
            if let Some(Value::String(role)) = obj.get("role") {
                assert_eq!(role, "Developer");
            } else {
                panic!("Node should have 'role' property");
            }
        } else {
            panic!(
                "Expected node object, got: {:?}",
                result.rows[0].values.first()
            );
        }
    });
}

#[test]
fn test_create_relationship_with_return() {
    run_with_timeout("test_create_relationship_with_return", || {
        let (mut executor, _dir) = create_test_executor();

        // First create two nodes
        let create_nodes_query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
            params: std::collections::HashMap::new(),
        };
        executor.execute(&create_nodes_query).unwrap();

        // Now create a relationship between them with RETURN
        let query = Query {
            cypher: "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}) CREATE (a)-[r:KNOWS {since: 2020}]->(b) RETURN r".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return the created relationship with RETURN clause
        assert!(
            !result.rows.is_empty(),
            "CREATE with RETURN should return created relationship"
        );
        assert!(result.columns.contains(&"r".to_string()));
    });
}

#[test]
fn test_create_path_with_return() {
    run_with_timeout("test_create_path_with_return", || {
        let (mut executor, _dir) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'})-[r1:KNOWS]->(b:Person {name: 'Bob'})-[r2:KNOWS]->(c:Person {name: 'Charlie'}) RETURN a, b, c, r1, r2".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Should return created entities (nodes and relationships) with RETURN clause
        assert!(
            !result.rows.is_empty(),
            "CREATE with RETURN should return created path"
        );
        // Should have variables for nodes and relationships
        assert!(
            result.columns.len() >= 3,
            "Should have at least 3 variables"
        );
        assert!(result.columns.contains(&"a".to_string()));
        assert!(result.columns.contains(&"b".to_string()));
        assert!(result.columns.contains(&"c".to_string()));
    });
}
