//! Test String Concatenation operator (+)
//! Neo4j compatibility tests for string concatenation

use nexus_core::executor::Query;
use nexus_core::testing::create_test_executor;
use serde_json::Value;

#[test]
fn test_string_concat_simple() {
    let (mut executor, _ctx) = create_test_executor();

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
    let (mut executor, _ctx) = create_test_executor();

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
    let (mut executor, _ctx) = create_test_executor();

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
fn test_string_concat_in_create_return() {
    let (mut executor, _ctx) = create_test_executor();

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
fn test_string_concat_empty_strings() {
    let (mut executor, _ctx) = create_test_executor();

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
