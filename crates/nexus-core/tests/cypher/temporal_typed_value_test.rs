//! Integration tests for the typed intermediate temporal value
//! (`executor::eval::temporal_value`): constructor round-trips through the
//! full `/cypher`-shaped `Engine::execute_cypher` pipeline, and the
//! "tagged form never leaks into responses" guarantee across scalars,
//! lists, maps, and `WITH`-carried values.

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

/// The tag key a tagged intermediate temporal value carries before
/// canonicalization — must never appear anywhere in a serialized response.
const TEMPORAL_TAG_KEY: &str = "_nexus_temporal_type";

/// Asserts `value` — and, recursively, anything nested inside it — never
/// contains the tagged-temporal marker key, by round-tripping through
/// `serde_json` and substring-searching the JSON text. This is the
/// strongest possible check short of re-parsing the whole `ResultSet`
/// wire format: any leftover `_nexus_temporal_type` key anywhere in the
/// value tree, at any depth, fails it.
fn assert_no_temporal_tag_leak(value: &serde_json::Value) {
    let serialized = serde_json::to_string(value).expect("value must serialize");
    assert!(
        !serialized.contains(TEMPORAL_TAG_KEY),
        "tagged temporal marker leaked into response: {serialized}"
    );
}

// ============================================================================
// GATE CASES — the F-015 typed-value gate: duration/date must render as
// canonical ISO strings, not the old flat/tagged object shape.
// ============================================================================

#[test]
fn gate_duration_from_map_renders_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN duration({days: 14, hours: 16, minutes: 12}) AS result",
    );
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("P14DT16H12M"));
}

#[test]
fn gate_date_from_map_renders_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN date({year: 2020, month: 1, day: 1}) AS result",
    );
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("2020-01-01"));
}

// ============================================================================
// NO-LEAK — the tagged intermediate form must never survive to a response,
// whether the temporal value is a bare scalar, nested in a list/map, or
// threaded through a WITH before the final RETURN.
// ============================================================================

#[test]
fn no_leak_bare_duration_and_date_scalars() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let result = execute_query(&mut engine, "RETURN duration({days: 3}) AS d");
    let value = get_single_value(&result);
    assert!(value.is_string(), "expected a string, got: {value:?}");
    assert_no_temporal_tag_leak(value);

    let result = execute_query(
        &mut engine,
        "RETURN date({year: 2021, month: 6, day: 15}) AS d",
    );
    let value = get_single_value(&result);
    assert!(value.is_string(), "expected a string, got: {value:?}");
    assert_no_temporal_tag_leak(value);
}

#[test]
fn no_leak_list_containing_temporals() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN [date({year: 2020, month: 1, day: 1}), duration({days: 1})] AS items",
    );
    let value = get_single_value(&result);
    let items = value.as_array().expect("expected a list");
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].as_str(), Some("2020-01-01"));
    assert_eq!(items[1].as_str(), Some("P1D"));
    assert_no_temporal_tag_leak(value);
}

#[test]
fn no_leak_map_containing_a_temporal() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "RETURN {label: 'meeting', when: duration({hours: 2})} AS m",
    );
    let value = get_single_value(&result);
    let map = value.as_object().expect("expected a map");
    assert_eq!(map.get("when").and_then(|v| v.as_str()), Some("PT2H"));
    assert_no_temporal_tag_leak(value);
}

#[test]
fn no_leak_temporal_stored_in_with_then_returned() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "WITH duration({days: 14, hours: 16, minutes: 12}) AS d RETURN d",
    );
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("P14DT16H12M"));
    assert_no_temporal_tag_leak(value);
}

