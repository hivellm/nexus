//! Regression suite for `phase0_fix-cypher-relationship-delete-noop`.
//!
//! Cypher relationship delete (`MATCH (a)-[r]->(b) DELETE r`) used to be a
//! no-op: the engine's DELETE path only projected/handled NODE variables, so
//! the `r` binding never reached the delete loop and the edge stayed live
//! (`relationships_deleted == 0`, record `is_deleted() == false`, `count(r)`
//! still 1). These tests pin the fix.

use nexus_core::testing::setup_isolated_test_engine;

#[test]
fn delete_relationship_soft_deletes_the_edge() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) DELETE r")
        .unwrap();

    // The edge must be gone from query results...
    let counted = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) RETURN count(r) AS c")
        .unwrap();
    let c = counted.rows[0].values[0]
        .as_i64()
        .expect("count(r) must be numeric");
    assert_eq!(
        c, 0,
        "DELETE r must remove the relationship from query results"
    );

    // ...and marked deleted in the store.
    let total = engine.storage.relationship_count();
    let mut any_live = false;
    for rid in 0..total {
        if let Ok(rel) = engine.storage.read_rel(rid) {
            if !rel.is_deleted() {
                any_live = true;
            }
        }
    }
    assert!(
        !any_live,
        "DELETE r must soft-delete the relationship record in the store"
    );
}

#[test]
fn delete_relationship_makes_endpoint_deletable() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) DELETE r")
        .unwrap();

    // With its only edge gone, a plain (non-DETACH) delete must now succeed.
    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) DELETE a")
        .unwrap();
    let gone = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) RETURN a")
        .unwrap();
    assert_eq!(
        gone.rows.len(),
        0,
        "Alice must be deletable once her edge is deleted"
    );
}

#[test]
fn double_delete_relationship_is_a_clean_noop() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();
    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) DELETE r")
        .unwrap();
    // Second delete matches nothing (edge already gone) — must not error.
    let second = engine.execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) DELETE r");
    assert!(
        second.is_ok(),
        "double DELETE r must be a clean no-op, got {second:?}"
    );
}

#[test]
fn delete_node_and_relationship_in_one_clause() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    // `DELETE r, a` must work regardless of order: the relationship is removed
    // first, so the non-DETACH delete of `a` no longer sees a live edge.
    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'})-[r:KNOWS]->(b) DELETE r, a")
        .unwrap();

    let alice = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) RETURN a")
        .unwrap();
    assert_eq!(alice.rows.len(), 0, "Alice must be deleted");
    let bob = engine
        .execute_cypher("MATCH (b:Person {name: 'Bob'}) RETURN b")
        .unwrap();
    assert_eq!(bob.rows.len(), 1, "Bob was not in DELETE, must survive");
}
