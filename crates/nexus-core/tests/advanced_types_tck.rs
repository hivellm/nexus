//! phase6_opencypher-advanced-types §7.1 — TCK-shaped integration
//! scenarios for the advanced-types surface.
//!
//! These tests mirror the shape the openCypher TCK uses for the
//! same features (bytes, dynamic labels, composite indexes, typed
//! collections, savepoints, graph scoping). They run in-process
//! against a fresh `Engine` per test, exercise the REST-equivalent
//! `execute_cypher` / `execute_cypher_with_params` entry points,
//! and assert on the public shape (ResultSet columns / rows / error
//! codes) rather than internal types.
//!
//! Why here and not inside `src/**/*::tests` — these scenarios are
//! end-to-end-ish: they span parser, executor, engine, catalog, and
//! the composite / typed-collection / savepoint registries. Keeping
//! them in the integration-test crate lets them exercise the full
//! `pub` surface and keeps them out of the per-module unit-test
//! budget.

use nexus_core::Engine;
use nexus_core::engine::typed_collections::ListElemType;
use nexus_core::executor::ResultSet;
use nexus_core::testing::setup_isolated_test_engine;
use std::collections::HashMap;

fn run(engine: &mut Engine, cypher: &str) -> ResultSet {
    engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("query failed: {cypher}\n  error: {e}"))
}

// ───────────────────────── §1 BYTES scalar family ─────────────────────────

#[test]
fn bytes_from_base64_roundtrip() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Wire shape: `{"_bytes": "<base64>"}` encodes to lowercase hex.
    let r = run(
        &mut engine,
        "RETURN bytesToHex(bytesFromBase64('AAH/')) AS hex",
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!("0001ff"));
}

#[test]
fn bytes_encode_utf8_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = run(&mut engine, "RETURN bytesToHex(bytes('abc')) AS hex");
    assert_eq!(r.rows[0].values[0], serde_json::json!("616263"));
}

#[test]
fn bytes_length_of_hex_decoded_value() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = run(
        &mut engine,
        "RETURN bytesLength(bytesFromBase64('AAECAwQ=')) AS len",
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!(5));
}

#[test]
fn bytes_slice_clamps_bounds() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Raw bytes 0x00 0x01 0x02 0x03 0x04 → slice(1, 3) → 0x01 0x02 0x03
    let r = run(
        &mut engine,
        "RETURN bytesToHex(bytesSlice(bytesFromBase64('AAECAwQ='), 1, 3)) AS hex",
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!("010203"));
}

#[test]
fn bytes_null_propagates() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let r = run(&mut engine, "RETURN bytesToHex(null) AS hex");
    assert_eq!(r.rows[0].values[0], serde_json::Value::Null);
}

// ─────────────────────── §2 Dynamic labels on writes ──────────────────────

#[test]
fn dynamic_label_create_via_params() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let mut params = HashMap::new();
    params.insert("label".to_string(), serde_json::json!("Person"));
    engine
        .execute_cypher_with_params("CREATE (n:$label)", params)
        .expect("CREATE with :$param must succeed");
    // Verify the node exists under that label.
    let r = run(&mut engine, "MATCH (n:Person) RETURN count(n) AS c");
    assert_eq!(r.rows[0].values[0], serde_json::json!(1));
}

#[test]
fn dynamic_label_list_expansion() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let mut params = HashMap::new();
    params.insert("labels".to_string(), serde_json::json!(["Person", "Admin"]));
    engine
        .execute_cypher_with_params("CREATE (n:$labels)", params)
        .unwrap();
    // Node is a Person.
    let r1 = run(&mut engine, "MATCH (n:Person) RETURN count(n) AS c");
    assert_eq!(r1.rows[0].values[0], serde_json::json!(1));
    // … and an Admin.
    let r2 = run(&mut engine, "MATCH (n:Admin) RETURN count(n) AS c");
    assert_eq!(r2.rows[0].values[0], serde_json::json!(1));
}

#[test]
fn dynamic_label_null_param_rejected() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let mut params = HashMap::new();
    params.insert("label".to_string(), serde_json::Value::Null);
    let err = engine
        .execute_cypher_with_params("CREATE (n:$label)", params)
        .expect_err("NULL label param must be rejected");
    assert!(
        err.to_string().contains("ERR_INVALID_LABEL"),
        "expected ERR_INVALID_LABEL, got {err}"
    );
}

