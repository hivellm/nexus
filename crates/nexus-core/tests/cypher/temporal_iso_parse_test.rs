//! Integration tests for the full ISO-8601 string parsing wired into the
//! `date('...')` and `duration('...')` constructors
//! (`executor::eval::temporal_parse`), exercised through the full
//! `/cypher`-shaped `Engine::execute_cypher` pipeline. Expected values are
//! taken directly from the openCypher TCK's `Temporal2.feature` "Should
//! parse date from string" and "Should parse duration from string"
//! scenario outlines.

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn single_string(engine: &mut Engine, query: &str) -> Option<String> {
    let result = execute_query(engine, query);
    assert!(!result.rows.is_empty(), "Result has no rows for: {query}");
    result.rows[0].values[0].as_str().map(str::to_string)
}

// ============================================================================
// date('...') — calendar / week / ordinal, extended and compact notation.
// ============================================================================

#[test]
fn date_string_calendar_extended_and_compact() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015-07-21') AS result"),
        Some("2015-07-21".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('20150721') AS result"),
        Some("2015-07-21".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015-07') AS result"),
        Some("2015-07-01".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('201507') AS result"),
        Some("2015-07-01".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015') AS result"),
        Some("2015-01-01".to_string())
    );
}

#[test]
fn date_string_week_extended_and_compact() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015-W30-2') AS result"),
        Some("2015-07-21".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015W302') AS result"),
        Some("2015-07-21".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015-W30') AS result"),
        Some("2015-07-20".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015W30') AS result"),
        Some("2015-07-20".to_string())
    );
}

#[test]
fn date_string_ordinal_extended_and_compact() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015-202') AS result"),
        Some("2015-07-21".to_string())
    );
    assert_eq!(
        single_string(&mut engine, "RETURN date('2015202') AS result"),
        Some("2015-07-21".to_string())
    );
}

#[test]
fn date_string_invalid_form_returns_null_not_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN date('not-a-date') AS result");
    assert!(result.rows[0].values[0].is_null());
}

// ============================================================================
// duration('...') — standard component form with fractional-component
// carry, and the ISO-8601 alternative `P<date>T<time>` count form.
// ============================================================================

#[test]
fn duration_string_no_carry() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('P14DT16H12M') AS result"),
        Some("P14DT16H12M".to_string())
    );
}

#[test]
fn duration_string_fractional_day_carries_into_hours() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('P5M1.5D') AS result"),
        Some("P5M1DT12H".to_string())
    );
}

#[test]
fn duration_string_fractional_month_carries_through_days_and_time() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('P0.75M') AS result"),
        Some("P22DT19H51M49.5S".to_string())
    );
}

#[test]
fn duration_string_fractional_minute_carries_into_seconds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('PT0.75M') AS result"),
        Some("PT45S".to_string())
    );
}

#[test]
fn duration_string_fractional_week_carries_into_days_and_hours() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('P2.5W') AS result"),
        Some("P17DT12H".to_string())
    );
}

#[test]
fn duration_string_overflowing_seconds_normalize_into_minutes() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN duration('P12Y5M14DT16H12M70S') AS result"
        ),
        Some("P12Y5M14DT16H13M10S".to_string())
    );
}

#[test]
fn duration_string_alternative_format() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN duration('P2012-02-02T14:37:21.545') AS result"
        ),
        Some("P2012Y2M2DT14H37M21.545S".to_string())
    );
}

#[test]
fn duration_string_alternative_format_compact() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN duration('P20120202T143721.545') AS result"
        ),
        Some("P2012Y2M2DT14H37M21.545S".to_string())
    );
}

#[test]
fn duration_string_leading_minus_negates_every_component() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(&mut engine, "RETURN duration('-P14DT16H12M') AS result"),
        Some("P-14DT-16H-12M".to_string())
    );
}

#[test]
fn duration_string_invalid_form_returns_null_not_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN duration('not-a-duration') AS result");
    assert!(result.rows[0].values[0].is_null());
}
