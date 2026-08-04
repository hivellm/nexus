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

#[test]
fn order_by_orders_stored_durations_by_component_value_not_lexicographically() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Plain canonical-string lexicographic order (correct for dates, see
    // `order_by_orders_stored_dates_chronologically` above) is WRONG for
    // durations: `"PT10H"` sorts before `"PT9H"` under `str::cmp` (`'1'` <
    // `'9'`) even though 10 hours is the longer duration. Neo4j's
    // average-length ordering (`months * AVG_SECONDS_PER_MONTH + days *
    // 86_400 + seconds`) must put `PT9H` first — for these two operands
    // (`months = days = 0` on both) the average-length key degenerates to
    // the raw second count, so this case alone doesn't distinguish it from
    // a plain component-tuple compare; see the `P1M`-vs-`P31D` test below
    // for a case where the two models disagree.
    execute_query(&mut engine, "CREATE (:D {dur: duration({hours: 10})})");
    execute_query(&mut engine, "CREATE (:D {dur: duration({hours: 9})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(&mut engine, "MATCH (d:D) RETURN d.dur ORDER BY d.dur ASC");
    let durations: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(durations, vec!["PT9H", "PT10H"]);
}

#[test]
fn order_by_orders_a_mix_of_stored_and_freshly_constructed_durations() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // A stored duration (round-tripped through storage as a plain
    // canonical string) and freshly-constructed ones (still a tagged
    // intermediate value at comparison time) must sort into the same
    // total order — `compare_values_for_sort` canonicalizes each operand
    // independently before comparing, so the two representations are
    // never distinguishable to the comparator.
    execute_query(&mut engine, "CREATE (:D {dur: duration({hours: 10})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (d:D) UNWIND [d.dur, duration({hours: 9}), duration({minutes: 30})] AS x \
         RETURN x ORDER BY x ASC",
    );
    let durations: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(durations, vec!["PT30M", "PT9H", "PT10H"]);
}

#[test]
fn where_less_than_and_greater_than_compare_durations_by_component_value() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // No openCypher TCK scenario in this corpus pins `<`/`>` semantics for
    // Duration operands specifically (grepped `expressions/comparison/*`
    // and `expressions/temporal/*` — no match); this locks in the actual
    // resulting behavior of the shared `compare_values_for_sort` total
    // order, which the executor's `BinaryOp` evaluator
    // (`projection/core.rs`) already routes `<`/`<=`/`>`/`>=` through for
    // every value kind, durations included — the same comparator ORDER BY
    // uses, not a separate "durations aren't orderable" error path.
    execute_query(&mut engine, "CREATE (:D {dur: duration({hours: 10})})");
    engine.refresh_executor().unwrap();
    let result = execute_query(
        &mut engine,
        "MATCH (d:D) RETURN d.dur < duration({hours: 9}) AS lt, \
         d.dur > duration({hours: 9}) AS gt",
    );
    let row = row_values(&result);
    assert_eq!(
        row[0],
        serde_json::Value::Bool(false),
        "PT10H should not be < PT9H"
    );
    assert_eq!(
        row[1],
        serde_json::Value::Bool(true),
        "PT10H should be > PT9H"
    );
}

#[test]
fn order_by_orders_durations_by_average_length_not_a_bare_component_tuple() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Neo4j orders durations by "average length"
    // (`months * AVG_SECONDS_PER_MONTH + days * 86_400 + seconds`,
    // `AVG_SECONDS_PER_MONTH = 2_629_746`), matching
    // `DurationValue.unsafeCompareTo` — NOT a bare lexicographic
    // `(months, days, seconds, nanos)` tuple compare, which would put the
    // larger `months` field first regardless of magnitude. `P1M` (average
    // length 2_629_746s) is shorter than `P31D` (31 * 86_400 =
    // 2_678_400s) even though a tuple compare would say `months: 1 > 0` and
    // rank `P1M` after `P31D`.
    let result = execute_query(
        &mut engine,
        "UNWIND [duration({days: 31}), duration({months: 1})] AS x RETURN x ORDER BY x ASC",
    );
    let durations: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(durations, vec!["P1M", "P31D"], "P1M must sort before P31D");
}

#[test]
fn order_by_orders_p12m_before_p366d_by_average_length() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `P12M` (12 * 2_629_746 = 31_556_952s) is shorter than `P366D`
    // (366 * 86_400 = 31_622_400s) under the average-length model. `P12M`
    // canonically renders as `'P1Y'` (12 whole months fold into 1 year —
    // see `temporal_value::duration_parts`), so that's the expected
    // rendering here, not a literal `'P12M'`; the ordering claim (P12M
    // sorts before P366D) is what this test actually pins.
    let result = execute_query(
        &mut engine,
        "UNWIND [duration({days: 366}), duration({months: 12})] AS x RETURN x ORDER BY x ASC",
    );
    let durations: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(
        durations,
        vec!["P1Y", "P366D"],
        "P12M (rendered 'P1Y') must sort before P366D"
    );
}

