//! Integration tests for property-access accessors on tagged temporal
//! values (`executor::eval::temporal_accessors`) — `d.year`, `d.quarter`,
//! `d.dayOfQuarter`, duration's total-vs-`…OfX` remainder pairs, etc.
//! Reproduces the openCypher TCK's `Temporal5.feature` ("Access
//! Components of Temporal Values") scenarios end-to-end through
//! `Engine::execute_cypher`, using only the temporal constructor
//! surface (`date`/`localtime`/`localdatetime`/`duration` map literals)
//! that is already fully wired — a fixed-offset `time`/`datetime`
//! carrying a `timezone` map key, and a `nanoseconds`/`milliseconds`
//! map key on `duration`, are separate, pre-existing constructor gaps
//! (not this accessor dispatch) verified independently by the
//! `executor::eval::temporal_accessors` unit tests, which build tagged
//! values directly and are unaffected by those gaps.

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn row_values(result: &ResultSet) -> &[serde_json::Value] {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    &result.rows[0].values
}

// ============================================================================
// Temporal5.feature scenario [1] — date accessors. The TCK's own query
// shape reads the date back off a stored node property (`CREATE` then
// `MATCH ... WITH v.date AS d`); this codebase's storage layer has no
// native temporal type yet (F-015/workstream #10 — a *separate*,
// unimplemented "typed round trip through storage" gap), so a stored
// property reads back as the already-canonicalized ISO string, not a
// re-hydrated tagged value, and `.year` etc. on that string is correctly
// `Null` today regardless of this task's accessor dispatch. `WITH
// date({...}) AS d` exercises the same tagged-value accessor path
// without depending on that separate gap.
// ============================================================================

#[test]
fn date_accessors_match_tck_scenario_1() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH date({year: 1984, month: 10, day: 11}) AS d \
         RETURN d.year, d.quarter, d.month, d.week, d.weekYear, d.day, d.ordinalDay, d.weekDay, d.dayOfQuarter",
    );
    let values = row_values(&result);
    let expected = [1984, 4, 10, 41, 1984, 11, 285, 4, 11];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

// ============================================================================
// Temporal5.feature scenario [2] — ISO week-year boundary: Jan 1 1984 was
// a Sunday, so it belongs to ISO week 52 of *1983*, not week 1 of 1984.
// ============================================================================

#[test]
fn date_accessors_match_tck_scenario_2_week_year_boundary() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH date({year: 1984, month: 1, day: 1}) AS d \
         RETURN d.year, d.weekYear, d.week, d.weekDay, d.dayOfWeek",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_i64(), Some(1984), "year");
    assert_eq!(values[1].as_i64(), Some(1983), "weekYear");
    assert_eq!(values[2].as_i64(), Some(52), "week");
    assert_eq!(values[3].as_i64(), Some(7), "weekDay");
    assert_eq!(
        values[4].as_i64(),
        Some(7),
        "dayOfWeek must be the documented `weekDay` synonym"
    );
}

// ============================================================================
// Temporal5.feature scenario [3] — localtime accessors. The
// `hour`/`minute`/`second` fields the `localtime({...})` map constructor
// carries through; its `nanosecond` map key is a separate, pre-existing
// constructor gap (silently dropped, see
// `executor::eval::projection::fn_temporal`'s `localtime` map branch),
// so the sub-second accessors are verified directly against a
// hand-built tagged value in `executor::eval::temporal_accessors`'s own
// `localtime_accessors_match_tck_scenario_3` unit test instead.
// ============================================================================

#[test]
fn localtime_accessors_match_tck_scenario_3() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH localtime({hour: 12, minute: 31, second: 14}) AS d \
         RETURN d.hour, d.minute, d.second",
    );
    let values = row_values(&result);
    let expected = [12, 31, 14];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

// ============================================================================
// Temporal5.feature scenario [5] — localdatetime accessors (the date-part
// and hour/minute/second time-part fields the `localdatetime({...})` map
// constructor already carries through; its `nanosecond` map key is a
// separate, pre-existing constructor gap, so the ns-derived accessors
// aren't exercised end-to-end here).
// ============================================================================

