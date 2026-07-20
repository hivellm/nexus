//! Regression suite for `phase0_fix-delete-node-dangling-relationships`.
//!
//! Three chained defects let a node with a live relationship be
//! hard-deleted, corrupting every subsequent traversal through the
//! dangling edge:
//!
//!   - C-2a: the Cypher plain-DELETE guard checked only
//!     `first_rel_ptr != 0`, but `first_rel_ptr` tracks OUTGOING
//!     relationships exclusively — an incoming-only node (`first_rel_ptr
//!     == 0`) passed the guard and was hard-deleted while a live edge
//!     still pointed at it.
//!   - C-2b: `Engine::delete_node` itself had no relationship check at
//!     all, so every direct caller (REST/RPC/RESP3) deleted
//!     unconditionally.
//!   - C-2c: `Expand` resolved a dangling endpoint to `Value::Null` and
//!     pushed the row anyway instead of skipping it, so a query on a
//!     corrupted graph silently returned `b = null` (and `count(r)` kept
//!     counting the dangling edge) instead of surfacing the corruption.
//!
//! The canonical trigger:
//! ```text
//! CREATE (a:Person{name:'Alice'})-[:KNOWS]->(b:Person{name:'Bob'})
//! MATCH (b:Person{name:'Bob'}) DELETE b        -- guard passed, b hard-deleted
//! MATCH (a)-[r:KNOWS]->(b) RETURN a,r,b         -- returned a, live r, b=null
//! ```

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;

/// Extracts the internal `_nexus_id` from a node `Value::Object` returned
/// by a Cypher query, matching the `_nexus_id` convention used across the
/// executor (see `crates/nexus-core/tests/cypher/builtin_functions_test.rs`).
fn node_id(value: &Value) -> u64 {
    value
        .as_object()
        .and_then(|obj| obj.get("_nexus_id"))
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("expected a node object with _nexus_id, got {value:?}"))
}

// ── C-2a: the Cypher plain-DELETE guard must see incoming relationships ──

#[test]
fn non_detach_cypher_delete_of_incoming_only_node_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    // Pre-fix: this SUCCEEDED, because `b` is only ever a relationship
    // TARGET, so `b.first_rel_ptr == 0` and the old
    // `first_rel_ptr != 0` guard in `match_exec.rs` passed it through.
    let result = engine.execute_cypher("MATCH (b:Person {name: 'Bob'}) DELETE b");
    assert!(
        result.is_err(),
        "non-DETACH DELETE of an incoming-only node with a live relationship must be \
         refused, got {result:?}"
    );

    // Bob must still be present — the refusal must not have partially
    // applied the delete.
    let still_there = engine
        .execute_cypher("MATCH (b:Person {name: 'Bob'}) RETURN b")
        .unwrap();
    assert_eq!(
        still_there.rows.len(),
        1,
        "node must survive a refused non-DETACH delete"
    );
}

// ── C-2c: Expand must skip a dangling endpoint, not surface it as null ───

#[test]
fn expand_skips_dangling_endpoint_instead_of_null_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let created = engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'}) RETURN a, b",
        )
        .unwrap();
    let b_id = node_id(&created.rows[0].values[1]);

    // `Engine::delete_node` now refuses this exact scenario (that's C-2a/
    // C-2b fixed), so the only way left to fabricate the dangling-edge
    // invariant Expand must defend against — a live relationship pointing
    // at a hard-deleted node — is to bypass the engine and mark the node
    // deleted directly at the storage layer, exactly mirroring what the
    // pre-fix `delete_node` used to do internally (`NodeRecord::mark_deleted`
    // + `write_node`) without going through the new guard.
    let mut b_record = engine.storage.read_node(b_id).unwrap();
    b_record.mark_deleted();
    engine.storage.write_node(b_id, &b_record).unwrap();

    // Pre-fix: this returned one row with `b = null` (and `r` still bound
    // to the live, now-dangling relationship).
    let matched = engine
        .execute_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN a, r, b")
        .unwrap();
    assert_eq!(
        matched.rows.len(),
        0,
        "Expand must skip a row whose endpoint is dangling instead of returning b=null, \
         got {:?}",
        matched.rows
    );

    // Pre-fix: `count(r)` kept counting the dangling edge forever.
    let counted = engine
        .execute_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS c")
        .unwrap();
    let c = counted.rows[0].values[0]
        .as_i64()
        .expect("count(r) must be numeric");
    assert_eq!(
        c, 0,
        "count(r) must not count a relationship whose endpoint is dangling"
    );
}

// ── OPTIONAL MATCH must keep its own null-row semantics ──────────────────

