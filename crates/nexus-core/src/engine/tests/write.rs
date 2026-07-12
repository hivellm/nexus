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

// ─── phase1_http-merge-rel-and-set-rel-parity (B4, B8) ────────────────────

/// B4: `MATCH (n) SET n.p = $param` must resolve the parameter to its real
/// value via `evaluate_set_expression`'s `Expression::Parameter` arm, not
/// hard-code it to `Value::Null`.
#[test]
fn set_node_property_resolves_parameter() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (n:B4 {id: 1})")
        .expect("baseline CREATE");

    let mut params = std::collections::HashMap::new();
    params.insert("v".to_string(), serde_json::json!(42));
    engine
        .execute_cypher_with_params("MATCH (n:B4 {id: 1}) SET n.p = $v", params)
        .expect("SET n.p = $param must succeed");

    let read = engine
        .execute_cypher("MATCH (n:B4 {id: 1}) RETURN n.p AS p")
        .expect("read after SET $param");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(42),
        "SET n.p = $param must persist the parameterized value, not null"
    );
}

/// B4: a missing parameter still resolves to NULL (safe no-op), matching
/// the pre-existing `SET n += $missing` contract — the fix only changes
/// behaviour for a *bound* parameter.
#[test]
fn set_node_property_missing_parameter_is_null() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (n:B4Missing {id: 1})")
        .expect("baseline CREATE");

    engine
        .execute_cypher("MATCH (n:B4Missing {id: 1}) SET n.p = $absent")
        .expect("SET with an unbound parameter must not error");

    let read = engine
        .execute_cypher("MATCH (n:B4Missing {id: 1}) RETURN n.p AS p")
        .expect("read after SET with unbound param");
    assert!(
        read.rows[0].values[0].is_null(),
        "an unbound parameter must resolve to NULL, got {:?}",
        read.rows[0].values[0]
    );
}

/// B8: `SET n.p = null` must REMOVE the property key (Neo4j semantics: a
/// null-valued property is absent), not store a literal JSON null.
#[test]
fn set_node_property_null_removes_key() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (n:B8 {id: 1, p: 'to-remove'})")
        .expect("baseline CREATE");

    engine
        .execute_cypher("MATCH (n:B8 {id: 1}) SET n.p = null")
        .expect("SET n.p = null");

    let read = engine
        .execute_cypher("MATCH (n:B8 {id: 1}) RETURN n")
        .expect("read after SET n.p = null");
    let node = read.rows[0].values[0]
        .as_object()
        .expect("RETURN n is a node object");
    assert!(
        !node.contains_key("p"),
        "SET n.p = null must remove the key p, not store a null value; node = {:?}",
        node
    );
}

/// B8: setting a SECOND property to null on the same statement removes only
/// that key, leaving unrelated properties untouched.
#[test]
fn set_node_property_null_leaves_other_properties_untouched() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (n:B8Multi {id: 1, keep: 'k', drop: 'd'})")
        .expect("baseline CREATE");

    engine
        .execute_cypher("MATCH (n:B8Multi {id: 1}) SET n.drop = null")
        .expect("SET n.drop = null");

    let read = engine
        .execute_cypher("MATCH (n:B8Multi {id: 1}) RETURN n")
        .expect("read after SET n.drop = null");
    let node = read.rows[0].values[0]
        .as_object()
        .expect("RETURN n is a node object");
    assert!(!node.contains_key("drop"), "drop key must be removed");
    assert_eq!(
        node.get("keep"),
        Some(&serde_json::json!("k")),
        "unrelated key must survive the null-removal SET"
    );
}

/// G1: `CREATE (n:L {x: $v})` via `execute_cypher_with_params` must resolve
/// the `$param` inline node property, not error with "Complex expressions
/// not supported in CREATE properties". Mirrors the `Expression::Parameter`
/// arm added to `match_exec.rs::expression_to_json_value`.
#[test]
fn create_node_property_resolves_parameter() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("v".to_string(), serde_json::json!(9));
    engine
        .execute_cypher_with_params("CREATE (n:G1 {x: $v})", params)
        .expect("CREATE with $param property must succeed");

    let read = engine
        .execute_cypher("MATCH (n:G1) RETURN n.x AS x")
        .expect("read after CREATE $param");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(9),
        "parameterized CREATE property must persist as 9, not null"
    );
}

