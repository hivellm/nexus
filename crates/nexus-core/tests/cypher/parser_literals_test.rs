//! Extended numeric-literal forms (scientific, hex, octal, underscore
//! separators, leading-dot floats) and string escapes (\uXXXX, \b, \f, \0).

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn val(query: &str) -> Value {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(query)
        .expect("query should succeed")
        .rows
        .first()
        .and_then(|row| row.values.first().cloned())
        .expect("one value")
}

fn f(query: &str) -> f64 {
    val(query).as_f64().expect("float")
}

fn i(query: &str) -> i64 {
    val(query).as_i64().expect("integer")
}

// ── numeric ──────────────────────────────────────────────────────────────────

#[test]
fn scientific_notation() {
    assert!((f("RETURN 1e3 AS v") - 1000.0).abs() < 1e-9);
    assert!((f("RETURN 1.5e-2 AS v") - 0.015).abs() < 1e-9);
    assert!((f("RETURN 2.0E3 AS v") - 2000.0).abs() < 1e-9);
    assert!((f("RETURN 6e0 AS v") - 6.0).abs() < 1e-9);
}

#[test]
fn hex_and_octal_integers() {
    assert_eq!(i("RETURN 0x1F AS v"), 31);
    assert_eq!(i("RETURN 0X10 AS v"), 16);
    assert_eq!(i("RETURN 0o17 AS v"), 15);
    assert_eq!(i("RETURN 0O10 AS v"), 8);
}

#[test]
fn underscore_digit_separators() {
    assert_eq!(i("RETURN 1_000 AS v"), 1000);
    assert_eq!(i("RETURN 1_000_000 AS v"), 1_000_000);
    assert_eq!(i("RETURN 0x_FF AS v"), 255);
    assert!((f("RETURN 1_000.5 AS v") - 1000.5).abs() < 1e-9);
}

#[test]
fn leading_dot_float() {
    assert!((f("RETURN .5 AS v") - 0.5).abs() < 1e-9);
    assert!((f("RETURN .25 AS v") - 0.25).abs() < 1e-9);
}

#[test]
fn range_is_not_misparsed_as_float() {
    // The fractional dot is only consumed when a digit follows, so `1..3`
    // stays a slice range rather than being eaten as `1.` then `.3`.
    let v = val("RETURN [1, 2, 3, 4][1..3] AS v");
    assert_eq!(v, serde_json::json!([2, 3]));
}

// ── string escapes ───────────────────────────────────────────────────────────

#[test]
fn unicode_escape() {
    assert_eq!(val("RETURN 'a\\u0062c' AS v"), Value::String("abc".into()));
    assert_eq!(val("RETURN '\\u00e9' AS v"), Value::String("é".into()));
}

#[test]
fn control_char_escapes() {
    assert_eq!(
        val("RETURN 'x\\by' AS v"),
        Value::String("x\u{0008}y".into())
    );
    assert_eq!(
        val("RETURN 'x\\fy' AS v"),
        Value::String("x\u{000C}y".into())
    );
    assert_eq!(
        val("RETURN 'x\\0y' AS v"),
        Value::String("x\u{0000}y".into())
    );
}

#[test]
fn existing_escapes_still_work() {
    assert_eq!(val("RETURN 'x\\ty' AS v"), Value::String("x\ty".into()));
    assert_eq!(val("RETURN 'x\\ny' AS v"), Value::String("x\ny".into()));
}
