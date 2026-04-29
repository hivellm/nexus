//! Regression suite for `phase8_optional-match-empty-driver`.
//!
//! Before the fix, a top-level OPTIONAL MATCH against an empty
//! label scan produced **zero rows** instead of one row with the
//! optional variables bound to NULL. Concrete repro:
//!
//! ```cypher
//! OPTIONAL MATCH (n:Ghost) RETURN n
//! ```
//!
//! Neo4j returns one row with `n = null`; Nexus before this fix
//! returned zero rows because the planner emitted a regular
//! `NodeByLabel + Project` pipeline that produces nothing when the
//! label is empty. OPTIONAL MATCH is a LEFT OUTER JOIN against an
//! implicit single-row driver when there is no prior MATCH clause.
//!
//! The fix injects an `ImplicitSingleRow` source operator before
//! the OPTIONAL pattern and tees the OPTIONAL scan into a
//! `LeftOuterJoin`-shaped projection: empty scan + non-empty driver
//! ⇒ one row with NULL bindings.
//!
//! Note on test infrastructure: each test owns an isolated engine
//! via `setup_isolated_test_engine`. The Alice row is seeded only
//! when the test needs to assert the post-MATCH-then-OPTIONAL path
//! (Section "regression: prior MATCH eliminates rows" below).

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn empty_engine() -> nexus_core::Engine {
    let (engine, ctx) = setup_isolated_test_engine().unwrap();
    std::mem::forget(ctx);
    engine
}

fn engine_with_alice() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {name: 'Alice'})")
        .unwrap();
    std::mem::forget(ctx);
    engine
}

#[test]
fn standalone_optional_on_empty_label_returns_one_null_row() {
    let mut engine = empty_engine();
    let r = engine
        .execute_cypher("OPTIONAL MATCH (n:Ghost) RETURN n")
        .unwrap_or_else(|e| panic!("execute_cypher failed: {e}"));
    assert_eq!(
        r.rows.len(),
        1,
        "expected 1 row with n=null, got {}",
        r.rows.len()
    );
    let n = &r.rows[0].values[0];
    assert!(matches!(n, Value::Null), "expected n to be null, got {n:?}");
}

#[test]
fn standalone_optional_property_access_returns_null() {
    let mut engine = empty_engine();
    let r = engine
        .execute_cypher("OPTIONAL MATCH (n:Ghost) RETURN n.name AS name")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert!(matches!(r.rows[0].values[0], Value::Null));
}

#[test]
fn standalone_optional_count_returns_zero() {
    // count(n) on an empty OPTIONAL MATCH must return one row with
    // c = 0. This already passed before the fix (count over zero
    // rows returns 0 by Cypher convention) — pinned here to make
    // sure the implicit-driver injection does not double-count.
    let mut engine = empty_engine();
    let r = engine
        .execute_cypher("OPTIONAL MATCH (n:Ghost) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    let c = &r.rows[0].values[0];
    let as_num = c
        .as_i64()
        .or_else(|| c.as_u64().map(|u| u as i64))
        .or_else(|| c.as_f64().map(|f| f as i64));
    assert_eq!(as_num, Some(0), "expected count = 0, got {c:?}");
}

#[test]
fn regression_prior_match_eliminates_rows_optional_after_does_not_resurrect() {
    // OPTIONAL MATCH after a real MATCH that produced zero rows
    // must NOT re-introduce wrapped-NULL rows. The implicit-driver
    // injection only applies when there is no prior driver.
    let mut engine = empty_engine(); // no Alice — Person is empty
    let r = engine
        .execute_cypher("MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN a, b")
        .unwrap();
    assert_eq!(
        r.rows.len(),
        0,
        "MATCH eliminated all rows; OPTIONAL MATCH must not resurrect them, got {} rows",
        r.rows.len()
    );
}

#[test]
fn standalone_optional_on_existing_label_returns_actual_rows() {
    // When the labelled set is non-empty, OPTIONAL MATCH must
    // behave like a regular MATCH and return the matching nodes —
    // NOT a single NULL row. The implicit-driver injection only
    // fires when the scan would otherwise produce zero rows.
    let mut engine = engine_with_alice();
    let r = engine
        .execute_cypher("OPTIONAL MATCH (n:Person) RETURN n.name AS name")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(
        r.rows[0].values[0].as_str(),
        Some("Alice"),
        "expected the actual matched row, got {:?}",
        r.rows[0].values[0]
    );
}

#[test]
fn regression_prior_match_with_rows_optional_no_match_binds_b_to_null() {
    // Sanity-check the existing `phase8_optional-match-binding-leak`
    // contract still holds: prior MATCH yields rows, OPTIONAL MATCH
    // finds no relationships, target variable is NULL.
    let mut engine = engine_with_alice();
    let r = engine
        .execute_cypher("MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) RETURN a.name, b")
        .unwrap();
    assert_eq!(r.rows.len(), 1);
    assert_eq!(r.rows[0].values[0].as_str(), Some("Alice"));
    assert!(
        matches!(r.rows[0].values[1], Value::Null),
        "expected b=null, got {:?}",
        r.rows[0].values[1]
    );
}
