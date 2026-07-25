//! `MATCH ()-[r:$type]->()` — a relationship type supplied via a query
//! parameter — must parse and resolve at runtime.
//!
//! Before this change `-[r:$type]->` failed to PARSE (`parse_types` had no
//! `$` branch). Unlike node labels (which reuse a runtime `Filter` that
//! already resolved `$param`), relationship types are resolved only at PLAN
//! time, where query parameters are unavailable. The fix resolves the
//! `$type` sentinel against the runtime params in the engine BEFORE
//! planning (`engine::dynamic_types::resolve_types`) and hands the executor
//! the rewritten AST via a preparsed override, so a STRING param becomes one
//! type, a LIST<STRING> a `:A|B` union, and an invalid parameter raises
//! `ERR_INVALID_RELATIONSHIP_TYPE` instead of silently matching nothing.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

/// One KNOWS edge (target id 2) and one FOLLOWS edge (target id 3), so the
/// two types have distinguishable targets.
fn seed_typed_edges(engine: &mut Engine) {
    engine
        .execute_cypher("CREATE (:P)-[:KNOWS]->(:P {id: 2}), (:P)-[:FOLLOWS]->(:P {id: 3})")
        .expect("seed typed edges");
}

fn target_ids(engine: &mut Engine, query: &str, ty: serde_json::Value) -> Vec<i64> {
    let mut params: HashMap<String, serde_json::Value> = HashMap::new();
    params.insert("type".to_string(), ty);
    let rs = engine
        .execute_cypher_with_params(query, params)
        .expect("query must parse and execute");
    let mut ids: Vec<i64> = rs
        .rows
        .iter()
        .filter_map(|row| row.values.first().and_then(serde_json::Value::as_i64))
        .collect();
    ids.sort_unstable();
    ids
}

/// Positive discriminator: `-[r:$type]->` resolves to the single matching
/// type and returns only that type's target.
#[test]
fn dynamic_rel_type_resolves_string_param_to_matching_edges() {
    let (mut engine, _ctx) = engine();
    seed_typed_edges(&mut engine);

    let q = "MATCH ()-[r:$type]->(b) RETURN b.id AS id";
    assert_eq!(
        target_ids(&mut engine, q, serde_json::json!("KNOWS")),
        vec![2]
    );
    assert_eq!(
        target_ids(&mut engine, q, serde_json::json!("FOLLOWS")),
        vec![3]
    );
}

/// A LIST parameter expands to the union of the listed types (`:A|B`).
#[test]
fn dynamic_rel_type_list_param_expands_to_union() {
    let (mut engine, _ctx) = engine();
    seed_typed_edges(&mut engine);

    let q = "MATCH ()-[r:$type]->(b) RETURN b.id AS id";
    assert_eq!(
        target_ids(&mut engine, q, serde_json::json!(["KNOWS", "FOLLOWS"])),
        vec![2, 3]
    );
}

/// A valid-but-unregistered type name matches no relationships — this is
/// NOT the error case, just an empty result.
#[test]
fn dynamic_rel_type_unknown_type_returns_no_rows() {
    let (mut engine, _ctx) = engine();
    seed_typed_edges(&mut engine);

    let q = "MATCH ()-[r:$type]->(b) RETURN b.id AS id";
    assert!(target_ids(&mut engine, q, serde_json::json!("NOPE")).is_empty());
}

/// Invalid parameters raise `ERR_INVALID_RELATIONSHIP_TYPE` — never a panic,
/// never silent no-rows, never a garbage catalog type.
#[test]
fn dynamic_rel_type_invalid_param_raises_typed_error() {
    let (mut engine, _ctx) = engine();
    seed_typed_edges(&mut engine);

    let q = "MATCH ()-[r:$type]->(b) RETURN b.id AS id";
    for bad in [
        serde_json::Value::Null,
        serde_json::json!(""),
        serde_json::json!(42),
        serde_json::json!([]),
        serde_json::json!(["KNOWS", 42]),
    ] {
        let mut params: HashMap<String, serde_json::Value> = HashMap::new();
        params.insert("type".to_string(), bad.clone());
        let res = engine.execute_cypher_with_params(q, params);
        match res {
            Err(e) => assert!(
                e.to_string().contains("ERR_INVALID_RELATIONSHIP_TYPE"),
                "expected ERR_INVALID_RELATIONSHIP_TYPE for {bad:?}, got {e}"
            ),
            Ok(rs) => panic!(
                "expected an error for invalid $type {bad:?}, got {} rows",
                rs.rows.len()
            ),
        }
    }
}

/// The AST rewrite reaches the variable-length lowering too:
/// `-[r:$type*1..2]->` traverses the resolved type.
#[test]
fn dynamic_rel_type_variable_length_resolves() {
    let (mut engine, _ctx) = engine();
    seed_typed_edges(&mut engine);

    // Isolate: confirm the STATIC var-length equivalent behaves the same,
    // so this test measures the dynamic-type rewrite, not an unrelated
    // var-length-from-anonymous-source limitation.
    let static_ids: Vec<i64> = {
        let rs = engine
            .execute_cypher("MATCH ()-[r:KNOWS*1..2]->(b) RETURN b.id AS id")
            .expect("static var-length query");
        rs.rows
            .iter()
            .filter_map(|row| row.values.first().and_then(serde_json::Value::as_i64))
            .collect()
    };

    let q = "MATCH ()-[r:$type*1..2]->(b) RETURN b.id AS id";
    let ids = target_ids(&mut engine, q, serde_json::json!("KNOWS"));
    assert_eq!(
        ids, static_ids,
        "dynamic `-[r:$type*1..2]->` (type=KNOWS) must match the static \
         `-[r:KNOWS*1..2]->` result exactly; dynamic={ids:?} static={static_ids:?}"
    );
}
