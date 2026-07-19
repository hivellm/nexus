//! Regression coverage for issue #29: `write_exec.rs` (MERGE, CREATE+SET,
//! and UNWIND+CREATE) silently discarded the reserved `_id` external-id
//! slot. The existing `cypher_external_id.rs` suite only exercises
//! standalone `CREATE`, which routes through the executor operator and
//! never broke — these tests close that hole for the write-query paths
//! and restore the `RETURN n._id` / `WHERE n._id = ...` projection
//! coverage that was deliberately dropped (see the comment at
//! `cypher_external_id.rs:47-53`) as flaky against the process-wide
//! shared catalog. Every test here uses an isolated per-test catalog via
//! `Engine::with_isolated_catalog` instead.

use nexus_core::Engine;
use nexus_core::catalog::external_id::ExternalId;
use nexus_core::testing::TestContext;
use std::collections::HashMap;
use std::str::FromStr;

const SHA256_ZEROS: &str =
    "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// Look up the internal node id mapped to `ext_str` directly through the
/// catalog's external-id reverse index (bypassing Cypher projection
/// entirely), mirroring the pattern in `cypher_external_id.rs`.
fn internal_id_for(engine: &Engine, ext_str: &str) -> Option<u64> {
    let ext = ExternalId::from_str(ext_str).expect("valid external id");
    let txn = engine.catalog.read_txn().expect("open catalog read txn");
    engine
        .catalog
        .external_id_index()
        .get_internal(&txn, &ext)
        .expect("index lookup")
}

/// Count nodes carrying `label` — used to assert MERGE does not create a
/// duplicate on the second application.
fn count_label(engine: &mut Engine, label: &str) -> u64 {
    let q = format!("MATCH (n:{label}) RETURN count(n) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

// ── Group B: the write forms that were actually broken ─────────────────

// NOTE on verification routes: `RETURN n._id` immediately in the SAME
// statement as the MERGE/CREATE that assigns it does NOT go through
// `Executor::evaluate_projection_expression` (the `_id` -> catalog
// reverse-map special case at
// `executor/eval/projection/core.rs:90-104`). Write-query clauses
// (MERGE, CREATE combined with SET/REMOVE/RETURN, UNWIND+CREATE) build
// their RETURN result via `Engine::build_return_result`
// (`engine/write_exec.rs:1669`), whose `PropertyAccess` arm
// (`:1746-1769`) reads the raw stored-property map and has no `_id`
// special case at all — `map.get("_id")` is always `None` because `_id`
// is deliberately stripped out of the stored property map by the parser
// and kept only in the catalog's external-id index. Empirically:
// `MERGE (n:Widget {_id:'str:x'}) RETURN n._id` in ONE statement
// projects `Null` even though the external id is correctly registered in
// the catalog. This is a genuine, separate defect from issue #29 (which
// fixed *persistence*, not *same-statement projection*) — see the full
// write-up in the test-run report. Every test below therefore verifies
// the Cypher-projection route with a SEPARATE follow-up query (which
// exercises the correct, executor-side projection path), in addition to
// the direct catalog-index check.
#[test]
fn merge_with_underscore_id_persists_external_id_and_is_queryable() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (n:Widget {_id: 'str:x', name: 'gizmo'})")
        .expect("MERGE with _id must succeed");

    // Route 1: the catalog reverse index — authoritative check that the
    // external id actually landed.
    let internal_id = internal_id_for(&engine, "str:x");
    assert!(
        internal_id.is_some(),
        "MERGE must register 'str:x' in the external-id index"
    );

    // Route 2: a SEPARATE follow-up Cypher query projecting `n._id`
    // through the normal (working) executor read path.
    let res = engine
        .execute_cypher("MATCH (n:Widget {name: 'gizmo'}) RETURN n._id")
        .expect("follow-up MATCH must succeed");
    assert_eq!(res.rows.len(), 1, "expected exactly one matching node");
    assert_eq!(
        res.rows[0].values[0].as_str(),
        Some("str:x"),
        "RETURN n._id on a follow-up query must project the id MERGE persisted, \
         got {:?} (a null here means _id was silently dropped by MERGE)",
        res.rows[0].values[0]
    );

    // Route 3: filter by `_id` via WHERE and confirm it resolves back to
    // the same internal node.
    let res2 = engine
        .execute_cypher("MATCH (n:Widget) WHERE n._id = 'str:x' RETURN n.name")
        .expect("WHERE n._id filter must succeed");
    assert_eq!(
        res2.rows.len(),
        1,
        "WHERE n._id = 'str:x' must match exactly the merged node"
    );
    assert_eq!(res2.rows[0].values[0].as_str(), Some("gizmo"));
}

#[test]
fn merge_with_same_underscore_id_twice_matches_existing_node_no_duplicate() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (n:Widget {_id: 'str:x', name: 'gizmo'})")
        .expect("first MERGE");
    let first_internal_id =
        internal_id_for(&engine, "str:x").expect("first MERGE must register the external id");

    engine
        .execute_cypher("MERGE (n:Widget {_id: 'str:x', name: 'gizmo'})")
        .expect("second MERGE with the same _id must not error");
    let second_internal_id = internal_id_for(&engine, "str:x")
        .expect("external id must still be registered after the second MERGE");

    assert_eq!(
        first_internal_id, second_internal_id,
        "second MERGE on the same _id must resolve to the SAME internal node, not create a new one"
    );
    assert_eq!(
        count_label(&mut engine, "Widget"),
        1,
        "second MERGE with the same _id must not create a duplicate node"
    );
}

