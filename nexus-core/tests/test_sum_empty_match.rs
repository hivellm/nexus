use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use serde_json::Value;
use std::collections::HashMap;

#[test]
fn test_sum_with_empty_match() {
    let (mut executor, _ctx) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test sum() with empty MATCH - should return NULL like Neo4j
    let query = Query {
        cypher: "MATCH (n:NonExistent) RETURN sum(n.value) AS total".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "total");
    assert_eq!(
        result.rows[0].values[0],
        Value::Null,
        "sum() on empty MATCH should return NULL"
    );
}

#[test]
fn test_sum_with_literal() {
    let (mut executor, _ctx) = create_test_executor();

    // Test sum() with literal - should work
    let query = Query {
        cypher: "RETURN sum(5) AS total".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "total");
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(5))
    );
}

#[test]
fn test_sum_with_actual_values() {
    let (mut executor, _ctx) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Create test data
    let query = Query {
        cypher: "CREATE (n:Test {value: 10}), (m:Test {value: 20}), (o:Test {value: 30})"
            .to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test sum() with actual values
    let query = Query {
        cypher: "MATCH (n:Test) RETURN sum(n.value) AS total".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "total");
    assert_eq!(
        result.rows[0].values[0],
        Value::Number(serde_json::Number::from(60))
    );
}

#[test]
fn test_avg_with_empty_match() {
    let (mut executor, _ctx) = create_test_executor();

    // Clean database
    let query = Query {
        cypher: "MATCH (n) DETACH DELETE n".to_string(),
        params: HashMap::new(),
    };
    executor.execute(&query).unwrap();

    // Test avg() with empty MATCH - should also return NULL
    let query = Query {
        cypher: "MATCH (n:NonExistent) RETURN avg(n.value) AS average".to_string(),
        params: HashMap::new(),
    };
    let result = executor.execute(&query).unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.columns[0], "average");
    assert_eq!(
        result.rows[0].values[0],
        Value::Null,
        "avg() on empty MATCH should return NULL"
    );
}