#[test]
fn order_by_on_a_mix_of_duration_and_non_duration_strings_is_deterministic() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `'PT5'` is not a valid duration (no unit letter) — a comparator that
    // only special-cases the `(Some, Some)` duration/duration case and
    // falls back to plain `str::cmp` for every other pairing is
    // internally inconsistent (whether a duration sorts before or after
    // `'PT5'` would depend on which side of the pair it's on), which
    // breaks the total order `ORDER BY`'s sort needs and risks an
    // inconsistent-comparator panic. The class-rank fix ranks every
    // duration before every non-duration string, so the durations must
    // end up contiguous (internally ordered `PT9H` before `PT10H`) with
    // the non-duration string on one consistent side.
    let result = execute_query(
        &mut engine,
        "UNWIND ['PT10H', 'PT5', 'PT9H'] AS x RETURN x ORDER BY x ASC",
    );
    let values: Vec<&str> = result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(values, vec!["PT9H", "PT10H", "PT5"]);
}

#[test]
fn order_by_and_where_resolve_a_tied_average_length_via_the_tuple_tiebreak() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `duration({months: 1})` (average length 1 * 2_629_746 = 2_629_746)
    // and `duration({days: 30, seconds: 37746})` (30 * 86_400 + 37_746 =
    // 2_592_000 + 37_746 = 2_629_746) land on the EXACT SAME average-length
    // key — the case `compare_duration_parts`'s tuple tiebreak exists for.
    // Tuple compare (months, days, seconds, nanos): `(1, 0, 0, 0)` vs
    // `(0, 30, 37_746, 0)` — the first field alone decides it (`1 > 0`), so
    // `{months: 1}` sorts AFTER `{days: 30, seconds: 37746}` despite the
    // tied average length, and the two are neither `<` nor `=` each other.
    let order_result = execute_query(
        &mut engine,
        "UNWIND [duration({months: 1}), duration({days: 30, seconds: 37746})] AS x \
         RETURN x ORDER BY x ASC",
    );
    let durations: Vec<&str> = order_result
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(
        durations,
        vec!["P30DT10H29M6S", "P1M"],
        "tied average-length durations must still resolve deterministically via the tuple \
         tiebreak"
    );

    let cmp_result = execute_query(
        &mut engine,
        "RETURN duration({months: 1}) < duration({days: 30, seconds: 37746}) AS lt, \
         duration({months: 1}) = duration({days: 30, seconds: 37746}) AS eq",
    );
    let row = row_values(&cmp_result);
    assert_eq!(
        row[0],
        serde_json::Value::Bool(false),
        "{{months: 1}} is not < {{days: 30, seconds: 37746}} (tuple tiebreak ranks it greater)"
    );
    assert_eq!(
        row[1],
        serde_json::Value::Bool(false),
        "a tied average-length key does not imply equality"
    );
}

#[test]
fn order_by_desc_is_the_exact_reverse_of_asc_on_a_mixed_duration_and_plain_string_column() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Same mixed duration/non-duration-string column as
    // `order_by_on_a_mix_of_duration_and_non_duration_strings_is_deterministic`
    // above (ASC: `["PT9H", "PT10H", "PT5"]`) — `DESC` must be its exact
    // reverse, proving the class-rank + average-length comparator is a
    // genuine total order (not merely consistent in one sort direction).
    let asc = execute_query(
        &mut engine,
        "UNWIND ['PT10H', 'PT5', 'PT9H'] AS x RETURN x ORDER BY x ASC",
    );
    let desc = execute_query(
        &mut engine,
        "UNWIND ['PT10H', 'PT5', 'PT9H'] AS x RETURN x ORDER BY x DESC",
    );
    let asc_values: Vec<&str> = asc
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    let mut desc_values: Vec<&str> = desc
        .rows
        .iter()
        .map(|r| r.values[0].as_str().unwrap())
        .collect();
    assert_eq!(asc_values, vec!["PT9H", "PT10H", "PT5"]);
    desc_values.reverse();
    assert_eq!(
        desc_values, asc_values,
        "DESC must be the exact reverse of ASC"
    );
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
// A named `timezone` map value resolves via chrono-tz's bundled IANA
// database, honoring the zone's real historical DST rules; a numeric
// offset or `'Z'`/`'UTC'` continues to work exactly as before. An
// unrecognised zone name is still a hard error.
// ============================================================================

