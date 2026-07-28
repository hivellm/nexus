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
use nexus_core::testing::setup_isolated_test_engine;

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Get duration between two datetimes
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2025-01-20T10:30:00') - datetime('2025-01-15T10:30:00') AS result",
    );
    let value = get_single_value(&result);
    // A typed `duration` value canonicalizes to its ISO-8601 string at the
    // projection boundary — it no longer leaks the old `{days: 5}` raw
    // JSON object shape.
    assert_eq!(
        value.as_str(),
        Some("P5D"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

// ============================================================================
// DURATION + DURATION TESTS
// ============================================================================

#[test]
fn test_duration_plus_duration() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Add two durations
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 3}) + duration({days: 2}) AS result",
    );
    let value = get_single_value(&result);
    // Canonical ISO string, not the old raw `{days: 5}` object shape.
    assert_eq!(
        value.as_str(),
        Some("P5D"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

#[test]
fn test_duration_plus_duration_mixed_units() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Add durations with different units
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 1, hours: 2}) + duration({hours: 3, minutes: 30}) AS result",
    );
    let value = get_single_value(&result);
    // 1 day, 2h+3h=5h, 30m -> canonical "P1DT5H30M" (not the old raw object).
    assert_eq!(
        value.as_str(),
        Some("P1DT5H30M"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

// ============================================================================
// DURATION - DURATION TESTS
// ============================================================================

#[test]
fn test_duration_minus_duration() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Subtract two durations
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 5}) - duration({days: 2}) AS result",
    );
    let value = get_single_value(&result);
    // Canonical ISO string, not the old raw `{days: 3}` object shape.
    assert_eq!(
        value.as_str(),
        Some("P3D"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

#[test]
fn test_duration_negative_result() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Subtract larger duration from smaller (should handle negative)
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 2}) - duration({days: 5}) AS result",
    );
    let value = get_single_value(&result);
    // Canonical ISO string with a per-component sign, not the old raw
    // `{days: -3}` object shape.
    assert_eq!(
        value.as_str(),
        Some("P-3D"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

// ============================================================================
// EDGE CASES AND INTEGRATION TESTS
// ============================================================================

#[test]
fn test_chained_temporal_operations() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Basic duration creation
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 5, hours: 3, minutes: 30}) AS result",
    );
    let value = get_single_value(&result);
    // Canonical ISO string ("P5DT3H30M"), not the old raw
    // `{days: 5, hours: 3, minutes: 30}` object shape.
    assert_eq!(
        value.as_str(),
        Some("P5DT3H30M"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

#[test]
fn test_duration_with_weeks() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Duration with weeks — `weeks` folds into `days` (2 weeks = 14 days),
    // matching Neo4j's own internal Duration representation, which has no
    // separate "weeks" bucket.
    let result = execute_query(&mut engine, "RETURN duration({weeks: 2}) AS result");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("P14D"),
        "Expected canonical ISO duration string, got: {:?}",
        value
    );
}

#[test]
fn test_datetime_arithmetic_preserves_time() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

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

// ============================================================================
// phase0_fix-cypher-eval-panics — overflow must surface as a Cypher error,
// never panic and never silently wrap.
// ============================================================================

#[test]
fn test_date_plus_duration_days_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result =
        engine.execute_cypher("RETURN date('2020-01-01') + duration({days: 999999999}) AS result");
    assert!(
        result.is_err(),
        "date + duration with an out-of-range day count must error, not panic or wrap; got: {:?}",
        result
    );
}

#[test]
fn test_datetime_plus_duration_days_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher(
        "RETURN datetime('2020-01-01T00:00:00Z') + duration({days: 100000000}) AS result",
    );
    assert!(
        result.is_err(),
        "datetime + duration with an out-of-range day count must error, not panic or wrap; got: {:?}",
        result
    );
}

#[test]
fn test_date_minus_duration_days_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result =
        engine.execute_cypher("RETURN date('2020-01-01') - duration({days: 999999999}) AS result");
    assert!(
        result.is_err(),
        "date - duration with an out-of-range day count must error, not panic or wrap; got: {:?}",
        result
    );
}

#[test]
fn test_datetime_minus_duration_days_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher(
        "RETURN datetime('2020-01-01T00:00:00Z') - duration({days: 100000000}) AS result",
    );
    assert!(
        result.is_err(),
        "datetime - duration with an out-of-range day count must error, not panic or wrap; got: {:?}",
        result
    );
}

