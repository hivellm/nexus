//! Regression coverage for the instant map constructors' (`localtime`,
//! `time`, `localdatetime`, `datetime`) sub-second keys: `millisecond`,
//! `microsecond` and `nanosecond` used to be mutually exclusive — only
//! `nanosecond` was honored, and `millisecond`/`microsecond` were silently
//! dropped. They now compose additively into the nanosecond-of-second
//! field (`millisecond * 1_000_000 + microsecond * 1_000 + nanosecond`),
//! and an out-of-range component is a hard query error rather than a
//! silent fold.
//!
//! Every expected rendering below is taken verbatim from the vendored
//! openCypher TCK's `Temporal1.feature` examples (see
//! `crates/nexus-core/tests/tck/opencypher/features/expressions/temporal/Temporal1.feature`).

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
// Temporal1.feature — `localtime({...})` renders the composed sub-second
// fraction, trimming trailing zero groups (3/6/9 significant digits).
// ============================================================================

#[test]
fn localtime_all_three_subsecond_keys_compose() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14, nanosecond:789, millisecond:123, microsecond:456})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.123456789");
}

#[test]
fn localtime_microsecond_alone_renders_six_digits() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14, microsecond:645876})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.645876");
}

#[test]
fn localtime_millisecond_alone_renders_three_digits() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14, millisecond:645})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.645");
}

#[test]
fn localtime_with_no_subsecond_keys_omits_the_fraction_entirely() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14})",
    );
    assert_eq!(get_single_value(&result), "12:31:14");
}

// ============================================================================
// Composition pins: a coarser neighbour narrows the finer key's slot, and
// the rendering width follows from how many digits are actually non-zero.
// ============================================================================

#[test]
fn localtime_millisecond_and_nanosecond_compose_into_nine_digits() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14, millisecond:645, nanosecond:2})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.645000002");
}

#[test]
fn localtime_millisecond_microsecond_and_nanosecond_all_given_compose_in_order() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localtime({hour:12,minute:31,second:14, millisecond:1, microsecond:2, nanosecond:3})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.001002003");
}

// ============================================================================
// `time({...})` — same composition, plus the `Z`/fixed-offset suffix.
// ============================================================================

#[test]
fn time_nanosecond_alone_renders_nine_digits_with_the_zulu_suffix() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN time({hour:12,minute:31,second:14, nanosecond:3})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.000000003Z");
}

#[test]
fn time_millisecond_with_a_fixed_offset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN time({hour:12,minute:31,second:14, millisecond:645, timezone:'+01:00'})",
    );
    assert_eq!(get_single_value(&result), "12:31:14.645+01:00");
}

// ============================================================================
// `localdatetime({...})` and `datetime({...})` — same sub-second
// composition carried through the full calendar constructors.
// ============================================================================

#[test]
fn localdatetime_microsecond_alone() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN localdatetime({year:1984,month:10,day:11,hour:12,minute:31,second:14, microsecond:645876})",
    );
    assert_eq!(get_single_value(&result), "1984-10-11T12:31:14.645876");
}

#[test]
fn datetime_microsecond_with_a_fixed_offset() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN datetime({year:1984,month:10,day:11,hour:12,minute:31,second:14, microsecond:645876, timezone:'+01:00'})",
    );
    assert_eq!(
        get_single_value(&result),
        "1984-10-11T12:31:14.645876+01:00"
    );
}

// ============================================================================
// The composed value round-trips through a variable: its own
// `.millisecond`/`.microsecond`/`.nanosecond` accessors read back the
// total-in-unit quantities, not just the key that was written.
// ============================================================================

#[test]
fn localtime_composed_value_round_trips_through_its_own_accessors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH localtime({hour:12,minute:31,second:14, millisecond:123, microsecond:456, nanosecond:789}) AS t \
         RETURN t.millisecond, t.microsecond, t.nanosecond",
    );
    let values = row_values(&result);
    assert_eq!(values[0].as_i64(), Some(123));
    assert_eq!(values[1].as_i64(), Some(123_456));
    assert_eq!(values[2].as_i64(), Some(123_456_789));
}

// ============================================================================
// An out-of-range component is a hard query error, not a null result or a
// silently folded value.
// ============================================================================

#[test]
fn localtime_millisecond_out_of_range_is_a_hard_query_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("RETURN localtime({hour:12,minute:31,second:14, millisecond:1000})")
        .expect_err("millisecond: 1000 is out of [0, 999] and must error, not fold");
    assert!(
        err.to_string().contains("millisecond"),
        "error message must name the offending key, got: {err}"
    );
}
