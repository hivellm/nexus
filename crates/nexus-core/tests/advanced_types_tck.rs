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

// ──────────────────── §7 $param propagation (GH issue #3) ─────────────────
//
// Regression coverage: POST /cypher with a parameters map returned 0 rows
// because the MATCH branch called execute_cypher (discarding params) and the
// read-fallthrough path hard-coded HashMap::new() instead of current_params.

/// Inline prop-map param: `MATCH (s {id: $id})` binds via the node
/// predicate evaluator. Expects exactly 1 row when the param matches.
#[test]
fn param_inline_prop_map_match_returns_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'alice'})")
        .unwrap();

    let mut params = HashMap::new();
    params.insert("id".to_string(), serde_json::json!("alice"));
    let r = engine
        .execute_cypher_with_params("MATCH (s {id: $id}) RETURN id(s)", params)
        .expect("MATCH with inline prop-map param must succeed");

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row, got {}",
        r.rows.len()
    );
}

/// WHERE param: `MATCH (s) WHERE s.id = $id` binds via the WHERE
/// predicate evaluator. Expects exactly 1 row when the param matches.
#[test]
fn param_where_clause_match_returns_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'bob'})")
        .unwrap();

    let mut params = HashMap::new();
    params.insert("id".to_string(), serde_json::json!("bob"));
    let r = engine
        .execute_cypher_with_params("MATCH (s) WHERE s.id = $id RETURN id(s)", params)
        .expect("MATCH with WHERE param must succeed");

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row, got {}",
        r.rows.len()
    );
}

/// Negative control: a param value that matches nothing must return 0 rows,
/// proving the predicate is actually evaluated rather than silently ignored.
#[test]
fn param_no_match_returns_zero_rows() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'carol'})")
        .unwrap();

    let mut params = HashMap::new();
    params.insert("id".to_string(), serde_json::json!("nobody"));
    let r = engine
        .execute_cypher_with_params("MATCH (s {id: $id}) RETURN id(s)", params)
        .expect("query must succeed even with zero matches");

    assert_eq!(
        r.rows.len(),
        0,
        "expected 0 rows for non-matching param, got {}",
        r.rows.len()
    );
}

/// Numeric param: integer property matched via `WHERE n.age = $age`.
/// Guards against string-only coercion in the evaluator.
#[test]
fn param_numeric_where_clause_match_returns_row() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher("CREATE (:Person {age: 42})").unwrap();

    let mut params = HashMap::new();
    params.insert("age".to_string(), serde_json::json!(42));
    let r = engine
        .execute_cypher_with_params("MATCH (n) WHERE n.age = $age RETURN id(n)", params)
        .expect("MATCH with numeric WHERE param must succeed");

    assert_eq!(
        r.rows.len(),
        1,
        "expected exactly 1 row for numeric param match, got {}",
        r.rows.len()
    );
}

// ──────────── §7.5 Missing $param returns structured error (GH issue #3) ────
//
// A parameter referenced in a query but absent from the parameters map MUST
// produce a structured Err rather than silently coalescing to NULL. Callers
// must be able to distinguish "param missing" from "no rows matched".

/// Inline prop-map form: `MATCH (s {id: $id})` with an empty params map must
/// return `Err`, not `Ok` with 0 rows.
#[test]
fn param_missing_returns_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'alice'})")
        .unwrap();

    let result =
        engine.execute_cypher_with_params("MATCH (s {id: $id}) RETURN id(s)", HashMap::new());

    let err = result.expect_err("expected Err for missing $id param, got Ok");
    let msg = err.to_string();
    assert!(
        msg.contains("id"),
        "error message must name the missing parameter; got: {msg}"
    );
}

/// WHERE form: `MATCH (s) WHERE s.id = $id` with an empty params map must
/// return `Err`. A node must exist so the filter predicate is actually
/// evaluated — the missing-parameter error is raised lazily per row at
/// predicate-evaluation time (issue #3's reproducer ran against a populated
/// graph).
#[test]
fn param_missing_where_clause_returns_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'alice'})")
        .unwrap();

    let result = engine
        .execute_cypher_with_params("MATCH (s) WHERE s.id = $id RETURN id(s)", HashMap::new());

    assert!(
        result.is_err(),
        "expected Err for missing $id param in WHERE clause, got Ok"
    );
}

/// A param that IS explicitly bound to JSON `null` must NOT error — the value
/// is present in the map (bound-null is distinct from absent-key). The query
/// may return 0 rows because no node property equals null, but the execution
/// itself must succeed.
#[test]
fn param_bound_to_null_resolves_without_error() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine
        .execute_cypher("CREATE (:Person {id: 'alice'})")
        .unwrap();

    let mut params = HashMap::new();
    params.insert("id".to_string(), serde_json::Value::Null);

    // The query must not Err just because the bound value is null.
    let result =
        engine.execute_cypher_with_params("MATCH (s) WHERE s.id = $id RETURN id(s)", params);

    let rs = result.expect("bound-null param must not cause an error");
    // No node has id = null, so 0 rows is the correct result.
    assert_eq!(rs.rows.len(), 0);
}
