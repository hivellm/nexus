//! `SHOW INDEXES` introspection (phase7 §4.6).
//!
//! Mirrors the existing `SHOW CONSTRAINTS` command: enumerate every
//! registered index in Neo4j-compatible column shape
//! (`name, type, entityType, labelsOrTypes, properties`). Single-property
//! indexes come from the durable catalog record; multi-property indexes come
//! from the composite B-tree registry.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use serde_json::Value;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

const EXPECTED_COLUMNS: [&str; 5] = ["name", "type", "entityType", "labelsOrTypes", "properties"];

#[test]
fn show_indexes_empty_has_stable_columns() {
    let (mut engine, _ctx) = engine();
    let rs = engine
        .execute_cypher("SHOW INDEXES")
        .expect("SHOW INDEXES must parse and execute");
    assert_eq!(rs.columns, EXPECTED_COLUMNS);
    assert!(
        rs.rows.is_empty(),
        "a fresh engine has no user indexes, got {:?}",
        rs.rows
    );
}

#[test]
fn show_indexes_lists_single_property_index() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.name)")
        .expect("create index");

    let rs = engine.execute_cypher("SHOW INDEXES").expect("show indexes");
    assert_eq!(rs.columns, EXPECTED_COLUMNS);
    assert_eq!(rs.rows.len(), 1, "one index expected");

    let row = &rs.rows[0].values;
    assert_eq!(row[1], Value::String("RANGE".to_string()), "type");
    assert_eq!(row[2], Value::String("NODE".to_string()), "entityType");
    assert_eq!(
        row[3],
        Value::Array(vec![Value::String("Person".to_string())]),
        "labelsOrTypes"
    );
    assert_eq!(
        row[4],
        Value::Array(vec![Value::String("name".to_string())]),
        "properties"
    );
}

#[test]
fn show_indexes_lists_composite_index_with_ordered_properties() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE INDEX person_ab FOR (n:Person) ON (n.a, n.b)")
        .expect("create composite index");

    let rs = engine.execute_cypher("SHOW INDEXES").expect("show indexes");
    assert_eq!(rs.rows.len(), 1, "one composite index expected");

    let row = &rs.rows[0].values;
    // User-supplied name is preserved.
    assert_eq!(row[0], Value::String("person_ab".to_string()), "name");
    assert_eq!(
        row[3],
        Value::Array(vec![Value::String("Person".to_string())]),
        "labelsOrTypes"
    );
    // Composite properties keep their declared order.
    assert_eq!(
        row[4],
        Value::Array(vec![
            Value::String("a".to_string()),
            Value::String("b".to_string()),
        ]),
        "properties preserve order"
    );
}

#[test]
fn show_indexes_lists_both_registries_together() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.name)")
        .expect("single");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.a, n.b)")
        .expect("composite");

    let rs = engine.execute_cypher("SHOW INDEXES").expect("show indexes");
    assert_eq!(
        rs.rows.len(),
        2,
        "single-property and composite indexes both listed"
    );
    assert_eq!(rs.columns, EXPECTED_COLUMNS);
}
