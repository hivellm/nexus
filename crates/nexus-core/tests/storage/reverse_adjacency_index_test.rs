//! The store's authoritative adjacency index — in particular the REVERSE
//! (incoming) half, which the record format does not carry at all
//! (`NodeRecord::first_rel_ptr` heads the outgoing chain only).
//!
//! What these pin down: the index is maintained at `RecordStore::write_rel`,
//! the single funnel every relationship-record mutation passes through, so it
//! stays correct no matter WHICH path wrote the edge — and it is rebuilt from
//! the records when the store is opened, so a restart cannot lose it. That is
//! what lets the node-delete guard and DETACH DELETE stop scanning every
//! relationship in the database.
//!
//! See phase0_perf-store-reverse-incoming-adjacency-index and
//! docs/analysis/store-adjacency-index/.

use nexus_core::Engine;
use nexus_core::storage::RecordStore;
use nexus_core::testing::TestContext;

/// Internal node id of the `:N {id: <marker>}` node.
fn node_id(engine: &mut Engine, marker: i64) -> u64 {
    let r = engine
        .execute_cypher(&format!("MATCH (n:N {{id: {marker}}}) RETURN id(n) AS i"))
        .expect("id lookup");
    assert_eq!(
        r.rows.len(),
        1,
        "marker {marker} must match exactly one node"
    );
    r.rows[0].values[0].as_u64().expect("id is an integer")
}

// ───────────────── every write path keeps it authoritative ─────────────────

#[test]
fn cypher_create_populates_the_incoming_side() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1})-[:R]->(:N {id: 2})")
        .expect("create edge");

    let (a, b) = (node_id(&mut engine, 1), node_id(&mut engine, 2));
    assert_eq!(
        engine.storage.incoming_relationships(b).len(),
        1,
        "the executor CREATE path must register the incoming edge"
    );
    assert!(
        engine.storage.incoming_relationships(a).is_empty(),
        "nothing points at the source node"
    );
    assert_eq!(engine.storage.outgoing_relationships(a).len(), 1);
}

#[test]
fn engine_create_relationship_populates_the_incoming_side() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1}), (:N {id: 2})")
        .expect("seed");
    let (a, b) = (node_id(&mut engine, 1), node_id(&mut engine, 2));

    engine
        .create_relationship(a, b, "R".to_string(), serde_json::json!({}))
        .expect("engine create_relationship");

    assert_eq!(
        engine.storage.incoming_relationships(b).len(),
        1,
        "the engine CRUD path must register the incoming edge"
    );
}

#[test]
fn direct_store_write_populates_both_sides() {
    // The lowest level: a raw `write_rel`, which is what every deletion path
    // and the bulk loader ultimately call. Nothing here knows the index
    // exists — that is the point.
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).expect("store");
    let record = nexus_core::storage::RelationshipRecord::new(4, 9, 0);
    store.write_rel(0, &record).expect("write rel");

    assert_eq!(store.outgoing_relationships(4), vec![0]);
    assert_eq!(store.incoming_relationships(9), vec![0]);
    assert_eq!(store.connected_relationships(9), vec![0]);
    assert!(store.has_any_relationship(9));
}

#[test]
fn marking_a_record_deleted_removes_it_from_both_sides() {
    let ctx = TestContext::new();
    let mut store = RecordStore::new(ctx.path()).expect("store");
    let mut record = nexus_core::storage::RelationshipRecord::new(4, 9, 0);
    store.write_rel(0, &record).expect("write rel");

    record.mark_deleted();
    store.write_rel(0, &record).expect("write deleted rel");

    assert!(
        store.outgoing_relationships(4).is_empty(),
        "a deleted edge must leave the outgoing side"
    );
    assert!(
        store.incoming_relationships(9).is_empty(),
        "a deleted edge must leave the incoming side"
    );
    assert!(!store.has_any_relationship(9));
}

#[test]
fn a_store_clone_shares_the_index() {
    // The executor runs on a clone of the engine's store; an edge created
    // through one must be visible to the other, or the delete guard reading
    // through the engine would miss it.
    let ctx = TestContext::new();
    let store = RecordStore::new(ctx.path()).expect("store");
    let mut clone = store.clone();
    let record = nexus_core::storage::RelationshipRecord::new(1, 2, 0);
    clone
        .write_rel(0, &record)
        .expect("write through the clone");

    assert_eq!(
        store.incoming_relationships(2),
        vec![0],
        "clones share one index"
    );
}

// ────────────────────────── rebuild on reopen ──────────────────────────────

