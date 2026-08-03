//! Integration tests for `date.truncate`/`datetime.truncate`/
//! `localdatetime.truncate`/`time.truncate`/`localtime.truncate`
//! (`executor::eval::temporal_truncate`), exercised through the full
//! `/cypher`-shaped `Engine::execute_cypher` pipeline. Expected values are
//! taken directly from the openCypher TCK's `Temporal9.feature` scenario
//! outline tables (`[1]` through `[5]`).

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
// [1] Should truncate date — Temporal9.feature.
// ============================================================================

#[test]
fn date_truncate_millennium_and_century() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('millennium', date({year: 2017, month: 10, day: 11}), \
             {day: 2}) AS result"
        ),
        Some("2000-01-02".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('century', date({year: 1984, month: 10, day: 11}), {}) AS \
             result"
        ),
        Some("1900-01-01".to_string())
    );
}

#[test]
fn date_truncate_decade_and_year_from_a_datetime_source() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('decade', datetime({year: 1984, month: 10, day: 11, hour: \
             12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), {}) AS \
             result"
        ),
        Some("1980-01-01".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('year', localdatetime({year: 1984, month: 10, day: 11, hour: \
             12, minute: 31, second: 14, nanosecond: 645876123}), {day: 2}) AS result"
        ),
        Some("1984-01-02".to_string())
    );
}

#[test]
fn date_truncate_weekyear_differs_from_calendar_year() {
    // datetime({year: 1984, month: 1, day: 1, ...}) — Jan 1 1984 is a
    // Sunday, so its ISO week-year is 1983: `weekYear` truncates to the
    // Monday of week 1 of *1983*, not 1984.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('weekYear', datetime({year: 1984, month: 1, day: 1, hour: \
             12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), {}) AS \
             result"
        ),
        Some("1983-01-03".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('weekYear', date({year: 1984, month: 2, day: 1}), {day: 5}) \
             AS result"
        ),
        Some("1984-01-05".to_string())
    );
}

#[test]
fn date_truncate_quarter_and_month() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('quarter', date({year: 1984, month: 11, day: 11}), {day: 2}) \
             AS result"
        ),
        Some("1984-10-02".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('month', date({year: 1984, month: 10, day: 11}), {}) AS \
             result"
        ),
        Some("1984-10-01".to_string())
    );
}

#[test]
fn date_truncate_week_with_day_of_week_override() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('week', date({year: 1984, month: 10, day: 11}), {}) AS result"
        ),
        Some("1984-10-08".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('week', date({year: 1984, month: 10, day: 11}), {dayOfWeek: \
             2}) AS result"
        ),
        Some("1984-10-09".to_string())
    );
}

#[test]
fn date_truncate_day_is_a_no_op_on_the_calendar_date() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN date.truncate('day', date({year: 1984, month: 10, day: 11}), {}) AS result"
        ),
        Some("1984-10-11".to_string())
    );
}

// ============================================================================
// [2] Should truncate datetime — Temporal9.feature.
// ============================================================================

#[test]
fn datetime_truncate_from_a_date_source_defaults_offset_to_z() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('millennium', date({year: 2017, month: 10, day: 11}), \
             {}) AS result"
        ),
        Some("2000-01-01T00:00Z".to_string())
    );
}

#[test]
fn datetime_truncate_from_a_datetime_source_keeps_its_offset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('millennium', datetime({year: 2017, month: 10, day: 11, \
             hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {day: 2}) AS result"
        ),
        Some("2000-01-02T00:00+01:00".to_string())
    );
}

#[test]
fn datetime_truncate_nanosecond_override_adds_to_the_truncated_remainder() {
    // millisecond truncation of 645876123 leaves 645000000; the
    // {nanosecond: 2} override must land on 645000002, not 2.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('millisecond', datetime({year: 1984, month: 10, day: 11, \
             hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {nanosecond: 2}) AS result"
        ),
        Some("1984-10-11T12:31:14.645000002+01:00".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('microsecond', datetime({year: 1984, month: 10, day: 11, \
             hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {nanosecond: 2}) AS result"
        ),
        Some("1984-10-11T12:31:14.645876002+01:00".to_string())
    );
}

