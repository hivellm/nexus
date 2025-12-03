//! Tests for new Neo4j compatibility functions
//!
//! This file tests the newly implemented functions for Neo4j compatibility:
//! - Temporal component extraction functions
//! - Advanced string functions
//! - List functions
//! - Mathematical functions

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
// TEMPORAL COMPONENT EXTRACTION FUNCTIONS
// ============================================================================

#[test]
fn test_temporal_year_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN year(date('2025-03-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 2025);
}

#[test]
fn test_temporal_month_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN month(date('2025-03-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 3);
}

#[test]
fn test_temporal_day_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN day(date('2025-03-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 15);
}

#[test]
fn test_temporal_quarter_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Q1: January-March
    let result = execute_query(&mut engine, "RETURN quarter(date('2025-03-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 1);

    // Q2: April-June
    let result = execute_query(&mut engine, "RETURN quarter(date('2025-05-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 2);

    // Q3: July-September
    let result = execute_query(&mut engine, "RETURN quarter(date('2025-08-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 3);

    // Q4: October-December
    let result = execute_query(&mut engine, "RETURN quarter(date('2025-11-15')) AS result");
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 4);
}

#[test]
fn test_temporal_hour_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN hour(datetime('2025-03-15T14:30:45Z')) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 14);
}

#[test]
fn test_temporal_minute_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN minute(datetime('2025-03-15T14:30:45Z')) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 30);
}

#[test]
fn test_temporal_second_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN second(datetime('2025-03-15T14:30:45Z')) AS result",
    );
    assert_eq!(get_single_value(&result).as_i64().unwrap(), 45);
}

#[test]
fn test_temporal_week_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN week(date('2025-03-15')) AS result");
    let week = get_single_value(&result).as_i64().unwrap();
    // Week 11 of 2025 (should be around week 11)
    assert!(week > 0 && week <= 53);
}

#[test]
fn test_temporal_dayofweek_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN dayOfWeek(date('2025-03-15')) AS result",
    );
    let dow = get_single_value(&result).as_i64().unwrap();
    // Should return 1-7 (Monday to Sunday)
    assert!(dow >= 1 && dow <= 7);
}

#[test]
fn test_temporal_dayofyear_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN dayOfYear(date('2025-03-15')) AS result",
    );
    let doy = get_single_value(&result).as_i64().unwrap();
    // March 15 is day 74 of the year (31 days in Jan + 28 days in Feb + 15 days)
    assert_eq!(doy, 74);
}

// ============================================================================
// ADVANCED STRING FUNCTIONS
// ============================================================================

#[test]
fn test_string_left_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN left('Hello World', 5) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "Hello");

    let result = execute_query(&mut engine, "RETURN left('Test', 2) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "Te");
}

#[test]
fn test_string_right_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN right('Hello World', 5) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "World");

    let result = execute_query(&mut engine, "RETURN right('Test', 2) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "st");
}

#[test]
fn test_string_left_longer_than_string() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN left('Hi', 10) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "Hi");
}

#[test]
fn test_string_right_longer_than_string() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN right('Hi', 10) AS result");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "Hi");
}

// ============================================================================
// LIST FUNCTIONS
// ============================================================================

#[test]
fn test_list_flatten_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(
        &mut engine,
        "RETURN flatten([[1, 2], [3, 4], [5]]) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0].as_i64().unwrap(), 1);
    assert_eq!(arr[1].as_i64().unwrap(), 2);
    assert_eq!(arr[2].as_i64().unwrap(), 3);
    assert_eq!(arr[3].as_i64().unwrap(), 4);
    assert_eq!(arr[4].as_i64().unwrap(), 5);
}

#[test]
fn test_list_flatten_mixed() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN flatten([[1, 2], 3, [4, 5]]) AS result");
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0].as_i64().unwrap(), 1);
    assert_eq!(arr[1].as_i64().unwrap(), 2);
    assert_eq!(arr[2].as_i64().unwrap(), 3);
    assert_eq!(arr[3].as_i64().unwrap(), 4);
    assert_eq!(arr[4].as_i64().unwrap(), 5);
}

