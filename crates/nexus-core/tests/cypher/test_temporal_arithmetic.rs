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
    // Original time was 10:30, should remain in some form (allowing for
    // timezone conversion). Seconds are omitted from the canonical
    // rendering when zero (see `temporal_value::render_time_of_day`), so
    // ":30" may be followed directly by an offset (`Z`, `+`, or `-`)
    // instead of `:00` — checked on any CI runner's timezone, positive or
    // negative offset alike.
    assert!(
        value_str.contains(":30:")
            || value_str.contains(":30+")
            || value_str.contains(":30-")
            || value_str.contains(":30Z"),
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

#[test]
fn test_duration_years_construction_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `years * 12` overflows i64 for `years: i64::MAX` — the exact-integer
    // fast path's `checked_mul` must reject this at construction, before
    // any `+`/`-` is ever reached. This is the direct standalone repro for
    // the overflow guard `test_duration_plus_duration_years_overflow_errors`'s
    // doc comment describes as a side effect.
    let result = engine.execute_cypher("RETURN duration({years: 9223372036854775807}) AS result");
    assert!(
        result.is_err(),
        "duration({{years: i64::MAX}}) must error (years * 12 overflows i64), not silently \
         saturate; got: {:?}",
        result
    );
}

// ============================================================================
// Fractional MAP components — `duration({years: 12.5, ...})` must carry a
// fractional year/month/week/day down to the next-smaller unit exactly like
// the ISO string parser (`temporal_parse::parse_iso_duration`), reusing its
// shared `carry_duration_components` helper rather than defaulting the
// component to `0` (the pre-fix behavior: `map.get("years").and_then
// (Value::as_i64)` returns `None` for a float-backed JSON number).
// ============================================================================

#[test]
fn test_duration_fractional_month_map_matches_iso_string_carry() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Both sides must produce byte-for-byte the same canonical string —
    // proof the map constructor's fractional branch and the ISO string
    // parser now share the exact same carry math
    // (`temporal_parse::carry_duration_components`), not two independent
    // approximations. `"P0.75M"` -> `"P22DT19H51M49.5S"` is the exact
    // fixture `temporal_parse`'s own
    // `duration_fractional_month_carries_through_days_and_time` unit test
    // already verifies for the string path.
    let map_result = execute_query(&mut engine, "RETURN duration({months: 0.75}) AS result");
    let string_result = execute_query(&mut engine, "RETURN duration('P0.75M') AS result");
    assert_eq!(
        get_single_value(&map_result).as_str(),
        Some("P22DT19H51M49.5S"),
    );
    assert_eq!(
        get_single_value(&map_result).as_str(),
        get_single_value(&string_result).as_str(),
        "duration({{months: 0.75}}) and duration('P0.75M') must render identically"
    );
}

#[test]
fn test_duration_fractional_years_with_no_remainder_converts_exactly() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // 0.5 years is exactly 6 months (years always fold into months via an
    // exact ×12, never the lossy average-month-in-seconds constant) — no
    // remainder should carry any further down into days/seconds.
    let result = execute_query(&mut engine, "RETURN duration({years: 0.5}) AS result");
    assert_eq!(get_single_value(&result).as_str(), Some("P6M"));
}

#[test]
fn test_duration_fractional_map_matches_temporal8_scenario1_row3() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Ground truth: openCypher TCK `Temporal8.feature` Scenario Outline
    // [1]'s third example row —
    // `{years: 12.5, months: 5.5, days: 14.5, hours: 16.5, minutes: 12.5,
    // seconds: 70.5, nanoseconds: 3}`. Hand-derived expected components via
    // the same carry chain `carry_duration_components` implements:
    //   months_total = 12.5*12 + 5.5 = 155.5 -> months=155, frac=0.5
    //   days_equiv_s = 14.5*86400 + 0.5*2_629_746 = 2_567_673
    //     -> days=29, remainder=62_073s
    //   total_s = 62_073 + 16.5*3600 + 12.5*60 + 70.5 = 122_293.5
    //     -> seconds=122_293, nanos=500_000_000 (+3 nanoseconds key) =
    //        500_000_003
    // Rendered: months=155 -> 12Y11M, days=29D, seconds=122_293 ->
    // 33H58M13S, plus the 500_000_003 nanosecond fraction.
    // Cross-validated independently against Scenario [6]'s row-3 fixture
    // (same map, duration + duration with an identical known-integer
    // duration) which nets out to the TCK's literal expected string
    // `'P25Y4M43DT50H11M23.500000004S'`.
    let result = execute_query(
        &mut engine,
        "RETURN duration({years: 12.5, months: 5.5, days: 14.5, hours: 16.5, minutes: 12.5, \
         seconds: 70.5, nanoseconds: 3}) AS result",
    );
    assert_eq!(
        get_single_value(&result).as_str(),
        Some("P12Y11M29DT33H58M13.500000003S"),
    );
}

