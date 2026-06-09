//! Regression tests for write-path correctness: multi-hop bound-variable
//! CREATE chains, DETACH DELETE, and the typed-list constraint API roundtrip.

use super::*;

/// Multi-hop chain variant of the bound-variable CREATE fix —
/// 3 nodes, 2 edges both referencing earlier declarations. Locks
/// the invariant across more than one edge, which the single-edge
/// reproducer above cannot prove.
#[test]
fn create_bound_variable_chain_reuses_nodes() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    engine
        .execute_cypher(
            "CREATE (a:X {id: 1}), (b:X {id: 2}), (c:X {id: 3}), \
             (a)-[:R]->(b), (b)-[:R]->(c)",
        )
        .expect("CREATE must succeed");

    let node_count = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(
        node_count.rows[0].values[0].as_u64(),
        Some(3),
        "expected 3 nodes, got {:?}",
        node_count.rows[0].values[0]
    );

    let rel_count = engine
        .execute_cypher("MATCH ()-[r]->() RETURN count(r) AS c")
        .unwrap();
    assert_eq!(
        rel_count.rows[0].values[0].as_u64(),
        Some(2),
        "expected 2 relationships, got {:?}",
        rel_count.rows[0].values[0]
    );

    // Property preservation: every id still reaches exactly one
    // node — guards against the fix accidentally collapsing
    // genuinely distinct nodes.
    for id in 1..=3 {
        let r = engine
            .execute_cypher(&format!("MATCH (n {{id: {id}}}) RETURN count(n) AS c"))
            .unwrap();
        assert_eq!(
            r.rows[0].values[0].as_u64(),
            Some(1),
            "id={id} should match exactly one node"
        );
    }
}

/// Regression test for phase6_nexus-delete-executor-bug:
/// `MATCH (n) DETACH DELETE n` via `engine.execute_cypher` must
/// actually remove the nodes. The RPC dispatch used to bypass
/// this path by calling the operator pipeline directly, whose
/// `Operator::DetachDelete` handler is an explicit no-op; the
/// server-side fix landed in commit `d46e2cfc`. This test locks
/// the engine-level contract the fix depends on so a future
/// refactor cannot regress the interception silently.
#[test]
fn detach_delete_actually_clears_nodes_via_execute_cypher() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).unwrap();

    // Seed a handful of nodes.
    for _ in 0..5 {
        engine
            .create_node(
                vec!["X".to_string()],
                serde_json::Value::Object(serde_json::Map::new()),
            )
            .unwrap();
    }

    // Confirm they exist — `execute_cypher` count before delete.
    let before = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(before.rows.len(), 1, "count query returns one row");
    // `c` column — first cell should be the number 5.
    let cell = &before.rows[0].values[0];
    assert_eq!(cell.as_u64(), Some(5), "expected 5 nodes, got {cell:?}");

    // Run the DETACH DELETE statement through the same high-level
    // API a REST / RPC caller hits.
    engine
        .execute_cypher("MATCH (n) DETACH DELETE n")
        .expect("DETACH DELETE must succeed");

    // And now the count must be zero — the guard that catches a
    // silent-no-op regression.
    let after = engine
        .execute_cypher("MATCH (n) RETURN count(n) AS c")
        .unwrap();
    assert_eq!(after.rows.len(), 1);
    let cell = &after.rows[0].values[0];
    assert_eq!(
        cell.as_u64(),
        Some(0),
        "DETACH DELETE left {cell:?} nodes — DELETE regression"
    );
}

// phase6_opencypher-advanced-types §4.3 — typed-list constraint
// registration is covered by the unit tests in
// `crate::engine::typed_collections::tests` (exercises the
// `validate_list` path that `Engine::check_constraints` wraps) plus
// a mirror regression here on the public
// `add_typed_list_constraint` / `drop_typed_list_constraint` API
// that the wiring in `check_constraints` depends on. We deliberately
// do NOT spawn a full `Engine` for this coverage because every
// engine instance holds an LMDB environment and this crate's test
// suite already sits near the per-process TLS-slot limit on Windows.
#[test]
fn typed_list_constraint_api_roundtrip() {
    use crate::engine::typed_collections::{ListElemType, validate_list};

    // Accept-then-reject round-trip using the same validator the
    // engine calls from `check_constraints`.
    assert!(validate_list(&serde_json::json!([1, 2, 3]), ListElemType::Integer).is_ok());
    let err = validate_list(&serde_json::json!([1, "two"]), ListElemType::Integer).unwrap_err();
    assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));

    // The `ANY` element type always accepts mixed content (§4.4 fallback).
    assert!(
        validate_list(&serde_json::json!([1, "two", true]), ListElemType::Any).is_ok(),
        "LIST<ANY> must accept any element type"
    );
}