#[test]
fn test_naive_datetime_plus_duration_days_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // No trailing 'Z'/offset — covers the offset-less datetime literal form
    // (may resolve via either the RFC3339 or NaiveDateTime parse branch
    // depending on chrono's leniency; both must be overflow-safe).
    let result = engine.execute_cypher(
        "RETURN datetime('2020-01-01T00:00:00') + duration({days: 100000000}) AS result",
    );
    assert!(
        result.is_err(),
        "naive datetime + duration with an out-of-range day count must error, not panic or wrap; got: {:?}",
        result
    );
}

#[test]
fn test_date_plus_duration_large_but_in_range_succeeds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // ~2,739 years — comfortably inside chrono's representable range
    // (roughly +/-262,000 years) — this must still work after the fix.
    let result =
        engine.execute_cypher("RETURN date('2020-01-01') + duration({days: 1000000}) AS result");
    assert!(
        result.is_ok(),
        "a large but in-range day count must still succeed; got: {:?}",
        result
    );
}

#[test]
fn test_duration_plus_duration_years_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `duration({years: 9223372036854775807})` (i.e. `{years: i64::MAX}`)
    // itself now errors at construction (`years * 12` overflows i64 before
    // the `+` operator is ever reached) — that no longer exercises the
    // addition-overflow guard in `eval/temporal.rs`'s `try_duration_add`.
    // `{months: i64::MAX}` survives construction (`years * 12` isn't
    // involved), so the overflow below is guaranteed to come from the `+`
    // itself.
    let result = engine.execute_cypher(
        "RETURN duration({months: 9223372036854775807}) + duration({months: 1}) AS result",
    );
    assert!(
        result.is_err(),
        "duration + duration months overflow (i64::MAX + 1) must error in both debug and release; got: {:?}",
        result
    );
}

#[test]
fn test_duration_plus_duration_years_boundary_succeeds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A typed `duration` folds `years` into a single total-months field
    // (`years * 12 + months`, Neo4j's own internal Duration layout), so the
    // i64 boundary this test exercises is on *months*, not years directly.
    // 768614336404564650 years == 9223372036854775800 months
    // (i64::MAX / 12, truncated); adding 7 more months lands exactly on
    // i64::MAX with no overflow.
    let result = engine.execute_cypher(
        "RETURN duration({years: 768614336404564650}) + duration({months: 7}) AS result",
    );
    assert!(
        result.is_ok(),
        "duration + duration exactly at i64::MAX total months must succeed; got: {:?}",
        result
    );
}

#[test]
fn test_duration_minus_duration_years_underflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher(
        "RETURN duration({years: -9223372036854775808}) - duration({years: 1}) AS result",
    );
    assert!(
        result.is_err(),
        "duration - duration years underflow (i64::MIN - 1) must error in both debug and release; got: {:?}",
        result
    );
}

// ============================================================================
// hours/minutes/seconds must go through exact integer arithmetic, not a
// lossy f64 round-trip: `f64`'s 52-bit mantissa cannot represent every
// `i64` exactly, and casting an out-of-range `f64` back with `as i64`
// *saturates* silently instead of erroring — hiding a genuine overflow
// behind a plausible-looking wrong answer.
// ============================================================================

#[test]
fn test_duration_seconds_i64_max_round_trips_exactly() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A whole-seconds value of exactly `i64::MAX` is legitimately
    // representable (there is no overflow: `hours`/`minutes` default to 0,
    // so nothing is multiplied against it) — the integer fast path must
    // carry it through unchanged rather than losing precision by routing
    // it through `f64` (which cannot represent `i64::MAX` exactly and
    // would round it to `9223372036854775808.0`, one past the max).
    let result = engine
        .execute_cypher("RETURN duration({seconds: 9223372036854775807}) AS result")
        .expect("an exactly-representable whole-seconds value must not error");
    let value = &result.rows[0].values[0];
    assert_eq!(
        value.as_str(),
        Some("PT2562047788015215H30M7S"),
        "seconds must round-trip through i64 exactly, not lose precision via f64; got: {value:?}"
    );
}

#[test]
fn test_duration_hours_times_3600_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // The direct analog of the `years * 12` overflow guard: `hours *
    // 3600` overflows i64 for `hours: i64::MAX`, and the integer fast
    // path's `checked_mul` must surface that as an error rather than
    // silently saturating (the old `f64`-based combination's failure
    // mode).
    let result = engine.execute_cypher("RETURN duration({hours: 9223372036854775807}) AS result");
    assert!(
        result.is_err(),
        "duration hours * 3600 overflow must error, not silently saturate; got: {:?}",
        result
    );
}
