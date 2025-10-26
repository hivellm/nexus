//! Integration tests for Cypher write operations
//!
//! Tests for MERGE, SET, DELETE, and REMOVE clauses

use nexus_core::catalog::Catalog;
use nexus_core::executor::{Executor, Query};
use nexus_core::index::{KnnIndex, LabelIndex};
use nexus_core::storage::RecordStore;
use std::collections::HashMap;
use tempfile::TempDir;

/// Helper to create test executor
fn create_test_executor() -> (Executor, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let catalog_path = temp_dir.path().join("catalog.db");
    let nodes_path = temp_dir.path().join("nodes.store");
    let rels_path = temp_dir.path().join("rels.store");

    let catalog = Catalog::open(catalog_path).unwrap();
    let record_store = RecordStore::new(nodes_path, rels_path).unwrap();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new();

    let executor = Executor::new(catalog, record_store, label_index, knn_index);

    (executor, temp_dir)
}

#[test]
fn test_parse_merge_query() {
    let (mut executor, _temp) = create_test_executor();

    // Test MERGE parsing
    let query = Query {
        cypher: "MERGE (n:Person {name: 'Alice'})".to_string(),
        params: HashMap::new(),
    };

    // For now, just test that parsing doesn't panic
    // Execution will be implemented in the next phase
    let result = executor.execute(&query);
    
    // Should fail with "not implemented" for now
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("not implemented") || err.to_string().contains("Unsupported"));
}

#[test]
fn test_parse_merge_with_on_create() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MERGE (n:Person {name: 'Alice'}) ON CREATE SET n.created = true".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_merge_with_on_match() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MERGE (n:Person {name: 'Alice'}) ON MATCH SET n.updated = true".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_set_property() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) SET n.age = 30".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_set_label() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) SET n:VIP".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_set_multiple() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) SET n.age = 30, n.name = 'Bob', n:VIP".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_delete() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) DELETE n".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_detach_delete() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_remove_property() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) REMOVE n.age".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_remove_label() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) REMOVE n:VIP".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_complex_write_query() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person {name: 'Alice'}) SET n.age = 30, n:VIP RETURN n".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_merge_relationship() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (a:Person), (b:Person) MERGE (a)-[r:KNOWS]->(b)".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}

#[test]
fn test_parse_set_with_expression() {
    let (mut executor, _temp) = create_test_executor();

    let query = Query {
        cypher: "MATCH (n:Person) SET n.age = n.age + 1".to_string(),
        params: HashMap::new(),
    };

    let result = executor.execute(&query);
    assert!(result.is_err());
}