/// G1: the same `$param` resolution must also apply to inline relationship
/// properties in `CREATE (a)-[r:T {w: $w}]->(b)`.
#[test]
fn create_relationship_property_resolves_parameter() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let mut params = std::collections::HashMap::new();
    params.insert("w".to_string(), serde_json::json!(8));
    engine
        .execute_cypher_with_params("CREATE (a:G1RelA)-[r:G1Rel {w: $w}]->(b:G1RelB)", params)
        .expect("CREATE relationship with $param property must succeed");

    let read = engine
        .execute_cypher("MATCH (:G1RelA)-[r:G1Rel]->(:G1RelB) RETURN r.w AS w")
        .expect("read after CREATE rel $param");
    assert_eq!(
        read.rows[0].values[0].as_i64(),
        Some(8),
        "parameterized relationship property must persist as 8, not null"
    );
}

/// G1: a genuinely unbound `$param` in a CREATE property must error clearly
/// (naming the parameter) instead of silently degrading to NULL — CREATE
/// properties are not the same safe-no-op contract as `SET n += $missing`.
#[test]
fn create_node_property_missing_parameter_errors() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let err = engine
        .execute_cypher_with_params(
            "CREATE (n:G1Missing {x: $absent})",
            std::collections::HashMap::new(),
        )
        .expect_err("CREATE with an unbound $param must error, not silently null it");
    assert!(
        err.to_string().contains("absent"),
        "error must name the missing parameter; got {err}"
    );
}

/// G2: `CREATE (n:X {p:1}) REMOVE n.p` in ONE statement must persist the
/// removal. Previously `execute_write_query`'s clause loop had no
/// `Clause::Create` arm — it fell through to the catch-all and silently
/// dropped the CREATE, leaving `n` unbound by the time REMOVE ran
/// ("Unknown variable 'n' in REMOVE clause").
#[test]
fn create_then_remove_property_same_statement_persists() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher_with_params(
            "CREATE (n:G2 {p: 1}) REMOVE n.p",
            std::collections::HashMap::new(),
        )
        .expect("CREATE + REMOVE n.p in one statement must succeed");

    let read = engine
        .execute_cypher("MATCH (n:G2) RETURN n")
        .expect("read after CREATE + REMOVE n.p");
    let node = read.rows[0].values[0]
        .as_object()
        .expect("RETURN n is a node object");
    assert!(
        !node.contains_key("p"),
        "REMOVE n.p must remove the key in the same statement; node = {:?}",
        node
    );
}

/// G3: a STANDALONE relationship-MERGE pattern with no preceding MATCH
/// (`MERGE (a:L1 {..})-[r:T]->(b:L2 {..})`) must MERGE (find-or-create) its
/// own endpoints and create the edge — not silently create only the first
/// endpoint node while dropping the relationship. `process_merge_relationship`
/// previously required both endpoint variables to already be bound in
/// `context` (populated only by a preceding MATCH), so a standalone pattern
/// always returned `None` and fell back to a node-only MERGE that binds
/// exactly one variable via `find_map`. Re-running the same MERGE must stay
/// idempotent (exactly one node per label, exactly one edge).
#[test]
fn merge_relationship_standalone_creates_edge_idempotently() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    let merge_query = "MERGE (a:GA {id: 1})-[r:GR]->(b:GB {id: 2})";

    engine
        .execute_cypher_with_params(merge_query, std::collections::HashMap::new())
        .expect("first standalone MERGE relationship must succeed");
    engine
        .execute_cypher_with_params(merge_query, std::collections::HashMap::new())
        .expect("second (idempotent) standalone MERGE relationship must succeed");

    let a_count = engine
        .execute_cypher("MATCH (a:GA) RETURN count(a) AS c")
        .expect("count GA nodes");
    assert_eq!(
        a_count.rows[0].values[0].as_i64(),
        Some(1),
        "repeated MERGE must not duplicate the source node"
    );
    let b_count = engine
        .execute_cypher("MATCH (b:GB) RETURN count(b) AS c")
        .expect("count GB nodes");
    assert_eq!(
        b_count.rows[0].values[0].as_i64(),
        Some(1),
        "repeated MERGE must not duplicate the target node"
    );

    let rel_count = engine
        .execute_cypher("MATCH ()-[r:GR]->() RETURN count(r) AS c")
        .expect("count GR relationships");
    assert_eq!(
        rel_count.rows[0].values[0].as_i64(),
        Some(1),
        "repeated MERGE must not duplicate the edge"
    );

    let endpoints = engine
        .execute_cypher("MATCH (a:GA)-[r:GR]->(b:GB) RETURN a.id AS a_id, b.id AS b_id")
        .expect("read edge endpoints");
    assert_eq!(
        endpoints.rows[0].values[0].as_i64(),
        Some(1),
        "edge must originate from the GA node with id 1"
    );
    assert_eq!(
        endpoints.rows[0].values[1].as_i64(),
        Some(2),
        "edge must terminate at the GB node with id 2"
    );
}