#[test]
fn test_date_plus_and_minus_fractional_duration_matches_temporal8_scenario1_row3() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // openCypher TCK `Temporal8.feature` Scenario Outline [1], row 3 —
    // `date({year: 1984, month: 10, day: 11}) + d.dur` / `- d.dur` where
    // `d.dur = duration({years: 12.5, months: 5.5, days: 14.5, hours: 16.5,
    // minutes: 12.5, seconds: 70.5, nanoseconds: 3})` (normalized
    // months=155, days=29, seconds=122_293, i.e. 1 day 9h58m13.5s beyond
    // `days`). A `Date` target has no time-of-day to absorb the seconds
    // against, but the whole-day overflow they contain (122_293 / 86400 = 1
    // via plain truncating-toward-zero division, remainder discarded)
    // still shifts the date by one extra day, folded in on the *unsigned*
    // side before `+`/`-`'s sign is applied:
    //   sum:  1984-10 + 155mo (clamped) -> 1997-09-11, then + (29+1)d
    //         -> 1997-10-11
    //   diff: 1984-10 - 155mo (clamped) -> 1971-11-11, then - (29+1)d
    //         -> 1971-10-12
    // Both directions only agree with a *pre-sign* fold (a post-sign fold
    // would subtract, not add, the extra day in the `diff` direction,
    // landing on 1971-10-14 instead).
    let result = execute_query(
        &mut engine,
        "WITH date({year: 1984, month: 10, day: 11}) AS x \
         RETURN x + duration({years: 12.5, months: 5.5, days: 14.5, hours: 16.5, minutes: 12.5, \
         seconds: 70.5, nanoseconds: 3}) AS sum, \
         x - duration({years: 12.5, months: 5.5, days: 14.5, hours: 16.5, minutes: 12.5, \
         seconds: 70.5, nanoseconds: 3}) AS diff",
    );
    let row = &result.rows[0].values;
    assert_eq!(row[0].as_str(), Some("1997-10-11"), "sum");
    assert_eq!(row[1].as_str(), Some("1971-10-12"), "diff");
}

// ============================================================================
// Date + duration with a NEGATIVE sub-day seconds/hours component must
// truncate toward zero, not floor (`div_euclid`) — a floor would invent a
// phantom day shift for a duration whose only non-zero field is a small
// negative `seconds`/`hours`, which Neo4j does not produce (Neo4j's own
// duration arithmetic truncates toward zero throughout).
// ============================================================================

#[test]
fn test_date_plus_duration_negative_seconds_alone_does_not_shift_the_date() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `dayless_seconds = -1`; a floor (`div_euclid`) would give
    // `extra_days = -1` (a full phantom day shift); truncation gives `0`
    // (the sub-day remainder is simply discarded) — the date must be
    // unchanged.
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 1984, month: 10, day: 11}) + duration({seconds: -1}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("1984-10-11"));
}

#[test]
fn test_date_plus_duration_one_day_and_negative_second_lands_exactly_one_day_later() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `days: 1` (unaffected) plus `dayless_seconds: -1` — truncation gives
    // `extra_days = 0`, so the date shifts by exactly the explicit `days: 1`,
    // not one day short (which a floor's `-1`-then-added `extra_days` would
    // otherwise produce: `1 + (-1) = 0`, landing back on the 11th).
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 1984, month: 10, day: 11}) + duration({days: 1, seconds: -1}) \
         AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("1984-10-12"));
}