#[test]
fn create_then_set_in_same_statement_preserves_underscore_id() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // CREATE combined with SET in the SAME statement routes through
    // `execute_write_query`'s linear CREATE arm (engine/write_exec.rs:108-149)
    // rather than the executor's standalone-CREATE operator.
    engine
        .execute_cypher("CREATE (n:Doc {_id: 'str:doc-1'}) SET n.k = 1")
        .expect("CREATE + SET with _id");

    // Route 1: catalog reverse index.
    let internal_id = internal_id_for(&engine, "str:doc-1");
    assert!(
        internal_id.is_some(),
        "CREATE+SET must register 'str:doc-1' in the external-id index"
    );

    // Route 2: follow-up query confirms both the external id AND the SET
    // property landed on the SAME node.
    let res = engine
        .execute_cypher("MATCH (n:Doc) WHERE n._id = 'str:doc-1' RETURN n.k")
        .expect("follow-up WHERE n._id query");
    assert_eq!(res.rows.len(), 1);
    assert_eq!(
        res.rows[0].values[0].as_i64(),
        Some(1),
        "the node found via n._id must be the one CREATE+SET wrote k=1 on"
    );
}

#[test]
fn unwind_create_with_literal_underscore_id_sets_external_id() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // A single-row UNWIND: the literal `_id` is resolved once
    // (engine/write_exec.rs:405-412) and consumed by the one node this
    // row's CREATE produces (engine/write_exec.rs:443-483).
    engine
        .execute_cypher("UNWIND ['gizmo'] AS name CREATE (n:Row {_id: 'str:unwind-x', name: name})")
        .expect("UNWIND + CREATE with _id");

    let internal_id = internal_id_for(&engine, "str:unwind-x");
    assert!(
        internal_id.is_some(),
        "UNWIND+CREATE must register 'str:unwind-x' in the external-id index"
    );

    let res = engine
        .execute_cypher("MATCH (n:Row) WHERE n._id = 'str:unwind-x' RETURN n.name")
        .expect("follow-up WHERE n._id query");
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0].values[0].as_str(), Some("gizmo"));
}

#[test]
fn unwind_create_with_per_row_underscore_id_is_a_parse_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Per-row `_id` (`_id: row.id`) is out of scope by design: the parser
    // only accepts a string literal or a `$parameter` for `_id`
    // (executor/parser/clauses/mod.rs:47-54); a PropertyAccess like
    // `row.id` is rejected at parse time, well before the UNWIND loop
    // runs. Pin that this fails cleanly rather than silently creating
    // nodes with a null/garbage id.
    let res = engine.execute_cypher(
        "UNWIND [{id: 'str:a'}, {id: 'str:b'}] AS row CREATE (n:Row {_id: row.id})",
    );
    assert!(
        res.is_err(),
        "per-row _id (_id: row.id) must be rejected, not silently accepted"
    );
}

// ── Group A: projection and filtering (restored deleted coverage) ──────

#[test]
fn create_with_underscore_id_then_return_projects_external_id() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let create_q = format!("CREATE (n:File {{_id: '{}', name: 'a.txt'}})", SHA256_ZEROS);
    engine.execute_cypher(&create_q).expect("CREATE with _id");

    let res = engine
        .execute_cypher("MATCH (n:File {name: 'a.txt'}) RETURN n._id")
        .expect("follow-up MATCH must succeed");
    assert_eq!(res.rows.len(), 1);
    assert_eq!(
        res.rows[0].values[0].as_str(),
        Some(SHA256_ZEROS),
        "RETURN n._id must project the persisted external id, got {:?}",
        res.rows[0].values[0]
    );
}

