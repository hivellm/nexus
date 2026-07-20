//! Regression suite for `phase8_optional-match-binding-leak`.
//!
//! Before the fix, OPTIONAL MATCH on a node with no matching
//! relationships silently bound the target (and relationship)
//! variables to **the source node's data** instead of NULL. The
//! corruption traced back to `find_relationships`'s scan-fallback
//! at `crates/nexus-core/src/executor/operators/path.rs`: when
//! `first_rel_ptr == 0`, the fallback read rel_id=0 from the
//! memmapped backing file as a zero-byte record (`src=0`,
//! `dst=0`, `type_id=0`); the existing
//! "skip if src=0 && dst=0 && rel_id > 0" filter let rel_id=0
//! through; the direction check accepted
//! `check_src_id (0) == node_id (0)` as a match for the very
//! first node (Alice in the canonical reproducer); and the
//! operator emitted a phantom relationship pointing back at the
//! source. Aggregations + `IS NULL` predicates inherited the
//! corruption.
//!
//! The fix lives in `path.rs`:
//!   1. Short-circuit when `relationship_count() == 0`.
//!   2. Clamp the scan upper bound to `relationship_count() - 1`.
//!   3. Strengthen the uninitialized-record skip filter to drop
//!      the `rel_id > 0` qualifier and key off `type_id == 0`
//!      instead.
//!
//! Each scenario below is the canonical-shape repro the
//! `phase8_optional-match-binding-leak` proposal called out.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn first_row(engine: &mut nexus_core::Engine, cypher: &str) -> Vec<Value> {
    let r = engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("cypher `{cypher}` failed: {e}"));
    assert_eq!(
        r.rows.len(),
        1,
        "expected 1 row from `{cypher}`, got {}",
        r.rows.len()
    );
    r.rows.into_iter().next().unwrap().values
}

fn setup_alice() -> nexus_core::Engine {
    let (mut engine, ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {name: 'Alice'})")
        .unwrap();
    // Drop ctx after engine setup; engine owns the tempdir guard.
    std::mem::forget(ctx);
    engine
}

#[test]
fn optional_match_no_match_binds_target_to_null() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) \
         RETURN a.name AS aname, b AS b_raw",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(
        row[1],
        Value::Null,
        "OPTIONAL MATCH no-match path must bind target var to NULL, got {:?}",
        row[1]
    );
}

#[test]
fn optional_match_no_match_property_access_returns_null() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) \
         RETURN a.name AS aname, b.name AS friend",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(
        row[1],
        Value::Null,
        "b.name must be NULL when b is unbound, got {:?}",
        row[1]
    );
}

#[test]
fn optional_match_no_match_is_null_predicate_true() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) \
         RETURN a.name AS aname, b IS NULL AS b_null",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(
        row[1],
        Value::Bool(true),
        "`b IS NULL` must be true on the no-match path, got {:?}",
        row[1]
    );
}

#[test]
fn optional_match_no_match_count_is_zero() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[:KNOWS]->(b) \
         RETURN count(b) AS friends",
    );
    let n = row[0].as_i64().expect("count(b) numeric");
    assert_eq!(
        n, 0,
        "count(b) must be 0 when b is unbound across the only row"
    );
}

#[test]
fn optional_match_no_match_relationship_var_is_null() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[r:KNOWS]->(b) \
         RETURN a.name AS aname, r AS r_raw, b AS b_raw",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(row[1], Value::Null, "rel var leaked: {:?}", row[1]);
    assert_eq!(row[2], Value::Null, "target var leaked: {:?}", row[2]);
}

#[test]
fn optional_match_no_match_anonymous_target_keeps_rel_null() {
    let mut engine = setup_alice();
    let row = first_row(
        &mut engine,
        "MATCH (a:Person) OPTIONAL MATCH (a)-[r:KNOWS]->() \
         RETURN a.name AS aname, r AS r_raw, r IS NULL AS r_null",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(row[1], Value::Null);
    assert_eq!(row[2], Value::Bool(true));
}

#[test]
fn optional_match_with_real_match_still_returns_target() {
    // Sanity: when a real :KNOWS edge exists, OPTIONAL MATCH
    // continues to bind the target — the fix must not regress
    // the happy path.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'}), (b:Person {name: 'Bob'}), (a)-[:KNOWS]->(b)",
        )
        .unwrap();

    let row = first_row(
        &mut engine,
        "MATCH (a:Person {name: 'Alice'}) OPTIONAL MATCH (a)-[:KNOWS]->(b) \
         RETURN a.name AS aname, b.name AS friend",
    );
    assert_eq!(row[0], Value::String("Alice".into()));
    assert_eq!(
        row[1],
        Value::String("Bob".into()),
        "OPTIONAL MATCH happy-path regression — bob's name must come through"
    );
}
