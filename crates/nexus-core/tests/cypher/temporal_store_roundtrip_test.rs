//! Integration tests for the temporal store round-trip: a temporal value
//! written by `CREATE`/`MERGE` persists as a plain canonical ISO-8601
//! `Value::String` (see `executor::eval::temporal_value`'s module doc for
//! why the tagged intermediate form never reaches disk), and reading it
//! back through `MATCH` still supports property/component access,
//! comparisons, and arithmetic — via strict, round-trip-verified
//! re-derivation at the point of use (see
//! `executor::eval::temporal_retag`'s module doc for the full design
//! rationale).
//!
//! Covers the openCypher TCK's `Temporal4.feature` (store round-trip),
//! `Temporal5.feature` (component access on a stored value), and
//! `Temporal8.feature` (arithmetic on a stored value) scenario shapes,
//! plus the critical control: an ordinary `STRING` property that is NOT
//! an exact canonical rendering must never gain temporal behaviour.

use nexus_core::testing::setup_isolated_test_engine;
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

fn row_values(result: &ResultSet) -> &[serde_json::Value] {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    &result.rows[0].values
}

// ============================================================================
// STORE ROUND-TRIP — CREATE → MATCH → RETURN yields the plain canonical
// ISO string for every temporal kind (Temporal4.feature).
// ============================================================================

#[test]
fn date_round_trips_through_storage_as_the_canonical_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {d: date({year: 1984, month: 10, day: 11})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "1984-10-11");
}

#[test]
fn localtime_round_trips_omitting_zero_seconds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {d: localtime({hour: 12})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "12:00");
}

#[test]
fn time_round_trips_with_zero_offset_as_z() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {d: time({hour: 12})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "12:00Z");
}

#[test]
fn localdatetime_round_trips_through_storage() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {d: localdatetime({year: 1912})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "1912-01-01T00:00");
}

#[test]
fn datetime_round_trips_defaulting_to_utc() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {d: datetime({year: 1912})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "1912-01-01T00:00Z");
}

#[test]
fn duration_round_trips_through_storage() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {d: duration({seconds: 12})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert_eq!(get_single_value(&result), "PT12S");
}

#[test]
fn array_of_dates_round_trips_through_storage() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {ds: [date({year: 1984, month: 10, day: 12}), \
                            date({year: 1984, month: 10, day: 13})]})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.ds");
    let value = get_single_value(&result);
    assert_eq!(value, &serde_json::json!(["1984-10-12", "1984-10-13"]));
}

#[test]
fn array_of_durations_round_trips_through_storage() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {ds: [duration({seconds: 13}), duration({seconds: 14})]})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.ds");
    let value = get_single_value(&result);
    assert_eq!(value, &serde_json::json!(["PT13S", "PT14S"]));
}

#[test]
fn null_temporal_property_round_trips_as_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {name: 'no date here'})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.d");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn accessor_on_a_null_stored_property_yields_null_not_an_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {name: 'no date here'})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) WITH v.d AS d RETURN d.year");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// ACCESSOR ON STORED VALUE — Temporal5.feature: property/component access
// on a value read back from storage (a plain STRING) must re-derive the
// typed component table via `WITH v.d AS d ... d.year`.
//
// The direct chained form (`RETURN v.d.year`, no intermediate `WITH`) is
// NOT exercised here: it is blocked by a pre-existing, unrelated parser
// limitation — `PropertyAccess`'s AST node addresses its base by a plain
// variable name (`row.get(variable)`), not by a nested expression, so a
// second `.identifier` after a property access is silently dropped by
// the parser (`v.date.year` parses as, and returns, `v.date`) rather
// than chaining. This affects ANY nested property chain, not just
// temporals (there is no other passing test anywhere in this suite that
// exercises `x.y.z`), and fixing it would mean widening `PropertyAccess`
// to carry a boxed sub-expression — an AST-level change well outside
// this task's assigned files. The `WITH`-bound form above is the
// supported, TCK-equivalent idiom and is what this file verifies.
// ============================================================================

#[test]
fn accessor_on_stored_date_via_with_binding() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 1984, month: 10, day: 11})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val) WITH v.date AS d \
         RETURN d.year, d.quarter, d.month, d.day, d.dayOfQuarter",
    );
    let values = row_values(&result);
    let expected = [1984, 4, 10, 11, 11];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

#[test]
fn accessor_on_stored_localtime() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {date: localtime({hour: 12, minute: 31, second: 14, nanosecond: 645876123})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val) WITH v.date AS d \
         RETURN d.hour, d.minute, d.second, d.millisecond, d.microsecond, d.nanosecond",
    );
    let values = row_values(&result);
    let expected = [12, 31, 14, 645, 645_876, 645_876_123];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

#[test]
fn accessor_on_stored_duration() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {d: duration({years: 1, months: 4, days: 10, hours: 1, minutes: 1, seconds: 1})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val) WITH v.d AS d RETURN d.years, d.months, d.days, d.hours, d.minutes",
    );
    let values = row_values(&result);
    // `d.months` is the TOTAL months field (years:1, months:4 -> 16), not
    // the `monthsOfYear` remainder — matches `temporal_accessors`'
    // `duration_property` table. `d.minutes` is likewise the derived
    // total (hours:1, minutes:1 -> 61 total minutes).
    let expected = [1, 16, 10, 1, 61];
    for (value, expected) in values.iter().zip(expected) {
        assert_eq!(value.as_i64(), Some(expected), "row: {values:?}");
    }
}

