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
fn test_substring_positive_index() {
    let (mut executor, _dir) = create_test_executor();

    let query = Query {
        cypher: "RETURN substring('hello', 1, 3) AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::String("ell".to_string()));
}

#[test]
#[ignore] // TODO: Fix negative index calculation in substring
fn test_substring_negative_index() {
    let (mut executor, _dir) = create_test_executor();

    // Test negative index - should count from end
    let query = Query {
        cypher: "RETURN substring('hello', -3, 2) AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    // -3 from 'hello' (length 5) = position 2, take 2 chars = 'll'
    assert_eq!(result.rows[0].values[0], Value::String("ll".to_string()));
}

#[test]
#[ignore] // TODO: Fix negative index calculation in substring
fn test_substring_negative_index_no_length() {
    let (mut executor, _dir) = create_test_executor();

    // Test negative index without length - should take from that position to end
    let query = Query {
        cypher: "RETURN substring('hello', -2) AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    // -2 from 'hello' = position 3, take rest = 'lo'
    assert_eq!(result.rows[0].values[0], Value::String("lo".to_string()));
}

#[test]
fn test_substring_negative_index_large() {
    let (mut executor, _dir) = create_test_executor();

    // Test negative index larger than string length - should start from beginning
    let query = Query {
        cypher: "RETURN substring('hello', -10, 3) AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    // -10 from 'hello' = position 0 (clamped), take 3 chars = 'hel' (first 3 chars)
    assert_eq!(result.rows[0].values[0], Value::String("hel".to_string()));
}

#[test]
fn test_substring_no_length() {
    let (mut executor, _dir) = create_test_executor();

    let query = Query {
        cypher: "RETURN substring('hello', 2) AS result".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].values[0], Value::String("llo".to_string()));
}
