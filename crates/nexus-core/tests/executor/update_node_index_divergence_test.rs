//! Regression suite for `phase0_fix-update-node-index-divergence`.
//!
//! `Engine::update_node` (the write path behind REST `PUT /data/nodes`, RPC
//! `UPDATE_NODE`, RESP3 `NODE.UPDATE`) wrote the node record and property blob
//! directly but called none of the index-refresh helpers the Cypher SET path
//! (`persist_node_state`) runs. So after an `update_node`, every typed / label
//! index that covers the node pointed at its OLD state: a seek on the new value
//! found nothing, a seek on the stale old value still (falsely) matched, and a
//! label change never reached the label bitmap index.

use nexus_core::index::PropertyValue;
use nexus_core::testing::setup_isolated_test_engine;
use serde_json::{Value, json};

/// Extract the internal `_nexus_id` from a node object returned by a query.
fn node_id(value: &Value) -> u64 {
    value
        .as_object()
        .and_then(|obj| obj.get("_nexus_id"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("expected a node object with _nexus_id, got {value:?}"))
}

/// §1.1 + §1.2 — after `update_node` changes an indexed property, the typed
/// index must find the NEW value and must NOT find the OLD value.
#[test]
fn update_node_refreshes_typed_index_new_and_old_value() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.email)")
        .expect("CREATE INDEX DDL must succeed");
    engine
        .execute_cypher("CREATE (n:Person {email: 'old@x.com'})")
        .expect("create must succeed");

    let created = engine
        .execute_cypher("MATCH (n:Person {email: 'old@x.com'}) RETURN n")
        .expect("initial lookup must succeed");
    assert_eq!(created.rows.len(), 1, "node must exist before the update");
    let id = node_id(&created.rows[0].values[0]);

    let label_id = engine.catalog.get_or_create_label("Person").unwrap();
    let key_id = engine.catalog.get_or_create_key("email").unwrap();

    engine
        .update_node(
            id,
            vec!["Person".to_string()],
            json!({"email": "new@x.com"}),
        )
        .expect("update_node must succeed");

    // Raw typed-index occupancy (no is_deleted / value re-check masking): the
    // new value must be present, the old value must be gone.
    let new_hits = engine
        .indexes
        .property_index
        .find_exact(
            label_id,
            key_id,
            PropertyValue::String("new@x.com".to_string()),
        )
        .expect("seek new value");
    assert_eq!(
        new_hits.len(),
        1,
        "typed index must point at the node's NEW value after update_node, got {new_hits:?}"
    );
    let old_hits = engine
        .indexes
        .property_index
        .find_exact(
            label_id,
            key_id,
            PropertyValue::String("old@x.com".to_string()),
        )
        .expect("seek old value");
    assert!(
        old_hits.is_empty(),
        "typed index must not retain the stale OLD value after update_node, got {old_hits:?}"
    );

    // And the real query path must agree.
    let by_new = engine
        .execute_cypher("MATCH (n:Person {email: 'new@x.com'}) RETURN n")
        .unwrap();
    assert_eq!(
        by_new.rows.len(),
        1,
        "node must be findable by its new value"
    );
    let by_old = engine
        .execute_cypher("MATCH (n:Person {email: 'old@x.com'}) RETURN n")
        .unwrap();
    assert_eq!(
        by_old.rows.len(),
        0,
        "node must no longer be findable by its old value"
    );
}

/// §3.3 — updating a NODE KEY property via `update_node` must refresh the
/// composite B-tree: the old tuple becomes reusable and the new tuple is taken.
#[test]
fn update_node_refreshes_composite_node_key_index() {
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

    let created = engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 1}) RETURN p")
        .expect("initial lookup must succeed");
    let id = node_id(&created.rows[0].values[0]);

    // Change the NODE KEY tuple from (t1,1) to (t1,2) via update_node.
    engine
        .update_node(
            id,
            vec!["Person".to_string()],
            json!({"tenantId": "t1", "id": 2}),
        )
        .expect("update_node must succeed");

    // The OLD tuple (t1,1) must now be free to re-create...
    engine
        .execute_cypher("MERGE (p2:Person {tenantId: 't1', id: 1})")
        .expect(
            "old NODE KEY tuple must be reusable after update_node moved the node off it — \
             a leftover composite entry would falsely reject this",
        );

    // ...and the NEW tuple (t1,2) must be taken (MERGE matches the same node,
    // so exactly one Person carries id=2).
    engine
        .execute_cypher("MERGE (p3:Person {tenantId: 't1', id: 2})")
        .expect("new tuple MERGE must succeed");
    let with_id2 = engine
        .execute_cypher("MATCH (p:Person {tenantId: 't1', id: 2}) RETURN p")
        .unwrap();
    assert_eq!(
        with_id2.rows.len(),
        1,
        "exactly one node must carry the new NODE KEY tuple (t1,2), not a duplicate"
    );
}

/// §1.3 — a label change via `update_node` must reach the label-bitmap index so
/// `MATCH (n:NewLabel)` finds the node.
#[test]
fn update_node_refreshes_label_index_on_label_change() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    engine
        .execute_cypher("CREATE (n:Person {name: 'alice'})")
        .expect("create must succeed");
    let created = engine
        .execute_cypher("MATCH (n:Person {name: 'alice'}) RETURN n")
        .expect("initial lookup must succeed");
    let id = node_id(&created.rows[0].values[0]);

    // Relabel the node to Employee via update_node.
    engine
        .update_node(id, vec!["Employee".to_string()], json!({"name": "alice"}))
        .expect("update_node must succeed");

    let by_new_label = engine
        .execute_cypher("MATCH (n:Employee) RETURN n")
        .unwrap();
    assert_eq!(
        by_new_label.rows.len(),
        1,
        "node must be findable by its new label after update_node"
    );
}
