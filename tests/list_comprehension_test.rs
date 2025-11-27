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
fn test_list_comprehension_simple_transform() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (n:Person {name: 'Alice', age: 30}), (m:Person {name: 'Bob', age: 25})",
            None,
        )
        .unwrap();

    // Simple transformation: [x IN [1, 2, 3] | x * 2]
    let result = engine
        .execute_cypher("RETURN [x IN [1, 2, 3] | x * 2] AS doubled", None)
        .unwrap();

    let doubled = extract_first_row_value(result);
    assert_eq!(doubled, json!([2, 4, 6]));
}

#[test]
fn test_list_comprehension_with_where() {
    let mut engine = create_engine().unwrap();

    // Filter: [x IN [1, 2, 3, 4, 5] WHERE x > 2]
    let result = engine
        .execute_cypher("RETURN [x IN [1, 2, 3, 4, 5] WHERE x > 2] AS filtered", None)
        .unwrap();

    let filtered = extract_first_row_value(result);
    assert_eq!(filtered, json!([3, 4, 5]));
}

#[test]
fn test_list_comprehension_filter_and_transform() {
    let mut engine = create_engine().unwrap();

    // Filter and transform: [x IN [1, 2, 3, 4, 5] WHERE x % 2 = 0 | x * x]
    let result = engine
        .execute_cypher(
            "RETURN [x IN [1, 2, 3, 4, 5] WHERE x % 2 = 0 | x * x] AS squares",
            None,
        )
        .unwrap();

    let squares = extract_first_row_value(result);
    assert_eq!(squares, json!([4, 16]));
}

#[test]
fn test_list_comprehension_from_node_properties() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (n:Person {name: 'Alice', scores: [85, 90, 78, 92]})",
            None,
        )
        .unwrap();

    // Transform scores: [s IN n.scores WHERE s >= 85 | s + 5]
    let result = engine
        .execute_cypher(
            "MATCH (n:Person {name: 'Alice'}) RETURN [s IN n.scores WHERE s >= 85 | s + 5] AS adjusted",
            None,
        )
        .unwrap();

    let adjusted = extract_first_row_value(result);
    assert_eq!(adjusted, json!([90, 95, 97]));
}

#[test]
fn test_list_comprehension_empty_list() {
    let mut engine = create_engine().unwrap();

    // Empty list should return empty list
    let result = engine
        .execute_cypher("RETURN [x IN [] | x * 2] AS empty", None)
        .unwrap();

    let empty = extract_first_row_value(result);
    assert_eq!(empty, json!([]));
}

#[test]
fn test_list_comprehension_no_transform() {
    let mut engine = create_engine().unwrap();

    // Filter only, no transformation: [x IN [1, 2, 3, 4, 5] WHERE x > 3]
    let result = engine
        .execute_cypher("RETURN [x IN [1, 2, 3, 4, 5] WHERE x > 3] AS filtered", None)
        .unwrap();

    let filtered = extract_first_row_value(result);
    assert_eq!(filtered, json!([4, 5]));
}

#[test]
fn test_list_comprehension_string_operations() {
    let mut engine = create_engine().unwrap();

    // Transform strings: [s IN ['hello', 'world', 'test'] | UPPER(s)]
    let result = engine
        .execute_cypher(
            "RETURN [s IN ['hello', 'world', 'test'] | UPPER(s)] AS uppercased",
            None,
        )
        .unwrap();

    let uppercased = extract_first_row_value(result);
    assert_eq!(uppercased, json!(["HELLO", "WORLD", "TEST"]));
}

#[test]
fn test_list_comprehension_nested() {
    let mut engine = create_engine().unwrap();

    // Nested list comprehension: [x IN [1, 2, 3] | [y IN [1, 2] | x * y]]
    let result = engine
        .execute_cypher(
            "RETURN [x IN [1, 2, 3] | [y IN [1, 2] | x * y]] AS nested",
            None,
        )
        .unwrap();

    let nested = extract_first_row_value(result);
    assert_eq!(nested, json!([[1, 2], [2, 4], [3, 6]]));
}

#[test]
fn test_list_comprehension_with_variables() {
    let mut engine = create_engine().unwrap();

    // Create test data
    engine
        .execute_cypher(
            "CREATE (n:Person {name: 'Alice', age: 30}), (m:Person {name: 'Bob', age: 25})",
            None,
        )
        .unwrap();

    // Use variable from MATCH in list comprehension
    let result = engine
        .execute_cypher(
            "MATCH (n:Person) WITH n.age AS age RETURN [x IN [1, 2, 3] WHERE x < age | x * 10] AS result",
            None,
        )
        .unwrap();

    let rows = result
        .get("rows")
        .and_then(|r| r.as_array())
        .unwrap();
    
    // Should have 2 rows (one for each person)
    assert_eq!(rows.len(), 2);
    
    // First row: Alice (age 30) -> [10, 20]
    let first_result = rows[0]
        .as_array()
        .and_then(|r| r.first())
        .unwrap();
    assert_eq!(first_result, &json!([10, 20]));
    
    // Second row: Bob (age 25) -> [10, 20]
    let second_result = rows[1]
        .as_array()
        .and_then(|r| r.first())
        .unwrap();
    assert_eq!(second_result, &json!([10, 20]));
}

#[test]
fn test_list_comprehension_all_items_pass_filter() {
    let mut engine = create_engine().unwrap();

    // All items pass filter
    let result = engine
        .execute_cypher("RETURN [x IN [1, 2, 3] WHERE x > 0 | x * 2] AS all", None)
        .unwrap();

    let all = extract_first_row_value(result);
    assert_eq!(all, json!([2, 4, 6]));
}

#[test]
fn test_list_comprehension_no_items_pass_filter() {
    let mut engine = create_engine().unwrap();

    // No items pass filter
    let result = engine
        .execute_cypher("RETURN [x IN [1, 2, 3] WHERE x > 10 | x * 2] AS none", None)
        .unwrap();

    let none = extract_first_row_value(result);
    assert_eq!(none, json!([]));
}