#[test]
fn localdatetime_accessors_match_tck_scenario_5() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH localdatetime({year: 1984, month: 11, day: 11, hour: 12, minute: 31, second: 14}) AS d \
         RETURN d.year, d.quarter, d.month, d.week, d.weekYear, d.day, d.ordinalDay, d.weekDay, d.dayOfQuarter, \
                d.hour, d.minute, d.second",
    );
    let values = row_values(&result);
    let expected = [1984, 4, 11, 45, 1984, 11, 316, 7, 42, 12, 31, 14];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

// ============================================================================
// Temporal5.feature scenario [4] / [6] — time/datetime offset accessors,
// exercised at the constructor's default (no explicit `timezone` map key,
// so offset defaults to UTC — matching real Neo4j).
// ============================================================================

#[test]
fn time_offset_accessors_use_the_constructed_offset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH time({hour: 12, minute: 31, second: 14}) AS d \
         RETURN d.offset, d.offsetMinutes, d.offsetSeconds, d.timezone",
    );
    let values = row_values(&result);
    // A zero UTC offset renders as `Z` (matches `java.time.ZoneOffset.UTC`,
    // which Neo4j's own rendering is built on).
    assert_eq!(values[0].as_str(), Some("Z"));
    assert_eq!(values[1].as_i64(), Some(0));
    assert_eq!(values[2].as_i64(), Some(0));
    assert_eq!(values[3].as_str(), Some("Z"));
}

#[test]
fn datetime_epoch_accessors_are_consistent_with_the_constructed_offset() {
    // The `datetime({...})` map constructor defaults to UTC when no
    // explicit `timezone` key is given (matching real Neo4j) — this test
    // still reads back `d.offsetSeconds` rather than hard-coding `0`, so
    // it doubles as a regression check on that default and exercises the
    // accessor's arithmetic invariant directly: `epochSeconds ==
    // (wall-clock reading, taken as UTC) - offsetSeconds`.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH datetime({year: 1984, month: 11, day: 11, hour: 12, minute: 31, second: 14}) AS d \
         RETURN d.offsetSeconds, d.epochSeconds, d.epochMillis",
    );
    let values = row_values(&result);
    let offset_seconds = values[0].as_i64().expect("offsetSeconds must be present");
    let naive_as_utc_seconds = chrono::NaiveDate::from_ymd_opt(1984, 11, 11)
        .unwrap()
        .and_hms_opt(12, 31, 14)
        .unwrap()
        .and_utc()
        .timestamp();
    let expected_seconds = naive_as_utc_seconds - offset_seconds;
    assert_eq!(values[1].as_i64(), Some(expected_seconds));
    assert_eq!(values[2].as_i64(), Some(expected_seconds * 1000));
}

// ============================================================================
// Temporal5.feature scenario [7] — duration accessors: the total-in-unit
// (`years`..`nanoseconds`) vs remainder-within-parent-unit (`…OfYear`..
// `…OfSecond`) split. `seconds: 1.111111111` (a genuinely fractional
// literal) routes the map constructor's f64 fallback path so the
// fractional nanosecond remainder survives, unlike a bare `nanoseconds:`
// map key (a separate, pre-existing constructor gap).
// ============================================================================

#[test]
fn duration_accessors_match_tck_scenario_7() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH duration({years: 1, months: 4, days: 10, hours: 1, minutes: 1, seconds: 1.111111111}) AS d \
         RETURN d.years, d.quarters, d.months, d.weeks, d.days, \
                d.hours, d.minutes, d.seconds, d.milliseconds, d.microseconds, d.nanoseconds, \
                d.quartersOfYear, d.monthsOfQuarter, d.monthsOfYear, d.daysOfWeek, d.minutesOfHour, \
                d.secondsOfMinute, d.millisecondsOfSecond, d.microsecondsOfSecond, d.nanosecondsOfSecond",
    );
    let values = row_values(&result);
    let expected: [i64; 20] = [
        1,
        5,
        16,
        1,
        10,
        1,
        61,
        3661,
        3_661_111,
        3_661_111_111,
        3_661_111_111_111,
        1,
        1,
        4,
        3,
        1,
        1,
        111,
        111_111,
        111_111_111,
    ];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

