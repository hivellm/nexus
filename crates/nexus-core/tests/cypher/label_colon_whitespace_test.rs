//! Integration coverage for whitespace between a pattern's label/type
//! colon and the following identifier — `(dur2: Duration2)`,
//! `-[r: KNOWS]->`, `(v : A : B)`. openCypher permits `SP?` around the
//! colon in this grammar position; the TCK corpus relies on it. Controls
//! confirm the fix is scoped to the pattern-label colon and does not leak
//! into map-literal or `WHERE`-predicate colon parsing.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

fn first_value(engine: &mut nexus_core::Engine, query: &str) -> Value {
    engine
        .execute_cypher(query)
        .expect("query should parse and execute")
        .rows
        .first()
        .and_then(|row| row.values.first().cloned())
        .expect("expected one row with one value")
}

#[test]
fn node_label_with_whitespace_after_colon_creates_and_matches() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (dur2: Duration2 {name: 'x'})")
        .expect("CREATE with spaced label should parse and execute");

    let name = first_value(
        &mut engine,
        "MATCH (dur2: Duration2) RETURN dur2.name AS name",
    );
    assert_eq!(name, Value::String("x".to_string()));
}

#[test]
fn relationship_type_with_whitespace_after_colon() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:A)-[r: KNOWS]->(b:B)")
        .expect("CREATE with spaced relationship type should parse and execute");

    let rel_type = first_value(
        &mut engine,
        "MATCH (a:A)-[r: KNOWS]->(b:B) RETURN type(r) AS t",
    );
    assert_eq!(rel_type, Value::String("KNOWS".to_string()));
}

#[test]
fn multi_label_with_whitespace_around_both_colons() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (v : A : B)")
        .expect("CREATE with spaced multi-label should parse and execute");

    let count = first_value(&mut engine, "MATCH (v : A : B) RETURN count(v) AS c");
    assert_eq!(count, Value::Number(1.into()));
}

#[test]
fn map_literal_colon_whitespace_is_unaffected() {
    // Control: `{k: v}` colon-space is a different grammar position
    // (`parse_property_map`) from the pattern label colon fix and must
    // keep parsing exactly as before.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let value = first_value(&mut engine, "RETURN {k: 1, j: 2} AS m");
    assert_eq!(value, serde_json::json!({"k": 1, "j": 2}));
}

#[test]
fn where_label_predicate_is_unaffected() {
    // Control: `WHERE n:Label` is an expression-level label predicate,
    // parsed by a separate code path (`identifier.rs`) that already
    // skipped whitespace correctly before this fix.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (n:Person)")
        .expect("CREATE should succeed");

    let count = first_value(&mut engine, "MATCH (n) WHERE n:Person RETURN count(n) AS c");
    assert_eq!(count, Value::Number(1.into()));
}