#[test]
fn datetime_with_a_named_timezone_resolves_via_chrono_tz() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN datetime({year: 1984, timezone: 'Europe/Stockholm'}) AS d",
    );
    // 1 January 1984 is Stockholm winter time (`+01:00`, CET); the zone
    // name persists through to the canonical rendering.
    assert_eq!(
        get_single_value(&result),
        "1984-01-01T00:00+01:00[Europe/Stockholm]"
    );
}

#[test]
fn time_with_a_named_timezone_resolves_via_chrono_tz() {
    // `TIME` has no calendar date of its own, so a named zone resolves
    // against the CURRENT INSTANT (see
    // `temporal_retag::resolve_timezone_string_at_current_instant`'s doc
    // comment) — Stockholm is only ever `+01:00` (CET) or `+02:00` (CEST),
    // so assert one of those two rather than hard-coding whichever is
    // correct at the moment this test happens to run.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN time({hour: 12, timezone: 'Europe/Stockholm'}) AS d",
    );
    let rendered = get_single_value(&result).as_str().unwrap().to_string();
    assert!(
        rendered == "12:00+01:00" || rendered == "12:00+02:00",
        "expected Stockholm's standard or daylight-saving offset, got {rendered:?}"
    );
}

#[test]
fn datetime_with_an_unknown_zone_name_is_a_hard_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = engine.execute_cypher("RETURN datetime({year: 1984, timezone: 'Not/AZone'}) AS d");
    assert!(
        result.is_err(),
        "an unrecognised IANA zone name must still error explicitly; got: {result:?}"
    );
}

// ============================================================================
// A `datetime('...[Zone]')` STRING literal with NO written offset resolves
// the offset from the zone itself (openCypher TCK `Temporal2.feature`
// scenario [6]); an unrecognised bracket zone name is `Null`, not an error
// (the established convention for any other unparseable string literal).
// ============================================================================

#[test]
fn datetime_string_with_a_zone_bracket_and_no_written_offset_resolves_from_the_zone() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        get_single_value(&execute_query(
            &mut engine,
            "RETURN datetime('2015-07-21T21:40:32.142[Europe/London]') AS d",
        )),
        "2015-07-21T21:40:32.142+01:00[Europe/London]"
    );
    // 1818 predates any standardized zone — chrono-tz's bundled tzdata
    // resolves Stockholm's Local Mean Time, a sub-minute UTC offset.
    assert_eq!(
        get_single_value(&execute_query(
            &mut engine,
            "RETURN datetime('1818-07-21T21:40:32.142[Europe/Stockholm]') AS d",
        )),
        "1818-07-21T21:40:32.142+00:53:28[Europe/Stockholm]"
    );
}

#[test]
fn datetime_string_with_a_written_offset_and_a_zone_bracket_trusts_the_written_offset() {
    // openCypher TCK `Temporal2.feature` scenario [6]'s other three rows:
    // an offset that IS written in the literal is always trusted exactly
    // as written, never re-derived from the zone.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        get_single_value(&execute_query(
            &mut engine,
            "RETURN datetime('2015-07-21T21:40:32.142+02:00[Europe/Stockholm]') AS d",
        )),
        "2015-07-21T21:40:32.142+02:00[Europe/Stockholm]"
    );
    assert_eq!(
        get_single_value(&execute_query(
            &mut engine,
            "RETURN datetime('2015-07-21T21:40:32.142+0845[Australia/Eucla]') AS d",
        )),
        "2015-07-21T21:40:32.142+08:45[Australia/Eucla]"
    );
    assert_eq!(
        get_single_value(&execute_query(
            &mut engine,
            "RETURN datetime('2015-07-21T21:40:32.142-04[America/New_York]') AS d",
        )),
        "2015-07-21T21:40:32.142-04:00[America/New_York]"
    );
}

#[test]
fn datetime_string_with_an_unknown_bracket_zone_is_null_not_an_error() {
    // Distinct from the map constructor's `timezone` key (always a hard
    // error for an unresolvable value) — an unrecognised zone bracket in
    // a STRING literal follows this grammar's established "can't parse it,
    // return Null" convention (see `date_string_invalid_form_returns_null_not_error`
    // in `temporal_iso_parse_test.rs` for the sibling `date('...')` case).
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN datetime('2015-07-21T21:40:32.142[Not/AZone]') AS d",
    );
    assert_eq!(get_single_value(&result), &serde_json::Value::Null);
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
