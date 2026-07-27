//! Tests for newly implemented functions: sin, cos, tan, reduce, extract, all, any, none, single, toDate

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

// ============================================================================
// TRIGONOMETRIC FUNCTIONS: sin, cos, tan
// ============================================================================

#[test]
fn test_sin_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // sin(0) = 0
    let result = execute_query(&mut engine, "RETURN sin(0) AS s");
    assert!((get_single_value(&result).as_f64().unwrap() - 0.0).abs() < 0.0001);

    // sin(PI/2) ≈ 1
    let result = execute_query(&mut engine, "RETURN sin(1.57079632679) AS s");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.0).abs() < 0.0001);

    // sin(PI) ≈ 0
    let result = execute_query(&mut engine, "RETURN sin(3.14159265359) AS s");
    assert!((get_single_value(&result).as_f64().unwrap() - 0.0).abs() < 0.0001);
}

#[test]
fn test_cos_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // cos(0) = 1
    let result = execute_query(&mut engine, "RETURN cos(0) AS c");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.0).abs() < 0.0001);

    // cos(PI/2) ≈ 0
    let result = execute_query(&mut engine, "RETURN cos(1.57079632679) AS c");
    assert!((get_single_value(&result).as_f64().unwrap() - 0.0).abs() < 0.0001);

    // cos(PI) ≈ -1
    let result = execute_query(&mut engine, "RETURN cos(3.14159265359) AS c");
    assert!((get_single_value(&result).as_f64().unwrap() - (-1.0)).abs() < 0.0001);
}

#[test]
fn test_tan_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // tan(0) = 0
    let result = execute_query(&mut engine, "RETURN tan(0) AS t");
    assert!((get_single_value(&result).as_f64().unwrap() - 0.0).abs() < 0.0001);

    // tan(PI/4) ≈ 1
    let result = execute_query(&mut engine, "RETURN tan(0.78539816339) AS t");
    assert!((get_single_value(&result).as_f64().unwrap() - 1.0).abs() < 0.1);
}

#[test]
fn test_trigonometric_functions_with_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // NULL input should return NULL
    let result = execute_query(&mut engine, "RETURN sin(null) AS s");
    assert!(get_single_value(&result).is_null());

    let result = execute_query(&mut engine, "RETURN cos(null) AS c");
    assert!(get_single_value(&result).is_null());

    let result = execute_query(&mut engine, "RETURN tan(null) AS t");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// LIST FUNCTIONS: reduce, extract
// ============================================================================

// Note: reduce, extract, all, any, none, single functions require list comprehensions
// which need parser support. These functions are implemented in the executor but
// need proper Cypher syntax support. Tests will be added when parser supports them.
// For now, we verify the functions exist and handle NULL correctly.

#[test]
fn test_reduce_function_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // reduce with NULL should return NULL
    let result = execute_query(&mut engine, "RETURN reduce(null, null, null, null) AS r");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_extract_function_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // extract with NULL should return NULL
    let result = execute_query(&mut engine, "RETURN extract(null, null, null) AS e");
    assert!(get_single_value(&result).is_null());
}

// ============================================================================
// PREDICATE FUNCTIONS: all, any, none, single
// ============================================================================

// NOTE (phase21_tck-quantifier-in-where-parser): the four `test_*_function_null`
// tests were REMOVED. They exercised a non-standard variadic form
// `any/all/none/single(null, null, null)` that openCypher does not define —
// these are strictly list-predicate quantifiers, `any/all/none/single(x IN
// list WHERE predicate)`. Supporting the correct quantifier grammar is
// incompatible with the invented variadic form (the parser now requires
// `<var> IN <list> WHERE <pred>`), so the old tests encoded wrong behavior and
// were dropped. Quantifier coverage now lives in
// tests/cypher/test_quantifier_predicates.rs.

// ============================================================================
// TYPE CONVERSION: toDate
// ============================================================================

#[test]
fn test_todate_function() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // toDate('2025-11-12') should return '2025-11-12'
    let result = execute_query(&mut engine, "RETURN toDate('2025-11-12') AS d");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "2025-11-12");

    // toDate with datetime string
    let result = execute_query(&mut engine, "RETURN toDate('2025-11-12T10:30:00Z') AS d");
    assert_eq!(get_single_value(&result).as_str().unwrap(), "2025-11-12");

    // toDate with object {year, month, day}
    let result = execute_query(
        &mut engine,
        "RETURN toDate({year: 2025, month: 11, day: 12}) AS d",
    );
    assert_eq!(get_single_value(&result).as_str().unwrap(), "2025-11-12");
}

#[test]
fn test_todate_with_null() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // NULL input should return NULL
    let result = execute_query(&mut engine, "RETURN toDate(null) AS d");
    assert!(get_single_value(&result).is_null());

    // Invalid string should return NULL
    let result = execute_query(&mut engine, "RETURN toDate('invalid-date') AS d");
    assert!(get_single_value(&result).is_null());
}