#[test]
fn order_by_on_canonical_iso_date_strings_sorts_chronologically() {
    // ORDER BY runs while values are still tagged (canonicalization only
    // happens once, at the projection boundary); `compare_values_for_sort`
    // canonicalizes temporals before comparing so same-kind dates still
    // sort chronologically (canonical
    // fixed-width ISO strings are lexicographically ordered).
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let result = execute_query(
        &mut engine,
        "UNWIND [date({year: 2020, month: 3, day: 1}), date({year: 2019, month: 12, day: 25}), date({year: 2020, month: 1, day: 10})] AS d \
         RETURN d ORDER BY d",
    );
    let rendered: Vec<&str> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_str().unwrap())
        .collect();
    assert_eq!(rendered, vec!["2019-12-25", "2020-01-10", "2020-03-01"]);
}

// ============================================================================
// STORAGE BOUNDARY — a tagged intermediate temporal value must canonicalize
// before it is ever persisted into a node/relationship property, not just
// before it leaves the executor in a `RETURN` row. A raw tagged object
// written to storage is a durable on-disk marker that would also corrupt
// any index built over the property.
// ============================================================================

#[test]
fn create_duration_property_persists_as_canonical_string_not_tagged_object() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // `UNWIND ... CREATE` (rather than a bare standalone `CREATE`) so the
    // executor routes through the row-scoped `execute_create_with_context`
    // / `resolve_property_expr_for_create` path this test targets, not the
    // separate `execute_create_pattern_with_variables` fast path a
    // context-free standalone `CREATE` takes. Both paths now canonicalize
    // function-call property values identically (see
    // `test_create_function_call_properties.rs`); this test keeps the
    // `UNWIND` form specifically to exercise the row-aware
    // `resolve_property_expr_for_create` path rather than the standalone
    // one.
    execute_query(
        &mut engine,
        "UNWIND [1] AS i CREATE (:Event {d: duration({days: 1, hours: 2})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Event) RETURN n.d AS d");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("P1DT2H"),
        "expected the stored property to read back as a canonical ISO string, got: {value:?}"
    );
    assert_no_temporal_tag_leak(value);

    // The whole node too, not just the single property — the tag must not
    // survive anywhere in the persisted property map.
    let result = execute_query(&mut engine, "MATCH (n:Event) RETURN n AS n");
    let node = get_single_value(&result);
    assert_no_temporal_tag_leak(node);
}

#[test]
fn create_date_property_persists_as_canonical_string_not_tagged_object() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "UNWIND [1] AS i CREATE (:Event {d: date({year: 2020, month: 1, day: 1})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:Event) RETURN n.d AS d");
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("2020-01-01"));
    assert_no_temporal_tag_leak(value);
}

// ============================================================================
// WRITE-PATH DEFENSE IN DEPTH — `MERGE`/`SET`/`REMOVE`/`FOREACH` build their
// own inline `RETURN` result (`engine::write_exec::return_builder`) without
// ever calling `Executor::execute`, so the projection-boundary
// canonicalization pass never runs for these statements. `build_return_result`
// reads node/relationship properties straight out of storage, so it must
// canonicalize independently rather than assume whatever wrote the property
// already did.
//
// Neither `CREATE`'s inline property map nor `SET`'s right-hand side
// currently accepts an arbitrary function-call expression like
// `duration({...})` in the *same* write-clause context this test needs
// (`SET n.eta = duration(...)` errors with "Unsupported expression type in
// SET clause" — a separate, pre-existing gap, not something this task's
// scope covers), so there is no live Cypher path today that could still
// hand `build_return_result` a raw tagged value even without this fix. The
// test below instead confirms the *fix itself* is exercised and harmless
// end-to-end: a duration property legitimately created via the (now
// storage-boundary-canonicalized) `CREATE` path reads back correctly
// through the `MERGE` write path's own inline `RETURN` — i.e.
// `build_return_result`'s new `canonicalize_value_in_place` call runs on
// every property read on this path and is a no-op for an already-plain
// value, not a regression.
// ============================================================================

#[test]
fn merge_inline_return_of_a_duration_property_does_not_leak_the_tag() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "UNWIND [1] AS i CREATE (:Task {id: 1, eta: duration({hours: 3})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MERGE (n:Task {id: 1}) RETURN n.eta AS eta");
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("PT3H"));
    assert_no_temporal_tag_leak(value);
}
