//! `IS NULL` / `IS NOT NULL` bind LOOSER than a comparison operator, so they
//! apply to the whole comparison: `false = true IS NULL` is
//! `(false = true) IS NULL`.
//!
//! The check used to run BEFORE the comparison operators, against the left
//! operand alone — `false` was tested for `IS`, `= true` was then consumed as a
//! comparison, and the trailing `IS NULL` was left unread. The projection-list
//! parser silently truncated the query there, so the whole item (and every item
//! after it) disappeared with the query still reported as successful.

use nexus_core::testing::setup_isolated_test_engine;

fn row(query: &str) -> Vec<serde_json::Value> {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = engine.execute_cypher(query).expect("query should execute");
    assert_eq!(rs.rows.len(), 1, "expected one row from {query}");
    rs.rows[0].values.clone()
}

/// openCypher TCK `expressions/precedence`: the three spellings are distinct,
/// and the unparenthesised one must agree with the `(comparison) IS NULL`
/// reading, not with `comparison (IS NULL)`.
#[test]
fn is_null_binds_looser_than_comparison() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let rs = engine
        .execute_cypher(
            "RETURN false = true IS NULL AS a, \
             false = (true IS NULL) AS b, \
             (false = true) IS NULL AS c",
        )
        .expect("all three items must parse");

    assert_eq!(
        rs.columns,
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        "no item may be truncated away"
    );
    let v = &rs.rows[0].values;
    assert_eq!(v[0], serde_json::json!(false), "a = (false = true) IS NULL");
    assert_eq!(v[1], serde_json::json!(true), "b = false = (true IS NULL)");
    assert_eq!(v[2], serde_json::json!(false), "c is a's explicit form");
    assert_eq!(v[0], v[2], "the unparenthesised form must equal c, not b");
}

#[test]
fn is_null_still_works_on_a_bare_operand() {
    // Control: the plain form the old early check handled.
    assert_eq!(row("RETURN null IS NULL AS a")[0], serde_json::json!(true));
    assert_eq!(row("RETURN 1 IS NOT NULL AS a")[0], serde_json::json!(true));
    assert_eq!(row("RETURN 1 IS NULL AS a")[0], serde_json::json!(false));
}

#[test]
fn is_not_null_applies_to_the_whole_comparison_too() {
    assert_eq!(
        row("RETURN 1 = 1 IS NOT NULL AS a")[0],
        serde_json::json!(true)
    );
}

#[test]
fn a_trailing_is_without_null_is_an_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("RETURN 1 = 1 IS AS a")
        .expect_err("IS must be followed by NULL");
    assert!(
        err.to_string().contains("Expected NULL after IS"),
        "got: {err}"
    );
}

/// Comparison chaining was already implemented (`a < b < c` desugars to
/// `a < b AND b < c`); pinned here because it shares this precedence level and
/// the `IS NULL` move must not disturb it.
#[test]
fn comparison_chaining_is_unaffected() {
    assert_eq!(
        row("RETURN 1 < 2 < 3 AS chained")[0],
        serde_json::json!(true)
    );
    assert_eq!(
        row("RETURN 1 < 2 < 1 AS chained")[0],
        serde_json::json!(false)
    );
}