// ============================================================================
// Temporal10.feature — `duration.between(...)` results also carry the
// tagged shape, so a property access on one must dispatch through the
// same table, not the generic `Object` descent.
// ============================================================================

#[test]
fn duration_between_result_supports_property_access() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `duration.between(a, b)` computes `b - a` (Neo4j's documented
    // `a + result == b` convention — see `temporal_duration_between`'s
    // module doc comment); the earlier date goes first here so the
    // resulting duration is positive.
    let result = execute_query(
        &mut engine,
        "WITH duration.between(date({year: 2020, month: 1, day: 1}), date({year: 2020, month: 1, day: 15})) AS dur \
         RETURN dur, dur.days, dur.seconds, dur.nanosecondsOfSecond",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_str(), Some("P14D"));
    assert_eq!(values[1].as_i64(), Some(14));
    assert_eq!(values[2].as_i64(), Some(0));
    assert_eq!(values[3].as_i64(), Some(0));
}

// ============================================================================
// Regression — an unknown accessor name on a temporal, and ordinary
// property access on a non-temporal map/node, are both untouched by the
// new dispatch.
// ============================================================================

#[test]
fn unknown_accessor_name_on_a_temporal_is_null_not_an_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH date({year: 2020, month: 1, day: 1}) AS d RETURN d.notARealAccessor",
    );
    let values = row_values(&result);
    assert!(values[0].is_null());
}

#[test]
fn plain_map_property_access_is_unaffected_by_temporal_dispatch() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH {year: 2020, month: 1} AS m RETURN m.year, m.month, m.missing",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_i64(), Some(2020));
    assert_eq!(values[1].as_i64(), Some(1));
    assert!(values[2].is_null());
}

// ============================================================================
// Regression — `localdatetime('...')`'s string-literal constructor must
// REJECT an offset present in the literal (returning `Null`, matching the
// `localtime` sibling branch), not silently drop it: `localdatetime` has
// no offset field to carry a caller-supplied one in, and Null surfaces the
// type mismatch instead of discarding information the caller explicitly
// wrote.
// ============================================================================

#[test]
fn localdatetime_string_without_an_offset_still_parses() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime('2015-07-21T21:40:32.142') AS d",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_str(), Some("2015-07-21T21:40:32.142"));
}

#[test]
fn localdatetime_string_with_an_offset_is_rejected_not_silently_dropped() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime('2015-07-21T21:40:32.142+05:30') AS d",
    );
    let values = row_values(&result);
    assert!(
        values[0].is_null(),
        "an offset in a localdatetime literal must be rejected (Null), not dropped: got {:?}",
        values[0]
    );
}

// ============================================================================
// Regression — `duration.between`'s months/days cascade must anchor on
// `a`'s own local reading, not a shared UTC frame, or `a + between(a, b)
// == b` silently breaks whenever the UTC shift crosses a day boundary the
// unshifted comparison would not have.
// ============================================================================

#[test]
fn duration_between_anchors_on_a_and_the_result_added_back_reproduces_b() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH datetime('2015-03-01T00:30+02:00') AS a, datetime('2015-04-01T00:30+02:00') AS b \
         RETURN duration.between(a, b), a + duration.between(a, b) = b",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_str(), Some("P1M"));
    assert_eq!(values[1].as_bool(), Some(true));
}

#[test]
fn duration_between_thirty_minutes_short_of_a_month_is_days_not_a_month() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH datetime('2015-03-01T00:30+02:00') AS a, datetime('2015-04-01T00:00+02:00') AS b \
         RETURN duration.between(a, b), duration.inMonths(a, b), a + duration.between(a, b) = b",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_str(), Some("P30DT23H30M"));
    assert_eq!(values[1].as_str(), Some("PT0S"));
    assert_eq!(values[2].as_bool(), Some(true));
}