#[test]
fn temporal_function_reads_a_stored_value_directly() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 1984, month: 10, day: 11})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN year(v.date)");
    assert_eq!(get_single_value(&result).as_i64(), Some(1984));
}

// ============================================================================
// COMPARISON ON STORED VALUE
// ============================================================================

#[test]
fn where_equality_matches_a_freshly_constructed_value_against_a_stored_one() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 1984, month: 10, day: 11})})",
    );
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 2000, month: 1, day: 1})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val) WHERE v.date = date('1984-10-11') RETURN count(v)",
    );
    assert_eq!(get_single_value(&result).as_i64(), Some(1));
}

#[test]
fn order_by_orders_stored_dates_chronologically() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 2000, month: 1, day: 1})})",
    );
    execute_query(
        &mut engine,
        "CREATE (:Val {date: date({year: 1984, month: 10, day: 11})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val) RETURN v.date ORDER BY v.date ASC",
    );
    let dates: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(dates, vec!["1984-10-11", "2000-01-01"]);
}

// ============================================================================
// ARITHMETIC ON STORED VALUE — Temporal8.feature shapes.
// ============================================================================

#[test]
fn datetime_plus_a_stored_duration() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Duration {dur: duration({days: 5})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH date({year: 1984, month: 10, day: 11}) AS x \
         MATCH (d:Duration) RETURN x + d.dur AS sum",
    );
    assert_eq!(get_single_value(&result), "1984-10-16");
}

#[test]
fn stored_duration_plus_stored_duration() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:D1 {v: duration({seconds: 12})})");
    execute_query(&mut engine, "CREATE (:D2 {v: duration({seconds: 13})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (a:D1), (b:D2) RETURN a.v + b.v AS sum");
    assert_eq!(get_single_value(&result), "PT25S");
}

#[test]
fn stored_duration_multiplied_by_a_number() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:D {v: duration({seconds: 12})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (d:D) RETURN d.v * 2 AS prod");
    assert_eq!(get_single_value(&result), "PT24S");
}

#[test]
fn stored_duration_divided_by_a_number() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:D {v: duration({seconds: 12})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (d:D) RETURN d.v / 2 AS div");
    assert_eq!(get_single_value(&result), "PT6S");
}

#[test]
fn localtime_plus_a_stored_duration_wraps_modulo_24h() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:D {dur: duration({hours: 16, minutes: 12, seconds: 70})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH localtime({hour: 12, minute: 31, second: 14, nanosecond: 1}) AS x \
         MATCH (d:D) RETURN x + d.dur AS sum",
    );
    assert_eq!(get_single_value(&result), "04:44:24.000000001");
}

// ============================================================================
// THE CRITICAL CONTROL — a plain user STRING property that is not an
// exact canonical temporal rendering must never gain temporal behaviour:
// RETURN stays the plain string, and accessor/arithmetic reads yield the
// same NULL a non-temporal string always yields.
// ============================================================================

#[test]
fn a_plain_non_canonical_string_property_never_gains_temporal_behaviour() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Looks date-ish at a glance but is padded with zero-seconds the
    // canonical renderer never emits — must stay an ordinary string.
    execute_query(&mut engine, "CREATE (:Val {label: '12:00:00'})");
    execute_query(&mut engine, "CREATE (:Val {label: 'not a date at all'})");
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.label ORDER BY v.label");
    let labels: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    // Wire format is untouched — the plain strings, verbatim.
    assert_eq!(labels, vec!["12:00:00", "not a date at all"]);

    // Component access on a non-canonical string yields NULL, exactly
    // like it would on any other non-temporal STRING property.
    let result = execute_query(
        &mut engine,
        "MATCH (v:Val {label: '12:00:00'}) WITH v.label AS d RETURN d.hour",
    );
    assert!(get_single_value(&result).is_null());

    let result = execute_query(
        &mut engine,
        "MATCH (v:Val {label: 'not a date at all'}) WITH v.label AS d RETURN d.year",
    );
    assert!(get_single_value(&result).is_null());
}

#[test]
fn a_real_canonical_date_string_written_as_a_plain_literal_still_supports_accessors() {
    // The flip side of the control above: this IS the documented
    // string/temporal ambiguity trade-off (see `temporal_retag`'s module
    // doc) — a hand-written literal that happens to be an exact canonical
    // rendering is indistinguishable from one that arrived via a real
    // constructor, and is treated as the corresponding temporal for
    // typed-operation purposes, while still returning as the plain
    // string on a bare `RETURN`.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:Val {label: '1984-10-11'})");
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:Val) RETURN v.label");
    assert_eq!(get_single_value(&result), "1984-10-11");

    let result = execute_query(&mut engine, "MATCH (v:Val) WITH v.label AS d RETURN d.year");
    assert_eq!(get_single_value(&result).as_i64(), Some(1984));
}

