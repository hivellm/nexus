//! Tests for temporal arithmetic operations
//!
//! This module tests:
//! - datetime + duration
//! - datetime - duration
//! - datetime - datetime (returns duration)
//! - duration + duration
//! - duration - duration

use nexus_core::Engine;
use nexus_core::executor::ResultSet;
use nexus_core::testing::setup_test_engine;

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
// DATETIME + DURATION TESTS
// ============================================================================

#[test]
fn test_datetime_plus_duration_days() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add 1 day to a datetime
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:30:00') + duration({days: 1}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    // Date should change to 16th
    assert!(
        value_str.contains("2025-01-16"),
        "Expected date to be 2025-01-16, got: {}",
        value_str
    );
}

#[test]
fn test_datetime_plus_duration_months() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add 2 months to a datetime
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:30:00') + duration({months: 2}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2025-03"),
        "Expected month to be March (03), got: {}",
        value_str
    );
}

#[test]
fn test_datetime_plus_duration_years() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add 1 year to a datetime
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:30:00') + duration({years: 1}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2026"),
        "Expected year to be 2026, got: {}",
        value_str
    );
}

// ============================================================================
// DATETIME - DURATION TESTS
// ============================================================================

#[test]
fn test_datetime_minus_duration_days() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Subtract 5 days from a datetime
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:30:00') - duration({days: 5}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2025-01-10"),
        "Expected date to be 2025-01-10, got: {}",
        value_str
    );
}

#[test]
fn test_datetime_minus_duration_months() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Subtract 2 months from a datetime
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-03-15T10:30:00') - duration({months: 2}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2025-01"),
        "Expected month to be January (01), got: {}",
        value_str
    );
}

// ============================================================================
// DATETIME - DATETIME TESTS (DURATION BETWEEN)
// ============================================================================

#[test]
fn test_datetime_difference_days() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Get duration between two datetimes
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-20T10:30:00') - datetime('2025-01-15T10:30:00') AS result",
    );
    let value = get_single_value(&result);
    // Should return a duration object with days component
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );
    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(0), 5, "Expected 5 days difference");
    }
}

// ============================================================================
// DURATION + DURATION TESTS
// ============================================================================

#[test]
fn test_duration_plus_duration() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add two durations
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 3}) + duration({days: 2}) AS result",
    );
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );
    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(0), 5, "Expected 5 days total");
    }
}

#[test]
fn test_duration_plus_duration_mixed_units() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add durations with different units
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 1, hours: 2}) + duration({hours: 3, minutes: 30}) AS result",
    );
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );
    // Should have days: 1, hours: 5, minutes: 30
    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(-1), 1, "Expected 1 day");
    }
    if let Some(hours) = value.get("hours") {
        assert_eq!(hours.as_i64().unwrap_or(-1), 5, "Expected 5 hours");
    }
}

// ============================================================================
// DURATION - DURATION TESTS
// ============================================================================

#[test]
fn test_duration_minus_duration() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Subtract two durations
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 5}) - duration({days: 2}) AS result",
    );
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );
    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(-1), 3, "Expected 3 days");
    }
}

#[test]
fn test_duration_negative_result() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Subtract larger duration from smaller (should handle negative)
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 2}) - duration({days: 5}) AS result",
    );
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );
    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(0), -3, "Expected -3 days");
    }
}

// ============================================================================
// EDGE CASES AND INTEGRATION TESTS
// ============================================================================

#[test]
fn test_chained_temporal_operations() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Chain multiple operations: datetime + duration - duration
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:00:00') + duration({days: 10}) - duration({days: 3}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2025-01-22"),
        "Expected 2025-01-22 (15 + 10 - 3 = 22), got: {}",
        value_str
    );
}

#[test]
fn test_date_plus_duration() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Add duration to date (not datetime)
    let result = execute_query(
        &mut engine,
        "RETURN date('2025-01-15') + duration({days: 10}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");
    assert!(
        value_str.contains("2025-01-25"),
        "Expected 2025-01-25, got: {}",
        value_str
    );
}

#[test]
fn test_duration_creation() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Basic duration creation
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 5, hours: 3, minutes: 30}) AS result",
    );
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );

    if let Some(days) = value.get("days") {
        assert_eq!(days.as_i64().unwrap_or(-1), 5, "Expected 5 days");
    }
    if let Some(hours) = value.get("hours") {
        assert_eq!(hours.as_i64().unwrap_or(-1), 3, "Expected 3 hours");
    }
    if let Some(minutes) = value.get("minutes") {
        assert_eq!(minutes.as_i64().unwrap_or(-1), 30, "Expected 30 minutes");
    }
}

#[test]
fn test_duration_with_weeks() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Duration with weeks
    let result = execute_query(&mut engine, "RETURN duration({weeks: 2}) AS result");
    let value = get_single_value(&result);
    assert!(
        value.is_object(),
        "Expected duration object, got: {:?}",
        value
    );

    if let Some(weeks) = value.get("weeks") {
        assert_eq!(weeks.as_i64().unwrap_or(-1), 2, "Expected 2 weeks");
    }
}

#[test]
fn test_datetime_arithmetic_preserves_time() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Adding days should not change the time component
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-15T10:30:00') + duration({days: 5}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");

    // Date should change to 20th
    assert!(
        value_str.contains("2025-01-20"),
        "Expected date to be 2025-01-20, got: {}",
        value_str
    );
    // Original time was 10:30, should remain in some form (allowing for timezone conversion)
    assert!(
        value_str.contains(":30:") || value_str.contains(":30+"),
        "Expected time to still have :30 minutes, got: {}",
        value_str
    );
}

#[test]
fn test_datetime_year_crossover() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Adding months that cross year boundary
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-11-15T10:00:00') + duration({months: 3}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");

    // Should be February 2026
    assert!(
        value_str.contains("2026-02"),
        "Expected 2026-02 after adding 3 months to November, got: {}",
        value_str
    );
}

#[test]
fn test_datetime_month_crossover() {
    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Adding days that cross month boundary
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-30T10:00:00') + duration({days: 5}) AS result",
    );
    let value = get_single_value(&result);
    let value_str = value.as_str().unwrap_or("");

    // Should be February 4th
    assert!(
        value_str.contains("2025-02-04"),
        "Expected 2025-02-04 after adding 5 days to Jan 30, got: {}",
        value_str
    );
}