#[test]
fn optional_match_still_yields_null_row_when_no_relationship_matches() {
    // C-2c's skip is scoped to NON-optional patterns only — OPTIONAL MATCH
    // legitimately yields NULL for a target/relationship variable when no
    // match exists, and that must be unaffected by the dangling-endpoint
    // skip added to the same operator.
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})")
        .unwrap();

    let result = engine
        .execute_cypher(
            "MATCH (a:Person) OPTIONAL MATCH (a)-[r:NONEXISTENT]->(x) \
             RETURN a.name AS aname, x AS x_raw",
        )
        .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert_eq!(
        result.rows[0].values[1],
        Value::Null,
        "OPTIONAL MATCH with no relationship match must still bind the target to NULL"
    );
}

// ── DETACH DELETE must keep working end to end ────────────────────────────

#[test]
fn detach_delete_still_works() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    engine
        .execute_cypher("MATCH (b:Person {name: 'Bob'}) DETACH DELETE b")
        .unwrap();

    let counted = engine
        .execute_cypher("MATCH (a)-[r:KNOWS]->(b) RETURN count(r) AS c")
        .unwrap();
    let c = counted.rows[0].values[0]
        .as_i64()
        .expect("count(r) must be numeric");
    assert_eq!(c, 0, "DETACH DELETE must clear the relationship too");

    let bob_gone = engine
        .execute_cypher("MATCH (b:Person {name: 'Bob'}) RETURN b")
        .unwrap();
    assert_eq!(
        bob_gone.rows.len(),
        0,
        "Bob must be gone after DETACH DELETE"
    );
}

// ── Tier 1 (outgoing fast-path) coverage ──────────────────────────────────
//
// `node_has_live_relationship` walks the node's own `first_rel_ptr` /
// `next_src_ptr` chain as an O(out-degree) short-circuit before ever
// reaching the authoritative full-store scan. These tests exercise that
// fast path directly: a node with a live OUTGOING edge, a node whose only
// outgoing edge has been soft-deleted, and a node with both an incoming and
// an outgoing edge (Tier 1 must catch the outgoing one).

#[test]
fn non_detach_delete_of_outgoing_only_node_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    // Alice is the relationship SOURCE, so `first_rel_ptr` heads her own
    // outgoing chain — the Tier 1 fast path must see the live edge and
    // short-circuit to `true` without needing the full-store scan.
    let result = engine.execute_cypher("MATCH (a:Person {name: 'Alice'}) DELETE a");
    assert!(
        result.is_err(),
        "non-DETACH DELETE of a node with a live outgoing relationship must be refused, \
         got {result:?}"
    );

    let still_there = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) RETURN a")
        .unwrap();
    assert_eq!(
        still_there.rows.len(),
        1,
        "node must survive a refused non-DETACH delete"
    );
}

#[test]
fn non_detach_delete_allowed_after_outgoing_edge_soft_deleted() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(b:Person {name: 'Bob'})")
        .unwrap();

    // NOTE: Cypher relationship delete (`MATCH ... DELETE r`) is a pre-existing
    // no-op stub — the `Delete` operator never wires through to
    // `storage::delete_rel`, so the edge record is left live (tracked as a
    // separate phase0 task). To authentically produce the "node whose only
    // outgoing edge is soft-deleted" state that Tier 1 must walk past, mark the
    // edge deleted directly at the storage layer — the same technique
    // `expand_skips_dangling_endpoint_instead_of_null_row` uses for nodes.
    let total_rels = engine.storage.relationship_count();
    for rel_id in 0..total_rels {
        let mut rel = engine.storage.read_rel(rel_id).unwrap();
        rel.mark_deleted();
        engine.storage.write_rel(rel_id, &rel).unwrap();
    }

    // Alice's only outgoing edge is now soft-deleted, so Tier 1 must walk past
    // it (`rel.is_deleted()` true) without short-circuiting, and Tier 2's
    // authoritative scan must confirm no live relationship remains — the plain
    // DELETE must succeed.
    engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) DELETE a")
        .unwrap();

    let gone = engine
        .execute_cypher("MATCH (a:Person {name: 'Alice'}) RETURN a")
        .unwrap();
    assert_eq!(
        gone.rows.len(),
        0,
        "node must be deletable once its only relationship is soft-deleted"
    );
}

#[test]
fn non_detach_delete_of_node_with_both_incoming_and_outgoing_edges_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher(
            "CREATE (a:Person {name: 'Alice'})-[:KNOWS]->(hub:Person {name: 'Hub'})-[:KNOWS]->(c:Person {name: 'Carol'})",
        )
        .unwrap();

    // Hub is both a relationship DESTINATION (from Alice) and a
    // relationship SOURCE (to Carol). Tier 1 only needs to see the live
    // outgoing edge to Carol to short-circuit to `true`.
    let result = engine.execute_cypher("MATCH (hub:Person {name: 'Hub'}) DELETE hub");
    assert!(
        result.is_err(),
        "non-DETACH DELETE of a node with both incoming and outgoing relationships must be \
         refused, got {result:?}"
    );

    let still_there = engine
        .execute_cypher("MATCH (hub:Person {name: 'Hub'}) RETURN hub")
        .unwrap();
    assert_eq!(
        still_there.rows.len(),
        1,
        "node must survive a refused non-DETACH delete"
    );
}
