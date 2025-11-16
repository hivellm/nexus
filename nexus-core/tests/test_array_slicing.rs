//! Test Array Slicing operator [start..end]
//! Neo4j compatibility tests for array slicing

use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
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

#[test]
fn test_array_slice_simple() {
    let (mut executor, _dir) = create_test_executor();
    
    // Simple array slicing [1..3] should return elements at index 1 and 2
    let query = Query {
        cypher: "RETURN [1, 2, 3, 4, 5][1..3] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    
    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "result");
    
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(2)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(3)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_slice_from_start() {
    let (mut executor, _dir) = create_test_executor();
    
    // Slice from start [..2] should return first 2 elements
    let query = Query {
        cypher: "RETURN [1, 2, 3, 4, 5][..2] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(1)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(2)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_slice_to_end() {
    let (mut executor, _dir) = create_test_executor();
    
    // Slice to end [2..] should return elements from index 2 to end
    let query = Query {
        cypher: "RETURN [1, 2, 3, 4, 5][2..] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(3)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(4)));
            assert_eq!(arr[2], Value::Number(serde_json::Number::from(5)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_slice_with_negative_indices() {
    let (mut executor, _dir) = create_test_executor();
    
    // Negative indices: [-3..-1] should return last 2 elements (excluding the last one)
    let query = Query {
        cypher: "RETURN [1, 2, 3, 4, 5][-3..-1] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(3)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(4)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_slice_with_property() {
    let (mut executor, _dir) = create_test_executor();
    
    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();
    
    // Create node with array property
    let query = Query {
        cypher: "CREATE (n:Person {scores: [10, 20, 30, 40, 50]})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();
    
    // Slice property array
    let query = Query {
        cypher: "MATCH (n:Person) RETURN n.scores[1..4] AS middle_scores".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();
    
    assert_eq!(result.rows.len(), 1);
    
    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(20)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(30)));
            assert_eq!(arr[2], Value::Number(serde_json::Number::from(40)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

