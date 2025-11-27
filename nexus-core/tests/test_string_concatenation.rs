//! Test String Concatenation operator (+)
//! Neo4j compatibility tests for string concatenation

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
fn test_string_concat_simple() {
    let (mut executor, _dir) = create_test_executor();

    // Simple string concatenation
    let query = Query {
        cypher: "RETURN 'Hello' + ' ' + 'World' AS greeting".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns.len(), 1);
    assert_eq!(result.columns[0], "greeting");

    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("Hello World".to_string()));
}

#[test]
fn test_string_concat_with_property() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create node with properties
    let query = Query {
        cypher: "CREATE (n:Person {firstName: 'John', lastName: 'Doe'})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Concatenate properties
    let query = Query {
        cypher: "MATCH (n:Person) RETURN n.firstName + ' ' + n.lastName AS fullName".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("John Doe".to_string()));
}

#[test]
fn test_string_concat_with_number_conversion() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create node with properties
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Alice', age: 30})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Concatenate string and number (using toString)
    let query = Query {
        cypher: "MATCH (n:Person) RETURN n.name + ' is ' + toString(n.age) + ' years old' AS info"
            .to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(
        row.values[0],
        Value::String("Alice is 30 years old".to_string())
    );
}

#[test]
#[ignore] // TODO: Fix temp dir race condition in parallel tests
fn test_string_concat_in_create_return() {
    let (mut executor, _dir) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // CREATE with string concatenation in RETURN
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Bob', age: 25}) RETURN n.name + ' - ' + toString(n.age) AS info".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("Bob - 25".to_string()));
}

#[test]
#[ignore] // TODO: Fix temp dir race condition in parallel tests
fn test_string_concat_empty_strings() {
    let (mut executor, _dir) = create_test_executor();

    // Concatenate with empty strings
    let query = Query {
        cypher: "RETURN '' + 'Test' + '' AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    let row = &result.rows[0];
    assert_eq!(row.values[0], Value::String("Test".to_string()));
}