// ============================================================================
// NEGATIVE MAP-LITERAL COMPONENTS — root-cause regression lock. Unary
// minus over an integer literal (`-14`) must stay INTEGER-typed, or
// `map.get("days").and_then(Value::as_i64)`-style readers in the
// `duration({...})` map constructor silently read back their `0`
// default (`serde_json::Number::as_i64()` returns `None` for ANY
// Float-backed number, whole-valued or not — see
// `evaluate_projection_expression`'s `UnaryOperator::Minus` arm).
// ============================================================================

#[test]
fn negative_days_in_a_duration_map_literal_is_not_silently_dropped() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN duration({days: -14}) AS d");
    assert_eq!(get_single_value(&result), "P-14D");
}

#[test]
fn date_minus_a_stored_duration_with_a_negative_days_component() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:Duration {dur: duration({months: 1, days: -14, hours: 16, minutes: -12, seconds: 70})})",
    );
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH date({year: 1984, month: 10, day: 11}) AS x \
         MATCH (d:Duration) RETURN x + d.dur AS sum, x - d.dur AS diff",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_str(), Some("1984-10-28"));
    assert_eq!(values[1].as_str(), Some("1984-09-25"));
}

// ============================================================================
// BLOCKER 2 — `nanosecond` map-key validation. An out-of-range value must
// be a hard `CypherExecution` error, never silently reinterpreted as a
// different (smaller, wrapped, or truncated) quantity.
// ============================================================================

#[test]
fn nanosecond_of_exactly_one_and_a_half_billion_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher("RETURN localtime({hour: 12, nanosecond: 1500000000}) AS d");
    assert!(
        result.is_err(),
        "nanosecond: 1500000000 must error, not silently render as 0.15s; got: {result:?}"
    );
}

#[test]
fn nanosecond_far_beyond_u32_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher("RETURN localtime({hour: 12, nanosecond: 4294967297}) AS d");
    assert!(
        result.is_err(),
        "nanosecond: 4294967297 must error, not silently wrap; got: {result:?}"
    );
}

#[test]
fn negative_nanosecond_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher("RETURN localtime({hour: 12, nanosecond: -1}) AS d");
    assert!(
        result.is_err(),
        "nanosecond: -1 must error, not silently become 0; got: {result:?}"
    );
}

// ============================================================================
// MAJOR 1 — an unresolvable named `timezone` map value is a hard error
// until a real timezone database is wired in; a numeric offset or
// `'Z'`/`'UTC'` still works.
// ============================================================================

#[test]
fn datetime_with_an_unresolvable_named_zone_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result =
        engine.execute_cypher("RETURN datetime({year: 1984, timezone: 'Europe/Stockholm'}) AS d");
    assert!(
        result.is_err(),
        "an unresolvable named zone must error, not silently fall back to UTC \
         while still rendering the unresolved name; got: {result:?}"
    );
}

#[test]
fn time_with_an_unresolvable_named_zone_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result =
        engine.execute_cypher("RETURN time({hour: 12, timezone: 'Europe/Stockholm'}) AS d");
    assert!(result.is_err(), "got: {result:?}");
}

#[test]
fn datetime_with_a_numeric_offset_timezone_still_works() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN datetime({year: 1984, timezone: '+02:00'}) AS d",
    );
    assert_eq!(get_single_value(&result), "1984-01-01T00:00+02:00");
}

// ============================================================================
// MAJOR 3 — the `add_values` reorder (temporal arithmetic tried before
// string concatenation) must not swallow genuine string concatenation
// once either operand isn't an exact canonical duration/instant shape.
// ============================================================================

#[test]
fn string_plus_string_still_concatenates_when_not_both_canonical_shapes() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(&mut engine, "RETURN 'P1D' + 'x' AS r");
    assert_eq!(get_single_value(&result), "P1Dx");
}

// ============================================================================
// MAJOR 4 — month-rollover clamping. Adding/subtracting a duration whose
// month/year delta lands on a day that doesn't exist in the target month
// must clamp to the last valid day of that month (matches Neo4j), not
// silently no-op back to the original date.
// ============================================================================

#[test]
fn date_plus_one_month_clamps_january_31_to_february_28() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN date('2015-01-31') + duration({months: 1}) AS d",
    );
    assert_eq!(get_single_value(&result), "2015-02-28");
}

#[test]
fn date_plus_one_year_clamps_a_leap_day_to_february_28() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN date('2016-02-29') + duration({years: 1}) AS d",
    );
    assert_eq!(get_single_value(&result), "2017-02-28");
}

#[test]
fn date_minus_one_month_clamps_march_31_to_february_28() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN date('2015-03-31') - duration({months: 1}) AS d",
    );
    assert_eq!(get_single_value(&result), "2015-02-28");
}

#[test]
fn localdatetime_plus_one_month_clamps_the_day_and_preserves_time_of_day() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime('2015-03-31T12:00:00') + duration({months: 1}) AS d",
    );
    assert_eq!(get_single_value(&result), "2015-04-30T12:00");
}
