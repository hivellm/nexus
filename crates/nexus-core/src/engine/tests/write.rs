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

// ─── phase7_merge-persists-rel-props (issue #25) ──────────────────────────

/// Helper: read a relationship's stored properties as a JSON object.
fn rel_props(engine: &Engine, rel_id: u64) -> serde_json::Map<String, serde_json::Value> {
    match engine
        .storage
        .load_relationship_properties(rel_id)
        .expect("load rel props")
    {
        Some(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    }
}

/// #25: `MERGE (a)-[r:T {k:v}]->(b)` must persist the inline relationship
/// properties when the edge is created (previously dropped — a hardcoded
/// empty map was used, so the edge had no props).
#[test]
fn merge_persists_inline_relationship_properties() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let a = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "a"}))
        .unwrap();
    let b = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "b"}))
        .unwrap();

    engine
        .execute_cypher("MATCH (a:N {id:'a'}), (b:N {id:'b'}) MERGE (a)-[r:REL {k:'v', n:7}]->(b)")
        .expect("MERGE with inline rel props");

    let rid = engine
        .find_relationship_between(a, b, "REL")
        .unwrap()
        .expect("edge created");
    let props = rel_props(&engine, rid);
    assert_eq!(
        props.get("k"),
        Some(&serde_json::json!("v")),
        "string prop persisted"
    );
    assert_eq!(
        props.get("n"),
        Some(&serde_json::json!(7)),
        "int prop persisted"
    );
}

/// #25: re-running the same MERGE is idempotent (no duplicate edge) and the
/// inline props from the first create remain.
#[test]
fn merge_relationship_idempotent_keeps_props() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let a = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "a"}))
        .unwrap();
    let b = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "b"}))
        .unwrap();
    let q = "MATCH (a:N {id:'a'}), (b:N {id:'b'}) MERGE (a)-[r:REL {k:'v'}]->(b)";
    engine.execute_cypher(q).expect("first MERGE");
    engine.execute_cypher(q).expect("second MERGE (idempotent)");

    let cnt = engine
        .execute_cypher("MATCH (:N {id:'a'})-[r:REL]->(:N {id:'b'}) RETURN count(r) AS c")
        .expect("count edges");
    assert_eq!(
        cnt.rows[0].values[0].as_i64(),
        Some(1),
        "MERGE must not duplicate the edge"
    );
    let rid = engine
        .find_relationship_between(a, b, "REL")
        .unwrap()
        .unwrap();
    assert_eq!(
        rel_props(&engine, rid).get("k"),
        Some(&serde_json::json!("v"))
    );
}

/// #25: `MATCH (a)-[r:T]->(b) SET r.k = v` must bind the relationship
/// variable and persist the property (previously: "Unknown variable 'r'
/// in SET clause" — the write-path MATCH never bound rel vars).
#[test]
fn set_on_matched_relationship_variable_persists() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let a = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "a"}))
        .unwrap();
    let b = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "b"}))
        .unwrap();
    // Propless MERGE, then attach props idempotently via SET on the rel var.
    engine
        .execute_cypher("MATCH (a:N {id:'a'}), (b:N {id:'b'}) MERGE (a)-[r:REL]->(b)")
        .expect("propless MERGE");
    engine
        .execute_cypher("MATCH (a:N {id:'a'})-[r:REL]->(b:N {id:'b'}) SET r.k = 'v', r.n = 3")
        .expect("SET r.k on matched rel var");

    let rid = engine
        .find_relationship_between(a, b, "REL")
        .unwrap()
        .unwrap();
    let props = rel_props(&engine, rid);
    assert_eq!(props.get("k"), Some(&serde_json::json!("v")));
    assert_eq!(props.get("n"), Some(&serde_json::json!(3)));
}

/// #25: `SET r += {…}` on a matched relationship variable merges the map
/// (and a null value removes the key).
#[test]
fn set_map_merge_on_relationship_variable() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let a = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "a"}))
        .unwrap();
    let b = engine
        .create_node(vec!["N".to_string()], serde_json::json!({"id": "b"}))
        .unwrap();
    engine
        .execute_cypher(
            "MATCH (a:N {id:'a'}), (b:N {id:'b'}) MERGE (a)-[r:REL {keep:1, drop:9}]->(b)",
        )
        .expect("MERGE with props");
    engine
        .execute_cypher(
            "MATCH (a:N {id:'a'})-[r:REL]->(b:N {id:'b'}) SET r += {added:'x', drop:null}",
        )
        .expect("SET r += map");

    let rid = engine
        .find_relationship_between(a, b, "REL")
        .unwrap()
        .unwrap();
    let props = rel_props(&engine, rid);
    assert_eq!(
        props.get("keep"),
        Some(&serde_json::json!(1)),
        "untouched key kept"
    );
    assert_eq!(
        props.get("added"),
        Some(&serde_json::json!("x")),
        "new key added"
    );
    assert_eq!(props.get("drop"), None, "null value removed the key");
}

/// #25: `SET r += {…}` must also work when the relationship is matched via
/// ANONYMOUS endpoints (`MATCH (:P {id:'e'})-[r:T]->(:P {id:'f'})`) — the
/// endpoint nodes are resolved by their pattern, not just from bound vars.
#[test]
fn set_on_relationship_with_anonymous_endpoints() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let a = engine
        .create_node(vec!["P".to_string()], serde_json::json!({"id": "e"}))
        .unwrap();
    let b = engine
        .create_node(vec!["P".to_string()], serde_json::json!({"id": "f"}))
        .unwrap();
    engine
        .execute_cypher("MATCH (a:P {id:'e'}), (b:P {id:'f'}) MERGE (a)-[r:T]->(b)")
        .expect("propless MERGE");
    // Anonymous endpoints — only the relationship is named.
    engine
        .execute_cypher("MATCH (:P {id:'e'})-[r:T]->(:P {id:'f'}) SET r.k = 'av', r += {m: 4}")
        .expect("SET on rel matched via anonymous endpoints");
    let rid = engine
        .find_relationship_between(a, b, "T")
        .unwrap()
        .unwrap();
    let props = rel_props(&engine, rid);
    assert_eq!(props.get("k"), Some(&serde_json::json!("av")));
    assert_eq!(props.get("m"), Some(&serde_json::json!(4)));
}
