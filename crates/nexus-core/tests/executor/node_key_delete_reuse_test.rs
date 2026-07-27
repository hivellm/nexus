//! Regression suite for `phase0_fix-delete-path-index-cleanup` (H-1).
//!
//! `Engine::delete_node` marked a node as soft-deleted but never evicted its
//! tuple from the composite B-tree backing NODE KEY / composite-index
//! enforcement. Since node ids are never recycled, and the NODE KEY
//! uniqueness check (`Engine::enforce_node_constraints`, composite B-tree
//! `seek_exact`) does not filter out soft-deleted rows, the leftover tuple
//! permanently and falsely rejected re-creating the exact same
//! `(tenantId, id)` pair after the original node was deleted.

use nexus_core::index::PropertyValue;
use nexus_core::testing::setup_isolated_test_engine;

/// Plain (non-relationship-bearing) node: DETACH DELETE then MERGE the
/// identical NODE KEY tuple back must succeed instead of raising a false
/// `ERR_CONSTRAINT_VIOLATED` / `NODE_KEY` violation.
#[test]
fn node_key_tuple_reusable_after_detach_delete() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 1})")
        .expect("initial tuple must populate the composite B-tree");

    engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 1}) DETACH DELETE p")
        .expect("DETACH DELETE of the tuple-bearing node must succeed");

    engine
        .execute_cypher("MERGE (p2:Person {tenantId: 't1', id: 1})")
        .expect(
            "re-creating the exact same NODE KEY tuple after DETACH DELETE must succeed \
             — a leftover composite B-tree entry would falsely reject it as a duplicate",
        );
}

/// Same scenario, but the deleted node carries a live relationship at the
/// time of DETACH DELETE — confirms the composite B-tree eviction runs on
/// the same `delete_node` code path DETACH DELETE uses to clear
/// relationships first.
#[test]
fn node_key_tuple_reusable_after_detach_delete_of_relationship_bearing_node() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    // Populate the composite B-tree via MERGE (the executor's CREATE operator
    // does not maintain the composite index), then give the node a live
    // relationship so the delete exercises the DETACH path.
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 1})")
        .expect("initial tuple must populate the composite B-tree");
    engine
        .execute_cypher(
            "MATCH (p:Person {tenantId: 't1', id: 1}) CREATE (p)-[:R]->(x:Other {name: 'x'})",
        )
        .expect("adding a relationship to the indexed node must succeed");

    engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 1}) DETACH DELETE p")
        .expect("DETACH DELETE of a relationship-bearing node must succeed");

    engine
        .execute_cypher("MERGE (p2:Person {tenantId: 't1', id: 1})")
        .expect(
            "re-creating the exact same NODE KEY tuple after DETACH DELETE of a \
             relationship-bearing node must succeed",
        );
}

/// Regression for M-3 (`phase0_fix-delete-path-index-cleanup`): `delete_node`
/// must evict the deleted node's `(label, key, value)` tuple from the typed
/// property B-tree (`IndexManager::property_index`), not just the label
/// index. `PropertyIndex::find_exact` returns the RAW node-id bitmap for a
/// (label, key, value) triple with no `is_deleted` filtering — reads that go
/// through Cypher re-check `is_deleted()` downstream, so a leftover entry
/// here is invisible to ordinary queries but still corrupts anything that
/// consults the typed index directly (occupancy counts, future seeks that
/// assume the bitmap is deletion-clean).
#[test]
fn typed_index_has_no_dead_entry_after_delete() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.age)")
        .expect("CREATE INDEX DDL must succeed");

    engine
        .execute_cypher("CREATE (:Person {age: 42})")
        .expect("create of an indexed node must succeed");

    let label_id = engine
        .catalog
        .get_or_create_label("Person")
        .expect("resolve the Person label id");
    let key_id = engine
        .catalog
        .get_or_create_key("age")
        .expect("resolve the age key id");

    let before = engine
        .indexes
        .property_index
        .find_exact(label_id, key_id, PropertyValue::Integer(42))
        .expect("raw typed-index seek before delete");
    assert_eq!(
        before.len(),
        1,
        "typed property index must contain exactly the freshly created node before delete"
    );

    engine
        .execute_cypher("MATCH (p:Person {age: 42}) DELETE p")
        .expect("delete of the indexed node must succeed");

    let after = engine
        .indexes
        .property_index
        .find_exact(label_id, key_id, PropertyValue::Integer(42))
        .expect("raw typed-index seek after delete");
    assert!(
        after.is_empty(),
        "typed property index must not retain a dead entry for the deleted node — \
         leftover entries would corrupt any consumer of `find_exact` that (correctly) \
         assumes the bitmap is not polluted by soft-deleted rows: {after:?}"
    );
}

/// Regression for M-1 (`phase0_fix-delete-path-index-cleanup`): `delete_node`
/// must free the deleted node's property-store blob
/// (`RecordStore::delete_node_properties`), not just mark the node record
/// itself deleted. `RecordStore::property_count()` mirrors the in-memory
/// `HashMap` sizes backing the property store's forward/reverse index — a
/// deterministic live-entry count, not a file-size heuristic — so it can
/// assert the blob accounting returns to baseline after every created node
/// is deleted, without ever depending on OS file size or reclaim timing.
#[test]
fn property_blob_freed_after_node_delete() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let baseline = engine.storage.property_count();

    for i in 0..5 {
        engine
            .execute_cypher(&format!("CREATE (:Item {{seq: {i}}})"))
            .expect("create of a node with a non-empty property map must succeed");
    }

    let after_create = engine.storage.property_count();
    assert_eq!(
        after_create,
        baseline + 5,
        "each CREATE with a non-empty property map must add exactly one live \
         property-store entry"
    );

    engine
        .execute_cypher("MATCH (n:Item) DELETE n")
        .expect("delete of every Item node must succeed");

    let after_delete = engine.storage.property_count();
    assert_eq!(
        after_delete, baseline,
        "deleting every node created above must free its property-store blob, \
         returning the live property-entry count to its pre-test baseline instead \
         of leaking one entry per deleted node"
    );
}
