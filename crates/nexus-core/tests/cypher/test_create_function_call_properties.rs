//! Regression coverage for standalone `CREATE`'s function-call property
//! values (`crates/nexus-core/src/executor/operators/create/properties.rs`,
//! `Executor::resolve_standalone_create_property`).
//!
//! Before this fix, `CREATE (:V {p: date({...})})` — no preceding
//! `MATCH`/`UNWIND` row — rejected any function-call property value with
//! "Complex expressions not supported in CREATE properties"
//! (`expression_to_json_value` had no `FunctionCall` arm). The row-aware
//! path (`resolve_property_expr_for_create`, used when a row context
//! exists, e.g. `UNWIND ... CREATE`) already evaluated function calls and
//! canonicalized temporals before storage; standalone `CREATE` now routes
//! through the same evaluator + canonicalization, while a bare
//! `Variable`/`PropertyAccess` reference — structurally unresolvable with
//! no row at all in a standalone CREATE — still errors (TCK
//! `Create1[20]`).

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
/// canonicalization — must never appear anywhere in a serialized response
/// or in what actually reaches storage.
const TEMPORAL_TAG_KEY: &str = "_nexus_temporal_type";

fn assert_no_temporal_tag_leak(value: &serde_json::Value) {
    let serialized = serde_json::to_string(value).expect("value must serialize");
    assert!(
        !serialized.contains(TEMPORAL_TAG_KEY),
        "tagged temporal marker leaked into response: {serialized}"
    );
}

#[test]
fn standalone_create_date_function_call_property_persists_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpDateNode {d: date('2015-07-21')})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpDateNode) RETURN v.d AS d");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("2015-07-21"),
        "expected the stored property to read back as the canonical ISO date string, got: {value:?}"
    );
    assert_no_temporal_tag_leak(value);

    // The tag must not survive anywhere in the persisted property map either.
    let result = execute_query(&mut engine, "MATCH (v:CfcpDateNode) RETURN v AS v");
    let node = get_single_value(&result);
    assert_no_temporal_tag_leak(node);
}

#[test]
fn standalone_create_duration_function_call_property_persists_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpDurationNode {d: duration({days: 1, hours: 2})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpDurationNode) RETURN v.d AS d");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("P1DT2H"),
        "expected the stored property to read back as the canonical ISO duration string, got: {value:?}"
    );
    assert_no_temporal_tag_leak(value);
}

#[test]
fn standalone_create_non_temporal_function_call_property_resolves() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpUpperNode {name: toUpper('nexus')})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpUpperNode) RETURN v.name AS name");
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("NEXUS"));
}

/// A property value referencing a variable with no earlier `MATCH`/`UNWIND`
/// to bind it must still be rejected outright — a standalone `CREATE` has
/// no row scope, so evaluating it against an empty row must never silently
/// degrade to `Null`. Mirrors TCK `Create1[20]`
/// (`CREATE (b {name: missing}) RETURN b`).
#[test]
fn standalone_create_unresolvable_variable_property_still_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let res = engine.execute_cypher("CREATE (:CfcpMissingNode {p: missing})");
    assert!(
        res.is_err(),
        "a bare undefined-variable property value in a standalone CREATE must error, got: {res:?}"
    );
}