#[test]
fn dynamic_label_missing_param_rejected() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher_with_params("CREATE (n:$missing)", HashMap::new())
        .expect_err("unbound param must fail");
    assert!(err.to_string().contains("ERR_INVALID_LABEL"));
}

#[test]
fn dynamic_label_non_string_list_element_rejected() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let mut params = HashMap::new();
    params.insert("labels".to_string(), serde_json::json!(["Person", 42]));
    let err = engine
        .execute_cypher_with_params("CREATE (n:$labels)", params)
        .expect_err("non-STRING list element must be rejected");
    assert!(err.to_string().contains("ERR_INVALID_LABEL"));
}

// ─────────────────────── §3 Composite index surface ───────────────────────

#[test]
fn composite_index_ddl_parses_and_registers() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE INDEX person_tenant_id FOR (p:Person) ON (p.tenantId, p.id)")
        .expect("composite index DDL must succeed");
    // db.indexes() reports the composite row with properties list.
    let r = run(&mut engine, "CALL db.indexes()");
    let has_composite = r.rows.iter().any(|row| {
        // Row layout per procedures.rs:
        //   0=id, 1=name, 2=state, 3=populationPercent, 4=uniqueness,
        //   5=type, 6=entityType, 7=labelsOrTypes, 8=properties, 9=indexProvider
        matches!(&row.values[1], serde_json::Value::String(s) if s == "person_tenant_id")
            && matches!(&row.values[5], serde_json::Value::String(s) if s == "BTREE")
    });
    assert!(
        has_composite,
        "db.indexes() must list the composite BTREE row. rows={:?}",
        r.rows
            .iter()
            .map(|row| row.values.clone())
            .collect::<Vec<_>>()
    );
}

// ─────────────────────── §4 Typed-collection surface ──────────────────────

#[test]
fn typed_list_constraint_rejects_wrong_element_type() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .add_typed_list_constraint("Doc", "tags", ListElemType::String)
        .unwrap();
    engine
        .create_node(
            vec!["Doc".to_string()],
            serde_json::json!({ "tags": ["a", "b"] }),
        )
        .expect("matching list must pass");
    let err = engine
        .create_node(
            vec!["Doc".to_string()],
            serde_json::json!({ "tags": ["a", 2] }),
        )
        .expect_err("mixed list must be rejected");
    assert!(err.to_string().contains("ERR_CONSTRAINT_VIOLATED"));
}

// ───────────────────────────── §5 Savepoints ──────────────────────────────

#[test]
fn savepoint_parse_surface() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // SAVEPOINT outside a transaction → ERR_SAVEPOINT_NO_TX.
    let err = engine
        .execute_cypher("SAVEPOINT s1")
        .expect_err("SAVEPOINT outside tx must fail");
    assert!(
        err.to_string().contains("ERR_SAVEPOINT_NO_TX"),
        "expected ERR_SAVEPOINT_NO_TX, got {err}"
    );
}

#[test]
fn rollback_to_unknown_savepoint_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();
    let err = engine
        .execute_cypher("ROLLBACK TO SAVEPOINT ghost")
        .expect_err("unknown savepoint must fail");
    assert!(err.to_string().contains("ERR_SAVEPOINT_UNKNOWN"));
    // Clean up the open transaction.
    let _ = engine.execute_cypher("ROLLBACK");
}

#[test]
fn release_savepoint_outside_tx_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let err = engine
        .execute_cypher("RELEASE SAVEPOINT s1")
        .expect_err("RELEASE outside tx must fail");
    assert!(err.to_string().contains("ERR_SAVEPOINT_NO_TX"));
}

// ────────────────────────────── §6 Graph scoping ──────────────────────────

#[test]
fn graph_scope_without_manager_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // The isolated test engine has no DatabaseManager attached, so
    // any GRAPH[name] scope is rejected with ERR_GRAPH_NOT_FOUND.
    let err = engine
        .execute_cypher("GRAPH[analytics] MATCH (n) RETURN n")
        .expect_err("single-engine must reject scoped queries");
    assert!(
        err.to_string().contains("ERR_GRAPH_NOT_FOUND"),
        "expected ERR_GRAPH_NOT_FOUND, got {err}"
    );
}

#[test]
fn query_without_graph_scope_runs_normally() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    // Sanity check that dropping the scope doesn't break the query.
    engine.execute_cypher("CREATE (n:Test)").unwrap();
    let r = run(&mut engine, "MATCH (n:Test) RETURN count(n) AS c");
    assert_eq!(r.rows[0].values[0], serde_json::json!(1));
}
