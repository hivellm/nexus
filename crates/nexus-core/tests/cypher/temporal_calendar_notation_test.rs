//! `date({...})` and friends accept four calendar notations, not one.
//!
//! Every temporal constructor read `year`/`month`/`day` and nothing else, so
//! an ISO week date, a quarter date or an ordinal date silently produced
//! January 1st of the given year — a wrong answer reported as success:
//!
//! ```cypher
//! RETURN date({year: 1816, week: 52})   -- was 1816-01-01, want 1816-12-23
//! ```

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

#[track_caller]
fn scalar(query: &str) -> Value {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = engine
        .execute_cypher(query)
        .unwrap_or_else(|e| panic!("`{query}` failed: {e}"));
    rs.rows
        .first()
        .unwrap_or_else(|| panic!("`{query}` returned no rows"))
        .values
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("`{query}` returned no columns"))
}

#[track_caller]
fn assert_text(query: &str, want: &str) {
    let got = scalar(query);
    let text = match &got {
        Value::String(s) => s.clone(),
        // A tagged temporal value carries its rendering alongside the tag.
        Value::Object(map) => map
            .get("_nexus_temporal_value")
            .or_else(|| map.get("value"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{got:?}")),
        other => format!("{other:?}"),
    };
    assert!(
        text.starts_with(want),
        "`{query}` gave `{text}`, wanted it to start with `{want}`"
    );
}

// ── ISO week dates ─────────────────────────────────────────────────────

#[test]
fn a_week_without_a_day_is_that_weeks_monday() {
    assert_text("RETURN date({year: 1816, week: 52})", "1816-12-23");
}

#[test]
fn a_week_and_day_of_week_resolve_together() {
    assert_text(
        "RETURN date({year: 1984, week: 10, dayOfWeek: 3})",
        "1984-03-07",
    );
}

#[test]
fn the_week_year_may_differ_from_the_calendar_year() {
    // ISO week 1 of 2019 begins in December 2018 — the week number is counted
    // in the week-year, which is the whole point of the notation.
    assert_text(
        "RETURN date({year: 2019, week: 1, dayOfWeek: 1})",
        "2018-12-31",
    );
}

#[test]
fn week_fields_work_on_localdatetime_and_datetime_too() {
    assert_text("RETURN localdatetime({year: 1816, week: 52})", "1816-12-23");
    assert_text("RETURN datetime({year: 1816, week: 52})", "1816-12-23");
}

// ── quarter dates ──────────────────────────────────────────────────────

#[test]
fn a_quarter_and_day_of_quarter_resolve_together() {
    assert_text(
        "RETURN date({year: 1984, quarter: 1, dayOfQuarter: 67})",
        "1984-03-07",
    );
}

#[test]
fn a_quarter_without_a_day_is_that_quarters_first_day() {
    assert_text("RETURN date({year: 1984, quarter: 3})", "1984-07-01");
}

// ── ordinal dates ──────────────────────────────────────────────────────

#[test]
fn an_ordinal_day_resolves_to_its_calendar_day() {
    assert_text("RETURN date({year: 1984, ordinalDay: 67})", "1984-03-07");
}

#[test]
fn an_ordinal_day_respects_leap_years() {
    // 1984 is a leap year and 1983 is not, so the same ordinal is a different
    // calendar day in each.
    assert_text("RETURN date({year: 1983, ordinalDay: 67})", "1983-03-08");
}

// ── the calendar form still works ──────────────────────────────────────

#[test]
fn calendar_fields_are_unaffected() {
    assert_text("RETURN date({year: 1984, month: 3, day: 7})", "1984-03-07");
    assert_text("RETURN date({year: 1984})", "1984-01-01");
    assert_text(
        "RETURN localdatetime({year: 1984, month: 3, day: 7, hour: 12, minute: 31})",
        "1984-03-07T12:31",
    );
}

// ── selecting from an existing temporal value ──────────────────────────

#[test]
fn a_date_selector_copies_the_selected_date() {
    assert_text(
        "WITH date({year: 1984, month: 11, day: 11}) AS other RETURN date({date: other})",
        "1984-11-11",
    );
}

#[test]
fn a_time_selector_copies_the_selected_time() {
    assert_text(
        "WITH localtime({hour: 12, minute: 31, second: 14, nanosecond: 645876123}) AS other \
         RETURN localtime({time: other})",
        "12:31:14.645876123",
    );
}

#[test]
fn map_components_override_the_selected_value() {
    assert_text(
        "WITH date({year: 1984, month: 11, day: 11}) AS other RETURN date({date: other, day: 1})",
        "1984-11-01",
    );
    assert_text(
        "WITH localtime({hour: 12, minute: 31, second: 14}) AS other \
         RETURN localtime({time: other, second: 42})",
        "12:31:42",
    );
}

#[test]
fn a_week_override_counts_in_the_selected_values_week_year() {
    // 1816-12-30 already belongs to ISO week-year 1817, so week 2 of it is in
    // January 1817 — reading the selector's *calendar* year would land a year
    // earlier.
    assert_text(
        "RETURN date({date: date('1816-12-30'), week: 2, dayOfWeek: 3})",
        "1817-01-08",
    );
}

#[test]
fn a_datetime_selector_supplies_both_halves() {
    assert_text(
        "WITH localdatetime({year: 1984, month: 3, day: 7, hour: 12, minute: 31}) AS other \
         RETURN localdatetime({datetime: other})",
        "1984-03-07T12:31",
    );
}

// ── fields naming no real date ─────────────────────────────────────────

#[test]
fn fields_that_name_no_real_date_are_null() {
    for q in [
        "RETURN date({year: 1984, week: 54})",
        "RETURN date({year: 1984, quarter: 5})",
        "RETURN date({year: 1983, ordinalDay: 366})",
        "RETURN date({year: 1984, month: 13})",
    ] {
        assert!(
            matches!(scalar(q), Value::Null),
            "`{q}` should be null, got {:?}",
            scalar(q)
        );
    }
}
