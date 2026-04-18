//! Test Array Concatenation with + operator
//! Neo4j compatibility tests for array concatenation

use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use serde_json::Value;

#[test]
fn test_array_concat_simple() {
    let (mut executor, _ctx) = create_test_executor();

    // Simple array concatenation [1, 2] + [3, 4] should return [1, 2, 3, 4]
    let query = Query {
        cypher: "RETURN [1, 2] + [3, 4] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "result");

    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 4);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(1)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(2)));
            assert_eq!(arr[2], Value::Number(serde_json::Number::from(3)));
            assert_eq!(arr[3], Value::Number(serde_json::Number::from(4)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_concat_empty() {
    let (mut executor, _ctx) = create_test_executor();

    // Concatenate with empty arrays
    let query = Query {
        cypher: "RETURN [1, 2] + [] + [3] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);

    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 3);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(1)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(2)));
            assert_eq!(arr[2], Value::Number(serde_json::Number::from(3)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_concat_with_property() {
    let (mut executor, _ctx) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create node with array property
    let query = Query {
        cypher: "CREATE (n:Person {name: 'Alice'})".to_string(),
        params: std::collections::HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Concatenate arrays in RETURN
    let query = Query {
        cypher: "MATCH (n:Person) RETURN [1, 2] + [3, 4] AS all_nums".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);

    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 4);
            assert_eq!(arr[0], Value::Number(serde_json::Number::from(1)));
            assert_eq!(arr[1], Value::Number(serde_json::Number::from(2)));
            assert_eq!(arr[2], Value::Number(serde_json::Number::from(3)));
            assert_eq!(arr[3], Value::Number(serde_json::Number::from(4)));
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}

#[test]
fn test_array_concat_nested() {
    let (mut executor, _ctx) = create_test_executor();

    // Concatenate nested arrays (should concatenate at top level)
    let query = Query {
        cypher: "RETURN [[1, 2]] + [[3, 4]] AS result".to_string(),
        params: std::collections::HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);

    let row = &result.rows[0];
    match &row.values[0] {
        Value::Array(arr) => {
            assert_eq!(arr.len(), 2);
            // First element should be [1, 2]
            match &arr[0] {
                Value::Array(inner1) => {
                    assert_eq!(inner1.len(), 2);
                    assert_eq!(inner1[0], Value::Number(serde_json::Number::from(1)));
                    assert_eq!(inner1[1], Value::Number(serde_json::Number::from(2)));
                }
                _ => panic!("Expected nested array"),
            }
            // Second element should be [3, 4]
            match &arr[1] {
                Value::Array(inner2) => {
                    assert_eq!(inner2.len(), 2);
                    assert_eq!(inner2[0], Value::Number(serde_json::Number::from(3)));
                    assert_eq!(inner2[1], Value::Number(serde_json::Number::from(4)));
                }
                _ => panic!("Expected nested array"),
            }
        }
        other => panic!("Expected array, got: {:?}", other),
    }
}
