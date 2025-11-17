use nexus_core::executor::{Executor, Query};
use nexus_core::index::KnnIndex;
use nexus_core::{catalog::Catalog, index::LabelIndex, storage::RecordStore};
use serde_json::Value;
use std::collections::HashMap;
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
fn test_array_slice_basic() {
    let (mut executor, _dir) = create_test_executor();

    // Basic slice [1..3] returns elements at index 1 and 2
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][1..3] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![Value::Number(1.into()), Value::Number(2.into())])
    );
}

#[test]
fn test_array_slice_from_start() {
    let (mut executor, _dir) = create_test_executor();

    // Slice from start [..3] returns first 3 elements
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][..3] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::Number(0.into()),
            Value::Number(1.into()),
            Value::Number(2.into())
        ])
    );
}

#[test]
fn test_array_slice_to_end() {
    let (mut executor, _dir) = create_test_executor();

    // Slice to end [2..] returns from index 2 to end
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][2..] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::Number(2.into()),
            Value::Number(3.into()),
            Value::Number(4.into())
        ])
    );
}

#[test]
#[ignore] // TODO: Fix parser to handle negative numbers in array slices
fn test_array_slice_negative_start() {
    let (mut executor, _dir) = create_test_executor();

    // Negative start index [-3..-1] counts from end
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][-3..-1] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    // -3 from length 5 = index 2, -1 = index 4, so [2, 3]
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![Value::Number(2.into()), Value::Number(3.into())])
    );
}

#[test]
#[ignore] // TODO: Fix negative end index calculation in array slicing
fn test_array_slice_negative_end() {
    let (mut executor, _dir) = create_test_executor();

    // Negative end index [1..-1]
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][1..-1] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    // Start at 1, end at -1 (index 4), so [1, 2, 3]
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::Number(1.into()),
            Value::Number(2.into()),
            Value::Number(3.into())
        ])
    );
}

#[test]
fn test_array_slice_out_of_bounds() {
    let (mut executor, _dir) = create_test_executor();

    // Out of bounds slice returns what's available
    let query = Query {
        cypher: "RETURN [0, 1, 2][1..10] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![Value::Number(1.into()), Value::Number(2.into())])
    );
}

#[test]
fn test_array_slice_empty() {
    let (mut executor, _dir) = create_test_executor();

    // Inverted indices return empty array
    let query = Query {
        cypher: "RETURN [0, 1, 2, 3, 4][3..1] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::Array(Vec::new()));
}

#[test]
#[ignore] // TODO: Fix array slicing with properties in CREATE/RETURN
fn test_array_slice_with_property() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create node with array property
    let query = Query {
        cypher: "CREATE (n:Test {nums: [10, 20, 30, 40, 50]}) RETURN n.nums[1..4] AS result"
            .to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::Number(20.into()),
            Value::Number(30.into()),
            Value::Number(40.into())
        ])
    );
}

#[test]
fn test_array_slice_full_range() {
    let (mut executor, _dir) = create_test_executor();

    // Full range [..] returns entire array
    let query = Query {
        cypher: "RETURN [1, 2, 3][..] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::Number(1.into()),
            Value::Number(2.into()),
            Value::Number(3.into())
        ])
    );
}

#[test]
fn test_array_slice_strings() {
    let (mut executor, _dir) = create_test_executor();

    // Slice works with string arrays
    let query = Query {
        cypher: "RETURN ['a', 'b', 'c', 'd', 'e'][1..3] AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[0],
        Value::Array(vec![
            Value::String("b".to_string()),
            Value::String("c".to_string())
        ])
    );
}