#[test]
fn test_todate_in_queries() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (e:Event {name: 'Meeting', date: '2025-11-12T14:00:00Z'})",
    );
    engine.refresh_executor().unwrap();

    // Extract date from datetime string
    let result = execute_query(
        &mut engine,
        "MATCH (e:Event) RETURN e.name, toDate(e.date) AS event_date",
    );
    assert_eq!(result.rows.len(), 1, "Should have exactly 1 event");
    assert_eq!(result.rows[0].values[0].as_str().unwrap(), "Meeting");
    assert_eq!(result.rows[0].values[1].as_str().unwrap(), "2025-11-12");
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_trigonometric_in_expressions() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(&mut engine, "CREATE (p:Point {angle: 1.57079632679})");
    engine.refresh_executor().unwrap();

    // Use sin in RETURN
    let result = execute_query(&mut engine, "MATCH (p:Point) RETURN sin(p.angle) AS y");
    assert_eq!(result.rows.len(), 1, "Should have exactly 1 point");
    assert!((result.rows[0].values[0].as_f64().unwrap() - 1.0).abs() < 0.0001);

    // Use cos in RETURN
    let result = execute_query(&mut engine, "MATCH (p:Point) RETURN cos(p.angle) AS x");
    assert!((result.rows[0].values[0].as_f64().unwrap() - 0.0).abs() < 0.0001);
}

#[test]
fn test_functions_combined() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // Test multiple functions in one query
    let result = execute_query(
        &mut engine,
        "RETURN sin(0) AS s, cos(0) AS c, toDate('2025-11-12') AS d",
    );
    assert_eq!(result.rows.len(), 1);
    assert!((result.rows[0].values[0].as_f64().unwrap() - 0.0).abs() < 0.0001);
    assert!((result.rows[0].values[1].as_f64().unwrap() - 1.0).abs() < 0.0001);
    assert_eq!(result.rows[0].values[2].as_str().unwrap(), "2025-11-12");
}

// ============================================================================
// phase21_tck-missing-functions: reverse(string), range(step=0) error,
// sign/cot/haversin/rand, properties()
// ============================================================================

#[test]
fn reverse_string_reverses_characters() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = execute_query(&mut engine, "RETURN reverse('abc') AS r");
    assert_eq!(get_single_value(&r).as_str(), Some("cba"));
}

#[test]
fn range_with_zero_step_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert!(
        engine.execute_cypher("RETURN range(1, 5, 0) AS r").is_err(),
        "range() with step 0 must error"
    );
}

#[test]
fn sign_returns_minus_one_zero_one() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    assert_eq!(
        get_single_value(&execute_query(&mut engine, "RETURN sign(-3.0) AS r")).as_i64(),
        Some(-1)
    );
    assert_eq!(
        get_single_value(&execute_query(&mut engine, "RETURN sign(0) AS r")).as_i64(),
        Some(0)
    );
    assert_eq!(
        get_single_value(&execute_query(&mut engine, "RETURN sign(2.5) AS r")).as_i64(),
        Some(1)
    );
}

#[test]
fn cot_and_haversin() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let cot = get_single_value(&execute_query(&mut engine, "RETURN cot(1.0) AS r"))
        .as_f64()
        .unwrap();
    assert!((cot - (1.0_f64 / 1.0_f64.tan())).abs() < 1e-9);
    let hav = get_single_value(&execute_query(&mut engine, "RETURN haversin(0.0) AS r"))
        .as_f64()
        .unwrap();
    assert!(hav.abs() < 1e-9, "haversin(0) should be 0");
}

#[test]
fn rand_is_in_unit_interval() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let v = get_single_value(&execute_query(&mut engine, "RETURN rand() AS r"))
        .as_f64()
        .unwrap();
    assert!((0.0..1.0).contains(&v), "rand() must be in [0,1), got {v}");
}

#[test]
fn properties_of_map_and_node() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // literal map
    let m = execute_query(&mut engine, "RETURN properties({a: 1, b: 2}) AS r");
    let obj = get_single_value(&m).as_object().unwrap();
    assert_eq!(obj.get("a").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(obj.get("b").and_then(|v| v.as_i64()), Some(2));
    // node: internal markers stripped
    engine
        .create_node(
            vec!["P".to_string()],
            serde_json::json!({ "name": "Alice" }),
        )
        .unwrap();
    let n = execute_query(&mut engine, "MATCH (p:P) RETURN properties(p) AS r");
    let nobj = get_single_value(&n).as_object().unwrap();
    assert_eq!(nobj.get("name").and_then(|v| v.as_str()), Some("Alice"));
    assert!(
        !nobj.keys().any(|k| k.starts_with('_')),
        "internal markers must be stripped: {nobj:?}"
    );
}