#[test]
fn the_index_is_rebuilt_from_the_records_on_reopen() {
    let ctx = TestContext::new();
    let (a, b) = {
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
        engine
            .execute_cypher("CREATE (:N {id: 1})-[:R]->(:N {id: 2})")
            .expect("create edge");
        let ids = (node_id(&mut engine, 1), node_id(&mut engine, 2));
        engine.flush().expect("flush");
        ids
    };

    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("reopen");
    assert_eq!(
        engine.storage.incoming_relationships(b).len(),
        1,
        "the reverse adjacency must survive a reopen (rebuilt from records)"
    );
    assert_eq!(engine.storage.outgoing_relationships(a).len(), 1);

    // And the guard it backs must still refuse the incoming-only delete.
    assert!(
        engine.delete_node(b).is_err(),
        "an incoming-only node must still be undeletable after a reopen"
    );
}

#[test]
fn a_deleted_edge_is_not_resurrected_by_the_reopen_rebuild() {
    let ctx = TestContext::new();
    let b = {
        let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
        engine
            .execute_cypher("CREATE (:N {id: 1})-[:R]->(:N {id: 2})")
            .expect("create edge");
        engine
            .execute_cypher("MATCH ()-[r:R]->() DELETE r")
            .expect("delete edge");
        let id = node_id(&mut engine, 2);
        engine.flush().expect("flush");
        id
    };

    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("reopen");
    assert!(
        engine.storage.incoming_relationships(b).is_empty(),
        "the rebuild must skip deleted records"
    );
    assert!(
        engine.delete_node(b).is_ok(),
        "with its only edge deleted, the node is deletable again"
    );
}

// ─────────────────── the guards the index exists to serve ──────────────────

#[test]
fn an_incoming_only_node_cannot_be_plain_deleted() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1})-[:R]->(:N {id: 2})")
        .expect("create edge");
    let b = node_id(&mut engine, 2);

    // `b` has NO outgoing edge — before the reverse index this could only be
    // answered by scanning every relationship in the database.
    assert!(
        engine.delete_node(b).is_err(),
        "a live incoming edge must block a non-DETACH delete"
    );
}

#[test]
fn detach_delete_clears_the_incoming_side() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1}), (:N {id: 2}), (:N {id: 3})")
        .expect("seed");
    // Two distinct sources pointing at the SAME hub — `CREATE` always creates
    // fresh nodes, so the fan-in has to be built by matching the hub back.
    engine
        .execute_cypher("MATCH (a:N {id: 1}), (b:N {id: 2}) CREATE (a)-[:R]->(b)")
        .expect("edge 1");
    engine
        .execute_cypher("MATCH (c:N {id: 3}), (b:N {id: 2}) CREATE (c)-[:R]->(b)")
        .expect("edge 2");
    let b = node_id(&mut engine, 2);
    assert_eq!(
        engine.storage.incoming_relationships(b).len(),
        2,
        "two edges point at the hub"
    );

    engine
        .delete_node_relationships(b)
        .expect("detach the hub's edges");

    assert!(
        engine.storage.incoming_relationships(b).is_empty(),
        "DETACH must clear the incoming side"
    );
    assert!(
        engine.delete_node(b).is_ok(),
        "the detached node is now deletable"
    );
    let remaining = engine
        .execute_cypher("MATCH ()-[r:R]->() RETURN count(r) AS c")
        .expect("count");
    assert_eq!(
        remaining.rows[0].values[0].as_u64(),
        Some(0),
        "both incoming edges are gone"
    );
}

#[test]
fn detach_delete_clears_a_self_loop() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1})")
        .expect("seed node");
    engine
        .execute_cypher("MATCH (n:N {id: 1}) CREATE (n)-[:R]->(n)")
        .expect("create self-loop");
    let a = node_id(&mut engine, 1);
    assert_eq!(
        engine.storage.connected_relationships(a).len(),
        1,
        "a self-loop is ONE incident edge, not two"
    );

    engine.delete_node_relationships(a).expect("detach");
    assert!(engine.storage.connected_relationships(a).is_empty());
    assert!(engine.delete_node(a).is_ok());
}

#[test]
fn churn_leaves_the_index_exact() {
    // Create/delete/re-create the same edge: the index must track live edges
    // only, never accumulate stale ids (which would permanently block the
    // node from being deleted).
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher("CREATE (:N {id: 1}), (:N {id: 2})")
        .expect("seed");
    let (a, b) = (node_id(&mut engine, 1), node_id(&mut engine, 2));

    for _ in 0..3 {
        engine
            .execute_cypher("MATCH (a:N {id: 1}), (b:N {id: 2}) CREATE (a)-[:R]->(b)")
            .expect("create edge");
        assert_eq!(
            engine.storage.incoming_relationships(b).len(),
            1,
            "exactly one live incoming edge"
        );
        engine
            .execute_cypher("MATCH ()-[r:R]->() DELETE r")
            .expect("delete edge");
        assert!(
            engine.storage.incoming_relationships(b).is_empty(),
            "no live incoming edge left"
        );
        assert!(
            engine.storage.outgoing_relationships(a).is_empty(),
            "no live outgoing edge left"
        );
    }

    assert!(
        engine.delete_node(b).is_ok(),
        "after the churn the node must still be deletable"
    );
}