#[test]
fn test_list_zip_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(
        &mut engine,
        "RETURN zip([1, 2, 3], ['a', 'b', 'c']) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    assert_eq!(arr.len(), 3);

    // Check first tuple
    let tuple0 = arr[0].as_array().unwrap();
    assert_eq!(tuple0[0].as_i64().unwrap(), 1);
    assert_eq!(tuple0[1].as_str().unwrap(), "a");

    // Check second tuple
    let tuple1 = arr[1].as_array().unwrap();
    assert_eq!(tuple1[0].as_i64().unwrap(), 2);
    assert_eq!(tuple1[1].as_str().unwrap(), "b");

    // Check third tuple
    let tuple2 = arr[2].as_array().unwrap();
    assert_eq!(tuple2[0].as_i64().unwrap(), 3);
    assert_eq!(tuple2[1].as_str().unwrap(), "c");
}

#[test]
fn test_list_zip_different_lengths() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(
        &mut engine,
        "RETURN zip([1, 2, 3, 4], ['a', 'b']) AS result",
    );
    let arr = get_single_value(&result).as_array().unwrap();
    // Should only zip to the length of the shortest list
    assert_eq!(arr.len(), 2);
}

// ============================================================================
// MATHEMATICAL FUNCTIONS
// ============================================================================

#[test]
fn test_math_pi_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN pi() AS result");
    let pi = get_single_value(&result).as_f64().unwrap();
    assert!((pi - std::f64::consts::PI).abs() < 0.0001);
}

#[test]
fn test_math_e_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN e() AS result");
    let e = get_single_value(&result).as_f64().unwrap();
    assert!((e - std::f64::consts::E).abs() < 0.0001);
}

#[test]
fn test_math_radians_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN radians(180) AS result");
    let rad = get_single_value(&result).as_f64().unwrap();
    assert!((rad - std::f64::consts::PI).abs() < 0.0001);
}

#[test]
fn test_math_degrees_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN degrees(3.14159265359) AS result");
    let deg = get_single_value(&result).as_f64().unwrap();
    assert!((deg - 180.0).abs() < 0.01);
}

#[test]
fn test_math_log10_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN log10(100) AS result");
    let log = get_single_value(&result).as_f64().unwrap();
    assert!((log - 2.0).abs() < 0.0001);
}

#[test]
fn test_math_log_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN log(2.71828182846) AS result");
    let log = get_single_value(&result).as_f64().unwrap();
    assert!((log - 1.0).abs() < 0.0001);
}

#[test]
fn test_math_exp_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN exp(1) AS result");
    let exp_val = get_single_value(&result).as_f64().unwrap();
    assert!((exp_val - std::f64::consts::E).abs() < 0.0001);
}

#[test]
fn test_math_asin_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN asin(0.5) AS result");
    let asin = get_single_value(&result).as_f64().unwrap();
    assert!((asin - 0.5236).abs() < 0.001); // approximately pi/6
}

#[test]
fn test_math_acos_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN acos(0.5) AS result");
    let acos = get_single_value(&result).as_f64().unwrap();
    assert!((acos - 1.0472).abs() < 0.001); // approximately pi/3
}

#[test]
fn test_math_atan_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN atan(1) AS result");
    let atan = get_single_value(&result).as_f64().unwrap();
    assert!((atan - std::f64::consts::FRAC_PI_4).abs() < 0.0001); // pi/4
}

#[test]
fn test_math_atan2_function() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN atan2(1, 1) AS result");
    let atan2 = get_single_value(&result).as_f64().unwrap();
    assert!((atan2 - std::f64::consts::FRAC_PI_4).abs() < 0.0001); // pi/4
}

// ============================================================================
// NULL HANDLING TESTS
// ============================================================================

#[test]
fn test_temporal_functions_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN year(null) AS result");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_string_functions_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN left(null, 5) AS result");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_math_functions_with_null() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN asin(null) AS result");
    assert!(get_single_value(&result).is_null());
}