#[test]
fn test_date_plus_duration_negative_hours_beyond_a_day_truncates_toward_zero() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `dayless_seconds = -25 * 3600 = -90_000`; truncation gives
    // `extra_days = -90_000 / 86400 = -1` (Rust's default `/` truncates
    // toward zero: `-1.041...` truncates to `-1`, not `-2`, so exactly one
    // day is subtracted, not two) — floor (`div_euclid`) would agree here
    // (`-90_000.div_euclid(86400) == -1` too), so this alone doesn't
    // distinguish the two; it's included as the direct repro from the
    // review that first exposed the negative-seconds case.
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 1984, month: 10, day: 11}) + duration({hours: -25}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("1984-10-10"));
}

#[test]
fn test_date_minus_duration_negative_seconds_alone_does_not_shift_the_date() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // The SUBTRACTION-direction sibling of
    // `test_date_plus_duration_negative_seconds_alone_does_not_shift_the_date`
    // — the direction the old `div_euclid` bug hit hardest, since `sign`
    // flips the `extra_days` fold too. `dayless_seconds = -1`; truncation
    // gives `extra_days = 0`, so subtracting a duration whose only
    // non-zero field is `seconds: -1` must leave the date unchanged (a
    // floor would have folded a phantom `extra_days = -1` in, then negated
    // by `sign = -1` into a *forward* one-day shift).
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 1984, month: 10, day: 11}) - duration({seconds: -1}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("1984-10-11"));
}

#[test]
fn test_date_minus_duration_negative_hours_beyond_a_day_shifts_forward_one_day() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // The SUBTRACTION-direction sibling of
    // `test_date_plus_duration_negative_hours_beyond_a_day_truncates_toward_zero`.
    // `dayless_seconds = -25 * 3600 = -90_000`; truncation gives
    // `extra_days = -1` (folded into `days` pre-sign, per the `+`
    // direction's own derivation), then `sign = -1` negates the whole
    // `total_days` — subtracting a *negative* 25-hour duration is
    // equivalent to adding it, and the whole-day part of that addition
    // (1 day, with the extra hour's remainder discarded) shifts the date
    // forward.
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 1984, month: 10, day: 11}) - duration({hours: -25}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("1984-10-12"));
}

#[test]
fn test_duration_fractional_map_plus_known_duration_matches_temporal8_scenario6_row3() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // openCypher TCK `Temporal8.feature` Scenario Outline [6], row 3 (`sum`
    // column): adding the fractional map above to a known-good all-integer
    // duration must land exactly on the TCK's literal expected string —
    // the strongest end-to-end proof the fractional carry path produces
    // the right normalized `(months, days, seconds, nanos)` tuple, not
    // just a plausible-looking rendering in isolation.
    let result = engine
        .execute_cypher(
            "RETURN duration({years: 12, months: 5, days: 14, hours: 16, minutes: 12, \
             seconds: 70, nanoseconds: 1}) + duration({years: 12.5, months: 5.5, days: 14.5, \
             hours: 16.5, minutes: 12.5, seconds: 70.5, nanoseconds: 3}) AS sum",
        )
        .expect("Query should succeed");
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("P25Y4M43DT50H11M23.500000004S"),
    );
}

#[test]
fn test_duration_fractional_years_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // The fractional carry path's own overflow guard (`to_i64` inside
    // `carry_duration_components`, mirroring `parse_iso_duration`'s
    // wildly-out-of-range check): a magnitude far beyond `i64`'s range
    // must error, not silently saturate.
    let result =
        engine.execute_cypher("RETURN duration({years: 99999999999999999999.5}) AS result");
    assert!(
        result.is_err(),
        "duration with a wildly out-of-range fractional years component must error, not \
         silently saturate; got: {:?}",
        result
    );
}

// ============================================================================
// `milliseconds`/`microseconds` map keys — fold into the nanosecond
// remainder like `nanoseconds` does. Ground truth: openCypher TCK
// `Temporal1.feature` Scenario Outline [12] and `Temporal6.feature`
// Scenario Outline [6].
// ============================================================================

