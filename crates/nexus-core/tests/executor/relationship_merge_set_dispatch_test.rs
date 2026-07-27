//! Regression for `phase0_fix-relationship-write-clauses-dropped` (H-2): a
//! relationship-pattern `MERGE ... ON CREATE/ON MATCH SET ...` must dispatch
//! every SET item to the correct entity — a node property, a node label, or a
//! map-merge on the node — instead of filtering the whole SET down to only
//! items whose target is the relationship variable. Historically the
//! node-targeted items were silently dropped.

use nexus_core::testing::setup_isolated_test_engine;

fn scalar(engine: &mut nexus_core::Engine, q: &str) -> serde_json::Value {
    let r = engine.execute_cypher(q).expect("query must succeed");
    assert_eq!(
        r.rows.len(),
        1,
        "expected one row for `{q}`; got {:?}",
        r.rows
    );
    r.rows[0].values[0].clone()
}

#[test]
fn merge_rel_on_create_sets_both_node_and_rel_properties() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "MERGE (a:A {k: 1})-[r:KNOWS]->(b:B {k: 2}) \
             ON CREATE SET a.createdAt = 1, r.since = 2",
        )
        .unwrap();

    assert_eq!(
        scalar(&mut engine, "MATCH (a:A {k: 1}) RETURN a.createdAt AS v").as_i64(),
        Some(1),
        "ON CREATE SET on the NODE variable must apply, not be dropped"
    );
    assert_eq!(
        scalar(
            &mut engine,
            "MATCH (a:A)-[r:KNOWS]->(b:B) RETURN r.since AS v"
        )
        .as_i64(),
        Some(2),
        "ON CREATE SET on the relationship variable must apply too"
    );
}

#[test]
fn merge_rel_on_create_sets_node_label() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("MERGE (a:A {k: 1})-[r:KNOWS]->(b:B {k: 2}) ON CREATE SET a:Extra")
        .unwrap();

    assert_eq!(
        scalar(&mut engine, "MATCH (a:Extra) RETURN count(a) AS c").as_i64(),
        Some(1),
        "ON CREATE SET a:Extra must add the label to the node"
    );
}

#[test]
fn merge_rel_on_create_map_merges_node() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("MERGE (a:A {k: 1})-[r:KNOWS]->(b:B {k: 2}) ON CREATE SET a += {x: 9}")
        .unwrap();

    assert_eq!(
        scalar(&mut engine, "MATCH (a:A {k: 1}) RETURN a.x AS v").as_i64(),
        Some(9),
        "ON CREATE SET a += {{...}} must map-merge into the node's properties"
    );
}

#[test]
fn merge_rel_on_match_sets_node_property() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    // First MERGE creates the pattern.
    engine
        .execute_cypher("MERGE (a:A {k: 1})-[r:KNOWS]->(b:B {k: 2})")
        .unwrap();

    // Second MERGE matches it, so ON MATCH fires.
    engine
        .execute_cypher("MERGE (a:A {k: 1})-[r:KNOWS]->(b:B {k: 2}) ON MATCH SET a.updatedAt = 7")
        .unwrap();

    assert_eq!(
        scalar(&mut engine, "MATCH (a:A {k: 1}) RETURN a.updatedAt AS v").as_i64(),
        Some(7),
        "ON MATCH SET on the NODE variable must apply, not be dropped"
    );
}
