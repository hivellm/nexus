//! Operator precedence & associativity: `^` is right-associative and binds
//! tighter than unary/`*`; `%` is the truncated remainder; comparisons chain
//! (`a < b < c` ≡ `a < b AND b < c`).

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
    val(query).as_f64().expect("number")
}

// ── ^ right-associative, tighter than unary and * ────────────────────────────

#[test]
fn power_is_right_associative() {
    assert!((f("RETURN 2^3^2 AS v") - 512.0).abs() < 1e-9); // 2^(3^2) = 2^9
    assert!((f("RETURN 2^2^3 AS v") - 256.0).abs() < 1e-9); // 2^(2^3) = 2^8
}

#[test]
fn power_binds_tighter_than_unary_minus() {
    assert!((f("RETURN -2^2 AS v") - (-4.0)).abs() < 1e-9); // -(2^2)
}

#[test]
fn power_binds_tighter_than_multiplication() {
    assert!((f("RETURN 2 * 3 ^ 2 AS v") - 18.0).abs() < 1e-9); // 2*(3^2)
}

#[test]
fn power_allows_signed_exponent() {
    assert!((f("RETURN 2^-2 AS v") - 0.25).abs() < 1e-9); // 2^(-2)
}

// ── % truncated remainder (sign follows dividend) ────────────────────────────

#[test]
fn modulo_is_truncated_not_euclidean() {
    assert!((f("RETURN -3 % 2 AS v") - (-1.0)).abs() < 1e-9); // not +1
    assert!((f("RETURN 3 % -2 AS v") - 1.0).abs() < 1e-9);
    assert!((f("RETURN 7 % 3 AS v") - 1.0).abs() < 1e-9);
}

// ── comparison chaining ──────────────────────────────────────────────────────

#[test]
fn comparison_chaining_matches_and_desugaring() {
    assert_eq!(val("RETURN 1 < 2 < 3 AS v"), Value::Bool(true));
    assert_eq!(val("RETURN 3 < 2 < 1 AS v"), Value::Bool(false));
    // Discriminates chaining from left-assoc bool comparison:
    // 1 < 3 < 2  ≡  (1<3) AND (3<2)  = true AND false = false.
    // A left-assoc `(1<3)<2` would instead be `true < 2` ≠ false here.
    assert_eq!(val("RETURN 1 < 3 < 2 AS v"), Value::Bool(false));
    // Extends past two links: 1 <= 2 <= 2 <= 3 all hold.
    assert_eq!(val("RETURN 1 <= 2 <= 2 <= 3 AS v"), Value::Bool(true));
}