#[test]
fn where_underscore_id_matches_correct_node_and_not_others() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:File {_id: 'str:node-alpha', name: 'alpha'})")
        .expect("create alpha");
    engine
        .execute_cypher("CREATE (:File {_id: 'str:node-beta', name: 'beta'})")
        .expect("create beta");

    let alpha = engine
        .execute_cypher("MATCH (n:File) WHERE n._id = 'str:node-alpha' RETURN n.name")
        .expect("filter by alpha id");
    assert_eq!(
        alpha.rows.len(),
        1,
        "WHERE n._id = 'str:node-alpha' must match exactly one node"
    );
    assert_eq!(alpha.rows[0].values[0].as_str(), Some("alpha"));

    let beta = engine
        .execute_cypher("MATCH (n:File) WHERE n._id = 'str:node-beta' RETURN n.name")
        .expect("filter by beta id");
    assert_eq!(beta.rows.len(), 1);
    assert_eq!(
        beta.rows[0].values[0].as_str(),
        Some("beta"),
        "WHERE n._id = 'str:node-beta' must not match the alpha node"
    );
}

#[test]
fn node_created_without_underscore_id_projects_null() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:File {name: 'no-id.txt'})")
        .expect("create without _id");

    let res = engine
        .execute_cypher("MATCH (n:File {name: 'no-id.txt'}) RETURN n._id")
        .expect("follow-up MATCH must succeed");
    assert_eq!(res.rows.len(), 1);
    assert!(
        res.rows[0].values[0].is_null(),
        "a node created without _id must project NULL for n._id, got {:?} \
         (proves absent means null, not a broken projection)",
        res.rows[0].values[0]
    );
}

// ── Group C: error propagation (silent-drop is what hid this bug) ──────

#[test]
fn merge_with_unprefixed_underscore_id_surfaces_clear_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // A bare, unprefixed string is syntactically a valid string literal
    // (passes the parser's "string literal or parameter" check), but
    // `ExternalId::from_str` rejects it for lacking a recognised
    // `blake3:`/`sha256:`/`sha512:`/`uuid:`/`str:`/`bytes:` prefix
    // (engine/write_exec.rs:57-58 -> Engine::resolve_external_id). This
    // must surface as a distinct, descriptive error — not silently
    // succeed with the id dropped.
    let res = engine.execute_cypher("MERGE (n:Bad {_id: 'no-prefix'})");
    let err = res.expect_err("_id without a recognised prefix must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid _id") && msg.contains("no-prefix"),
        "expected a descriptive 'invalid _id' error naming the bad value, got: {msg}"
    );

    // No node must have been created with the bad id anywhere in the
    // catalog: querying the label back must be empty.
    assert_eq!(
        count_label(&mut engine, "Bad"),
        0,
        "a MERGE that fails on an invalid _id must not leave a partially-created node behind"
    );
}

#[test]
fn merge_with_underscore_id_param_resolves_and_persists() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let mut params = HashMap::new();
    params.insert(
        "eid".to_string(),
        serde_json::Value::String("str:via-param".to_string()),
    );
    engine
        .execute_cypher_with_params("MERGE (n:Param {_id: $eid, name: 'p'})", params)
        .expect("MERGE with $param _id must succeed");

    let internal_id = internal_id_for(&engine, "str:via-param");
    assert!(
        internal_id.is_some(),
        "MERGE must register the parameter-supplied external id in the catalog"
    );

    let res = engine
        .execute_cypher("MATCH (n:Param) WHERE n._id = 'str:via-param' RETURN n.name")
        .expect("follow-up WHERE n._id query");
    assert_eq!(res.rows.len(), 1);
    assert_eq!(res.rows[0].values[0].as_str(), Some("p"));
}

#[test]
fn merge_with_missing_underscore_id_param_surfaces_clear_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // No `eid` param supplied at all.
    let res = engine.execute_cypher_with_params("MERGE (n:Param {_id: $eid})", HashMap::new());
    let err = res.expect_err("a missing _id parameter must be rejected, not treated as absent");
    let msg = err.to_string();
    assert!(
        msg.contains("eid") && msg.contains("not provided"),
        "expected a clear 'parameter not provided' error naming `eid`, got: {msg}"
    );
    assert_eq!(
        count_label(&mut engine, "Param"),
        0,
        "a MERGE that fails resolving its _id parameter must not create a node"
    );
}
