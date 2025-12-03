//! Tests for filter() function
//!
//! This file tests the filter() function which filters list elements based on a predicate.
//! Syntax: filter(variable IN list WHERE predicate)

use nexus_core::testing::setup_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn get_single_value(result: &ResultSet) -> &serde_json::Value {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    assert!(
        !result.rows[0].values.is_empty(),
        "First row has no values!"
    );
    &result.rows[0].values[0]
}

// ============================================================================
// BASIC FILTER TESTS
// ============================================================================

#[test]
fn test_filter_basic() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3, 4, 5] WHERE x > 2) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64().unwrap(), 3);
    assert_eq!(arr[1].as_i64().unwrap(), 4);
    assert_eq!(arr[2].as_i64().unwrap(), 5);
}

#[test]
fn test_filter_less_than() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3, 4, 5] WHERE x < 3) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_i64().unwrap(), 1);
    assert_eq!(arr[1].as_i64().unwrap(), 2);
}

#[test]
fn test_filter_equals() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3, 2, 1] WHERE x = 2) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_i64().unwrap(), 2);
    assert_eq!(arr[1].as_i64().unwrap(), 2);
}

#[test]
fn test_filter_modulo() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Filter even numbers
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3, 4, 5, 6] WHERE x % 2 = 0) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64().unwrap(), 2);
    assert_eq!(arr[1].as_i64().unwrap(), 4);
    assert_eq!(arr[2].as_i64().unwrap(), 6);
}

// ============================================================================
// FILTER WITH STRINGS
// ============================================================================

#[test]
fn test_filter_strings() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(s IN ['Alice', 'Bob', 'Charlie', 'David'] WHERE size(s) > 4) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    // Alice (5), Charlie (7), David (5) all have size > 4
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_str().unwrap(), "Alice");
    assert_eq!(arr[1].as_str().unwrap(), "Charlie");
    assert_eq!(arr[2].as_str().unwrap(), "David");
}

// ============================================================================
// EDGE CASES
// ============================================================================

#[test]
fn test_filter_empty_list() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN filter(x IN [] WHERE x > 2) AS result");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 0);
}

#[test]
fn test_filter_none_match() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3] WHERE x > 10) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 0);
}

#[test]
fn test_filter_all_match() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3] WHERE x > 0) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64().unwrap(), 1);
    assert_eq!(arr[1].as_i64().unwrap(), 2);
    assert_eq!(arr[2].as_i64().unwrap(), 3);
}

// ============================================================================
// COMPLEX PREDICATES
// ============================================================================

#[test]
fn test_filter_complex_predicate() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Filter numbers that are > 2 AND < 8
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 3, 5, 7, 9] WHERE x > 2 AND x < 8) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0].as_i64().unwrap(), 3);
    assert_eq!(arr[1].as_i64().unwrap(), 5);
    assert_eq!(arr[2].as_i64().unwrap(), 7);
}

#[test]
fn test_filter_or_predicate() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Filter numbers that are < 2 OR > 4
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN [1, 2, 3, 4, 5] WHERE x < 2 OR x > 4) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_i64().unwrap(), 1);
    assert_eq!(arr[1].as_i64().unwrap(), 5);
}

// ============================================================================
// PRACTICAL USE CASES
// ============================================================================

#[test]
fn test_filter_with_range() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Filter even numbers from range
    let result = execute_query(
        &mut engine,
        "RETURN filter(x IN range(1, 10) WHERE x % 2 = 0) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0].as_i64().unwrap(), 2);
    assert_eq!(arr[1].as_i64().unwrap(), 4);
    assert_eq!(arr[2].as_i64().unwrap(), 6);
    assert_eq!(arr[3].as_i64().unwrap(), 8);
    assert_eq!(arr[4].as_i64().unwrap(), 10);
}

#[test]
fn test_filter_combined_with_size() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // Filter list and get size
    let result = execute_query(
        &mut engine,
        "RETURN size(filter(x IN [1, 2, 3, 4, 5] WHERE x > 2)) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 3);
}

// ============================================================================
// NESTED FILTER
// ============================================================================

#[test]
fn test_nested_filter() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    // First filter > 2, then filter even numbers
    let result = execute_query(
        &mut engine,
        "RETURN filter(y IN filter(x IN [1, 2, 3, 4, 5, 6] WHERE x > 2) WHERE y % 2 = 0) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0].as_i64().unwrap(), 4);
    assert_eq!(arr[1].as_i64().unwrap(), 6);
}