#[test]
fn datetime_truncate_hour_minute_second_zero_the_remainder() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('hour', datetime({year: 1984, month: 10, day: 11, hour: \
             12, minute: 31, second: 14, nanosecond: 645876123, timezone: '-01:00'}), {}) AS \
             result"
        ),
        Some("1984-10-11T12:00-01:00".to_string())
    );
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN datetime.truncate('second', datetime({year: 1984, month: 10, day: 11, \
             hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {}) AS result"
        ),
        Some("1984-10-11T12:31:14+01:00".to_string())
    );
}

#[test]
fn datetime_truncate_with_a_named_timezone_override_is_a_hard_error() {
    // Same gap as every other `timezone` map key in this codebase (no
    // timezone database wired in yet) — a numeric override still works,
    // a named IANA zone errors explicitly rather than silently falling
    // back to UTC.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher(
        "RETURN datetime.truncate('year', date({year: 1984, month: 10, day: 11}), {timezone: \
         'Europe/Stockholm'}) AS result",
    );
    assert!(result.is_err(), "got: {result:?}");
}

// ============================================================================
// [3] Should truncate localdatetime — Temporal9.feature.
// ============================================================================

#[test]
fn localdatetime_truncate_drops_any_source_offset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN localdatetime.truncate('century', datetime({year: 1984, month: 10, day: \
             11, hour: 12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {day: 2}) AS result"
        ),
        Some("1900-01-02T00:00".to_string())
    );
}

#[test]
fn localdatetime_truncate_hour_keeps_the_hour_field() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN localdatetime.truncate('hour', localdatetime({year: 1984, month: 10, day: \
             11, hour: 12, minute: 31, second: 14, nanosecond: 645876123}), {}) AS result"
        ),
        Some("1984-10-11T12:00".to_string())
    );
}

// ============================================================================
// [4] Should truncate localtime — Temporal9.feature.
// ============================================================================

#[test]
fn localtime_truncate_day_zeroes_the_whole_time_of_day() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN localtime.truncate('day', datetime({year: 1984, month: 10, day: 11, hour: \
             12, minute: 31, second: 14, nanosecond: 645876123, timezone: '+01:00'}), \
             {nanosecond: 2}) AS result"
        ),
        Some("00:00:00.000000002".to_string())
    );
}

#[test]
fn localtime_truncate_hour_from_a_bare_localtime_source() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN localtime.truncate('hour', localtime({hour: 12, minute: 31, second: 14, \
             nanosecond: 645876123}), {}) AS result"
        ),
        Some("12:00".to_string())
    );
}

// ============================================================================
// [5] Should truncate time — Temporal9.feature.
// ============================================================================

#[test]
fn time_truncate_keeps_the_source_offset_by_default() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN time.truncate('hour', datetime({year: 1984, month: 10, day: 11, hour: 12, \
             minute: 31, second: 14, nanosecond: 645876123, timezone: '-01:00'}), {}) AS result"
        ),
        Some("12:00-01:00".to_string())
    );
}

#[test]
fn time_truncate_with_a_numeric_timezone_override() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN time.truncate('hour', datetime({year: 1984, month: 10, day: 11, hour: 12, \
             minute: 31, second: 14, nanosecond: 645876123, timezone: '-01:00'}), {timezone: \
             '+01:00'}) AS result"
        ),
        Some("12:00+01:00".to_string())
    );
}

#[test]
fn time_truncate_from_a_localtime_source_defaults_offset_to_z() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        single_string(
            &mut engine,
            "RETURN time.truncate('minute', localtime({hour: 12, minute: 31, second: 14, \
             nanosecond: 645876123}), {}) AS result"
        ),
        Some("12:31Z".to_string())
    );
}