#[test]
fn test_duration_milliseconds_key_matches_temporal1_scenario12() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `Temporal1.feature` [12]: `{days: 14, seconds: 70, milliseconds: 1}`
    // -> `'P14DT1M10.001S'`.
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 14, seconds: 70, milliseconds: 1}) AS result",
    );
    assert_eq!(get_single_value(&result).as_str(), Some("P14DT1M10.001S"));
}

#[test]
fn test_duration_microseconds_key_matches_temporal1_scenario12() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `Temporal1.feature` [12]: `{days: 14, seconds: 70, microseconds: 1}`
    // -> `'P14DT1M10.000001S'`.
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 14, seconds: 70, microseconds: 1}) AS result",
    );
    assert_eq!(
        get_single_value(&result).as_str(),
        Some("P14DT1M10.000001S")
    );
}

#[test]
fn test_duration_milliseconds_key_negative_rows_match_temporal6_scenario6() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `Temporal6.feature` [6], the three rows the review cited by line
    // number: positive seconds with a negative millisecond borrow, and
    // both-negative combinations. Each is independently derived by hand:
    //   {seconds: 2, milliseconds: -1}:
    //     hms_seconds=2, extra_nanos=-1_000_000 -> carry_seconds=-1,
    //     remainder_nanos=999_000_000 -> whole_seconds=1 -> 'PT1.999S'
    //   {seconds: -2, milliseconds: 1}:
    //     hms_seconds=-2, extra_nanos=1_000_000 -> carry_seconds=0,
    //     remainder_nanos=1_000_000 -> whole_seconds=-2, normalized to
    //     (-1, -999_000_000) by `make_duration` -> 'PT-1.999S'
    //   {seconds: -2, milliseconds: -1}:
    //     hms_seconds=-2, extra_nanos=-1_000_000 -> carry_seconds=-1,
    //     remainder_nanos=999_000_000 -> whole_seconds=-3, normalized to
    //     (-2, -1_000_000) -> 'PT-2.001S'
    let result = execute_query(
        &mut engine,
        "RETURN duration({seconds: 2, milliseconds: -1}) AS a, \
         duration({seconds: -2, milliseconds: 1}) AS b, \
         duration({seconds: -2, milliseconds: -1}) AS c",
    );
    let row = &result.rows[0].values;
    assert_eq!(row[0].as_str(), Some("PT1.999S"), "a");
    assert_eq!(row[1].as_str(), Some("PT-1.999S"), "b");
    assert_eq!(row[2].as_str(), Some("PT-2.001S"), "c");
}

#[test]
fn test_duration_milliseconds_and_microseconds_overflow_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `milliseconds * 1_000_000` overflows i64 for `milliseconds:
    // i64::MAX` — the checked-multiply guard must surface this as an
    // error, not silently wrap or saturate.
    let result =
        engine.execute_cypher("RETURN duration({milliseconds: 9223372036854775807}) AS result");
    assert!(
        result.is_err(),
        "duration milliseconds * 1_000_000 overflow must error, not silently saturate; got: {:?}",
        result
    );
}

// ============================================================================
// `duration_from_map`'s fractional-vs-integer gate must key on genuinely
// fractional JSON-float syntax, not merely "not an integer" — a
// non-numeric `years`/`months`/`weeks`/`days` field must NOT reroute a
// large, exactly-representable `seconds` integer off the checked path.
// ============================================================================

#[test]
fn test_duration_non_numeric_year_field_does_not_corrupt_large_integer_seconds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `years` is a string (non-numeric, defaults to `0`, same as always) —
    // it must NOT force `seconds: i64::MAX` through the lossy `f64`
    // fractional-carry path, which would round it to `9223372036854775808`
    // (one past `i64::MAX`, since `f64` cannot represent that exact value)
    // instead of preserving it exactly.
    let result = execute_query(
        &mut engine,
        "RETURN duration({years: 'oops', seconds: 9223372036854775807}) AS result",
    );
    assert_eq!(
        get_single_value(&result).as_str(),
        Some("PT2562047788015215H30M7S"),
        "a non-numeric years field must not corrupt an exactly-representable large seconds value"
    );
}
