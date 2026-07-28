#![allow(unused_mut)] // test fixtures declare `mut` preemptively

//! Test CREATE with and without RETURN clause
use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use serde_json::Value;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::time::Duration;

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
        let (mut executor, _ctx) = create_test_executor();

        let query = Query {
            cypher: "CREATE (n:Person {name: 'Alice'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // A write-only CREATE (no RETURN/WITH downstream) must yield an
        // EMPTY result set per openCypher/TCK semantics (Create2[2]/[3]/
        // [5]-[12]) — the node is still created, just not projected.
        assert!(
            result.rows.is_empty(),
            "CREATE without RETURN must return an empty result set"
        );
        assert!(result.columns.is_empty());

        // Verify the node was actually created (side effect) via a
        // follow-up MATCH on the same executor/store.
        let verify = Query {
            cypher: "MATCH (n:Person {name: 'Alice'}) RETURN n".to_string(),
            params: std::collections::HashMap::new(),
        };
        let verify_result = executor.execute(&verify).unwrap();
        assert_eq!(verify_result.rows.len(), 1, "node must have been created");
        if let Some(Value::Object(obj)) = verify_result.rows[0].values.first() {
            if let Some(Value::String(name)) = obj.get("name") {
                assert_eq!(name, "Alice");
            } else {
                panic!("Node should have 'name' property");
            }
        } else {
            panic!(
                "Expected node object, got: {:?}",
                verify_result.rows[0].values.first()
            );
        }
    });
}

#[test]
fn test_create_multiple_nodes_without_return() {
    run_with_timeout("test_create_multiple_nodes_without_return", || {
        let (mut executor, _ctx) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Write-only CREATE must yield an empty result set even with
        // multiple bound variables.
        assert!(
            result.rows.is_empty(),
            "CREATE without RETURN must return an empty result set"
        );
        assert!(result.columns.is_empty());

        // Both nodes must still have been created (side effect).
        let verify = Query {
            cypher: "MATCH (n:Person) RETURN count(n) AS c".to_string(),
            params: std::collections::HashMap::new(),
        };
        let verify_result = executor.execute(&verify).unwrap();
        assert_eq!(verify_result.rows[0].values[0].as_i64(), Some(2));
    });
}

#[test]
fn test_create_node_with_multiple_labels_without_return() {
    run_with_timeout(
        "test_create_node_with_multiple_labels_without_return",
        || {
            let (mut executor, _ctx) = create_test_executor();

            let query = Query {
                cypher: "CREATE (n:Person:Employee {name: 'Alice', role: 'Developer'})".to_string(),
                params: std::collections::HashMap::new(),
            };

            let result = executor.execute(&query).unwrap();
            // Write-only CREATE must yield an empty result set.
            assert!(
                result.rows.is_empty(),
                "CREATE without RETURN must return an empty result set"
            );
            assert!(result.columns.is_empty());

            // Verify the node was actually created (side effect) via a
            // follow-up MATCH on the same executor/store.
            let verify = Query {
                cypher: "MATCH (n:Person:Employee {name: 'Alice'}) RETURN n".to_string(),
                params: std::collections::HashMap::new(),
            };
            let verify_result = executor.execute(&verify).unwrap();
            assert_eq!(verify_result.rows.len(), 1, "node must have been created");
            if let Some(Value::Object(obj)) = verify_result.rows[0].values.first() {
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
                    verify_result.rows[0].values.first()
                );
            }
        },
    );
}

#[test]
fn test_create_relationship_without_return() {
    run_with_timeout("test_create_relationship_without_return", || {
        let (mut executor, _ctx) = create_test_executor();

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
        // Write-only `MATCH ... CREATE` (no RETURN) must return an empty
        // result set (routes through `execute_create_with_context` via
        // the main operator loop, not the standalone-CREATE fast path,
        // but shares the same "no downstream Project/With -> empty"
        // contract).
        assert!(
            result.rows.is_empty(),
            "CREATE without RETURN must return an empty result set"
        );
        assert!(result.columns.is_empty());

        // Verify the relationship was actually created (side effect).
        let verify = Query {
            cypher: "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c".to_string(),
            params: std::collections::HashMap::new(),
        };
        let verify_result = executor.execute(&verify).unwrap();
        assert_eq!(verify_result.rows[0].values[0].as_i64(), Some(1));
    });
}

#[test]
fn test_create_path_without_return() {
    run_with_timeout("test_create_path_without_return", || {
        let (mut executor, _ctx) = create_test_executor();

        let query = Query {
            cypher: "CREATE (a:Person {name: 'Alice'})-[r1:KNOWS]->(b:Person {name: 'Bob'})-[r2:KNOWS]->(c:Person {name: 'Charlie'})".to_string(),
            params: std::collections::HashMap::new(),
        };

        let result = executor.execute(&query).unwrap();
        // Write-only CREATE must yield an empty result set even for a
        // multi-hop chained pattern.
        assert!(
            result.rows.is_empty(),
            "CREATE without RETURN must return an empty result set"
        );
        assert!(result.columns.is_empty());

        // All three nodes and both relationships must still have been
        // created (side effect).
        let verify = Query {
            cypher: "MATCH (n:Person) RETURN count(n) AS c".to_string(),
            params: std::collections::HashMap::new(),
        };
        let verify_result = executor.execute(&verify).unwrap();
        assert_eq!(verify_result.rows[0].values[0].as_i64(), Some(3));

        let verify_rels = Query {
            cypher: "MATCH ()-[r:KNOWS]->() RETURN count(r) AS c".to_string(),
            params: std::collections::HashMap::new(),
        };
        let verify_rels_result = executor.execute(&verify_rels).unwrap();
        assert_eq!(verify_rels_result.rows[0].values[0].as_i64(), Some(2));
    });
}

// Tests for CREATE WITH RETURN clause (to ensure both cases work)

#[test]
fn test_create_single_node_with_return() {
    run_with_timeout("test_create_single_node_with_return", || {
        let (mut executor, _ctx) = create_test_executor();

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
        let (mut executor, _ctx) = create_test_executor();

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
        let (mut executor, _ctx) = create_test_executor();

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
        let (mut executor, _ctx) = create_test_executor();

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
        let (mut executor, _ctx) = create_test_executor();

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
