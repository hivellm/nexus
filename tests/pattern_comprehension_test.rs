//! Integration tests for Pattern Comprehensions
//!
//! Tests for pattern comprehensions in Cypher queries

use nexus_core::{Engine, Error};
use serde_json::{json, Value};

fn create_engine() -> Result<Engine, Error> {
    Engine::new()
}

fn extract_first_row_value(result: Value) -> Value {
    result
        .get("rows")
        .and_then(|rows| rows.as_array())
        .and_then(|arr| arr.first())
        .and_then(|row| row.as_array())
        .and_then(|row| row.first())
        .cloned()
        .unwrap_or(Value::Null)
}

#[test]
fn test_pattern_comprehension_simple() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b)",
            None,
        )
        .unwrap();

    // Pattern comprehension: [(a)-[:KNOWS]->(b) | b.name]
    // This should collect all patterns matching (a)-[:KNOWS]->(b) and return b.name
    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b) RETURN [(a)-[:KNOWS]->(b) | b.name] AS names",
            None,
        )
        .unwrap();

    let names = extract_first_row_value(result);
    // Should return array with one element: ["Bob"]
    assert_eq!(names, json!(["Bob"]));
}

#[test]
fn test_pattern_comprehension_with_where() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25}), (a)-[:KNOWS]->(b)",
            None,
        )
        .unwrap();

    // Pattern comprehension with WHERE: [(a)-[:KNOWS]->(b) WHERE a.age > b.age | b.name]
    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b) RETURN [(a)-[:KNOWS]->(b) WHERE a.age > b.age | b.name] AS names",
            None,
        )
        .unwrap();

    let names = extract_first_row_value(result);
    // Alice (30) > Bob (25), so should return ["Bob"]
    assert_eq!(names, json!(["Bob"]));
}

#[test]
fn test_pattern_comprehension_no_transform() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b)",
            None,
        )
        .unwrap();

    // Pattern comprehension without transform: [(a)-[:KNOWS]->(b)]
    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b) RETURN [(a)-[:KNOWS]->(b)] AS patterns",
            None,
        )
        .unwrap();

    let patterns = extract_first_row_value(result);
    // Should return array with pattern variables
    assert!(patterns.is_array());
}

#[test]
fn test_pattern_comprehension_single_node() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30})", None)
        .unwrap();

    // Pattern comprehension with single node: [(n:Person) WHERE n.age > 25 | n.name]
    let result = engine
        .execute_cypher(
            "MATCH (n:Person {name: 'Alice'}) RETURN [(n:Person) WHERE n.age > 25 | n.name] AS names",
            None,
        )
        .unwrap();

    let names = extract_first_row_value(result);
    // Should return ["Alice"]
    assert_eq!(names, json!(["Alice"]));
}

#[test]
fn test_pattern_comprehension_where_false() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice', age: 30}), (b:Person {name: 'Bob', age: 25}), (a)-[:KNOWS]->(b)",
            None,
        )
        .unwrap();

    // Pattern comprehension with WHERE that evaluates to false
    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b) RETURN [(a)-[:KNOWS]->(b) WHERE a.age < b.age | b.name] AS names",
            None,
        )
        .unwrap();

    let names = extract_first_row_value(result);
    // Alice (30) < Bob (25) is false, so should return empty array
    assert_eq!(names, json!([]));
}

#[test]
fn test_pattern_comprehension_missing_variables() {
    let mut engine = create_engine().unwrap();

    // Pattern comprehension with variables that don't exist in context
    let result = engine
        .execute_cypher(
            "MATCH (a:Person {name: 'Alice'}) RETURN [(x)-[:KNOWS]->(y) | y.name] AS names",
            None,
        )
        .unwrap();

    let names = extract_first_row_value(result);
    // Variables x and y don't exist, so should return empty array
    assert_eq!(names, json!([]));
}

