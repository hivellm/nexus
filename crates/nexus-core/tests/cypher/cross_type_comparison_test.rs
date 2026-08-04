//! Comparing values of different types must yield `null` for the ORDERING
//! operators and `false` for equality — not a confident wrong answer.
//!
//! All four ordering operators were implemented as
//! `compare_values_for_sort(l, r) == Ordering::Less` (etc.), and that
//! comparator's last arm stringifies both operands: `1 < 'text'` compared `"1"`
//! against `"text"` and answered `true`. Equality had its own coercion, parsing
//! a string as a number, so `1.0 = '1.0'` was `true`. Both silently corrupt
//! ordinary `WHERE` filters over heterogeneous properties.

use nexus_core::testing::setup_isolated_test_engine;

fn value(query: &str) -> serde_json::Value {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(query)
        .expect("query should execute")
        .rows
        .first()
        .and_then(|r| r.values.first().cloned())
        .expect("one value")
}

/// openCypher TCK `expressions/comparison/Comparison2.feature` [3] "Comparing
/// across types yields null, except numbers" and [6] "Comparability between
/// numbers and strings".
#[test]
fn ordering_across_types_is_null() {
    for query in [
        "RETURN 1 < 'text' AS r",
        "RETURN 1 > 'text' AS r",
        "RETURN 1 <= 'text' AS r",
        "RETURN 1 >= 'text' AS r",
        "RETURN '1.0' < 1.0 AS r",
        "RETURN true < 1 AS r",
        "RETURN true > 'a' AS r",
        "RETURN [1] < 1 AS r",
        "RETURN {k: 1} < 1 AS r",
        "RETURN [1] < 'a' AS r",
    ] {
        assert_eq!(
            value(query),
            serde_json::Value::Null,
            "{query} must be null, not a verdict from a stringified comparison"
        );
    }
}

#[test]
fn ordering_within_a_type_still_works() {
    // Control: the gate must not swallow the defined cases.
    assert_eq!(value("RETURN 1 < 3.14 AS r"), serde_json::json!(true));
    assert_eq!(value("RETURN 3.14 <= 1 AS r"), serde_json::json!(false));
    assert_eq!(value("RETURN 'a' < 'b' AS r"), serde_json::json!(true));
    assert_eq!(value("RETURN false < true AS r"), serde_json::json!(true));
    assert_eq!(value("RETURN [1] < [2] AS r"), serde_json::json!(true));
}

/// INTEGER and FLOAT are ONE numeric kind for comparison, so a mixed pair is
/// defined rather than null — the exception `Comparison2` [3] names in its title.
#[test]
fn integer_and_float_are_one_comparable_kind() {
    assert_eq!(value("RETURN 1 < 3.14 AS r"), serde_json::json!(true));
    assert_eq!(value("RETURN 1 = 1.0 AS r"), serde_json::json!(true));
}

/// openCypher TCK `expressions/comparison/Comparison1.feature` [9] "Equality
/// between strings and numbers": equality across types is `false`, NOT `null`.
#[test]
fn equality_across_types_is_false_not_null() {
    assert_eq!(value("RETURN 1.0 = '1.0' AS r"), serde_json::json!(false));
    assert_eq!(value("RETURN '1' = 1 AS r"), serde_json::json!(false));
    assert_eq!(value("RETURN 1 = true AS r"), serde_json::json!(false));
    assert_eq!(value("RETURN 1 = [1] AS r"), serde_json::json!(false));
    // …and inequality is its exact negation.
    assert_eq!(value("RETURN 1.0 <> '1.0' AS r"), serde_json::json!(true));
}

/// `=` and `<>` must be exact negations of each other. They were not: `=` used
/// a numeric-aware helper while `<>` used a raw structural compare, so `1` and
/// `1.0` were reported as BOTH equal and not-equal.
#[test]
fn equality_and_inequality_are_exact_negations() {
    assert_eq!(value("RETURN 1 = 1.0 AS r"), serde_json::json!(true));
    assert_eq!(
        value("RETURN 1 <> 1.0 AS r"),
        serde_json::json!(false),
        "1 <> 1.0 must be false when 1 = 1.0 is true"
    );
}

/// A `null` operand keeps yielding `null` for every operator — the type gate
/// must not turn that into `false`.
#[test]
fn a_null_operand_still_yields_null() {
    for query in [
        "RETURN null < 1 AS r",
        "RETURN 1 < null AS r",
        "RETURN null = 1 AS r",
        "RETURN null <> null AS r",
    ] {
        assert_eq!(value(query), serde_json::Value::Null, "{query}");
    }
}

/// Regression guard for the fix's own first attempt: a temporal value is a
/// tagged object in flight and a canonical ISO string once stored, and the
/// comparator canonicalizes both operands before comparing. The type
/// classification has to do the same, or `duration('PT10H') < <stored>` reads as
/// Map-vs-String and returns `null` where components should be compared.
#[test]
fn durations_still_compare_across_their_two_representations() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:D {d: duration({hours: 9})})")
        .expect("create");
    engine.refresh_executor().unwrap();

    let rs = engine
        .execute_cypher(
            "MATCH (n:D) RETURN duration({hours: 10}) < n.d AS lt, duration({hours: 10}) > n.d AS gt",
        )
        .expect("query");
    assert_eq!(
        rs.rows[0].values[0],
        serde_json::json!(false),
        "PT10H < PT9H"
    );
    assert_eq!(
        rs.rows[0].values[1],
        serde_json::json!(true),
        "PT10H > PT9H"
    );
}

/// An inline property match is equality, so it no longer coerces either: a
/// string pattern value stops matching a numeric property.
#[test]
fn an_inline_property_match_does_not_coerce_a_string_to_a_number() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:C {id: 1})")
        .expect("create");
    engine.refresh_executor().unwrap();

    let numeric = engine
        .execute_cypher("MATCH (n:C {id: 1}) RETURN count(*)")
        .expect("numeric match");
    assert_eq!(numeric.rows[0].values[0].as_i64(), Some(1));

    let stringly = engine
        .execute_cypher("MATCH (n:C {id: '1'}) RETURN count(*)")
        .expect("string match");
    assert_eq!(
        stringly.rows[0].values[0].as_i64(),
        Some(0),
        "'1' must not match the number 1"
    );
}
