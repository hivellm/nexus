//! Write-path parity harness (rulebook task
//! `.rulebook/tasks/phase1_http-merge-rel-and-set-rel-parity/`, checklist
//! item 1.1–1.3). Exercises the PUBLIC `/cypher` HTTP handler
//! (`execute_cypher`, the same function `axum` dispatches requests to)
//! with a battery of write queries and asserts the ENGINE-correct
//! (Neo4j-correct) outcome. Every case re-reads the data via a *fresh*
//! `MATCH` after the write — the write response alone is not sufficient
//! evidence, since a query can report `error: None` while silently
//! persisting nothing (or the wrong thing).
//!
//! This harness is Step 0 of `docs/nexus/04-write-path-unification.md`:
//! the safety net that must be green (ignoring documented divergences)
//! before Step 2 reroutes HTTP writes to
//! `Engine::execute_cypher_with_params` and Step 4 deletes the
//! `write_ops.rs` fork. Cases marked `#[ignore]` below are expected to
//! start passing once that rerouting lands — un-ignore them then.
//!
//! # Known divergences (write_ops.rs fork vs. engine `write_exec.rs`)
//!
//! `handler.rs` routes a query to `execute/write_ops.rs::execute_create_or_merge`
//! whenever the (uppercased) query text starts with `CREATE` or `MERGE`;
//! every other write form (`MATCH ... SET`, `MATCH ... MERGE`, UNWIND-driven
//! writes, transaction commands) is routed to
//! `Engine::execute_cypher_with_params`, which drives the already-fixed
//! `write_exec.rs` interpreter. `write_ops.rs` reimplements CREATE/MERGE/SET/
//! REMOVE/DELETE against a node-only `variable_context: HashMap<String,
//! Vec<u64>>` and never learned relationship semantics:
//!
//! - **B1** — a `MERGE` pattern containing a relationship element
//!   (`MERGE (a:L1 {..})-[r:T]->(b:L2 {..})`) never creates the edge:
//!   `execute_create_or_merge`'s MERGE loop only matches
//!   `PatternElement::Node(_)`; `PatternElement::Relationship(_)` has no
//!   arm and is silently skipped (cases 10, 11).
//! - **B2** — `SET r.k = v` / `SET r += {..}` on a relationship variable is
//!   dropped when the enclosing query is routed to `write_ops.rs` (i.e. it
//!   starts with `CREATE`/`MERGE`): the SET handler looks up `target` in
//!   `variable_context`, which relationship patterns never populate,
//!   logging `WARN Variable r not found in context` and doing nothing.
//!   Queries that start with `MATCH` route around `write_ops.rs` entirely
//!   (case 12/13 verify which side of that line they land on).
//! - **B3** — `CREATE (a)-[r:T {w:5}]->(b) RETURN r.w` (same-statement
//!   projection) returns `null`: the RETURN projection's `PropertyAccess`
//!   branch resolves `variable` against the same node-only
//!   `variable_context`, so a relationship-variable property access always
//!   misses (case 8). The value is nonetheless stored correctly — a
//!   separate `MATCH` reads it back (case 9 keeps that as a green guard).
//!
//! While building this harness, four ADDITIONAL divergences were found
//! empirically that proposal.md did not anticipate — all in the ENGINE
//! (`nexus-core`), not `write_ops.rs`, so rerouting HTTP writes to the
//! engine (Step 2) will NOT fix them and, for B4, will actively regress a
//! form that works today:
//!
//! - **B4** — `SET target.prop = $param` via the `MATCH ... SET` engine
//!   path (`Engine::evaluate_set_expression` in `engine/match_exec.rs`)
//!   has no arm for `Expression::Parameter` and hard-codes it to
//!   `Value::Null` (case 5b). `write_ops.rs`'s own `SET` resolves
//!   `$param` correctly today via `expression_to_json_value` in
//!   `api/cypher/mod.rs` — so a combined `CREATE (n) SET n.x = $v`
//!   statement (routed to `write_ops.rs`) works, but the equivalent
//!   `MATCH (n) SET n.x = $v` (routed to the engine) does not. Deleting
//!   `write_ops.rs` without first fixing `evaluate_set_expression` would
//!   regress every parameterized SET.
//! - **B6** — `UNWIND $rows AS row` (a `$param`-bound list) errors out of
//!   `Engine::eval_write_value` / `expression_to_json_value`
//!   ("Complex expressions not supported in CREATE properties" — a
//!   misleading message since this fails before any CREATE/MERGE clause
//!   runs). Only a literal UNWIND list (`UNWIND [{...}] AS row`) is
//!   supported (case 7).
//! - **B7** — `CREATE (n {p:1}) REMOVE n.p` in ONE `write_ops.rs`-routed
//!   statement does not persist the removal; the equivalent two-statement
//!   form (`CREATE` then a separate `MATCH ... REMOVE`, which routes to
//!   the engine) removes the key correctly (case 6c).
//! - **B8** — `SET n.p = null` via the `MATCH ... SET` engine path stores
//!   a literal JSON `null` instead of removing the key
//!   (`write_exec.rs::apply_set_clause` has no null-removal special case),
//!   diverging from Neo4j semantics (case 5d).
//! - **B9** — a query whose first non-blank line is a `//` comment fails
//!   to PARSE at all (`Cypher syntax error: Query must contain at least
//!   one clause`, `executor/planner/queries/planner_core.rs:98`) before
//!   `handler.rs`'s routing heuristic is ever reached. A parser gap, not
//!   a `write_ops.rs`/routing bug (case 15).
//!
//! One more finding is worth flagging even though its case is GREEN
//! today: **case 17b** (`BEGIN`/`CREATE`/`ROLLBACK`) passes only because
//! `write_ops.rs`'s CREATE calls the low-level `Engine::create_node`
//! directly. `nexus-core` has its own `#[ignore]`d
//! `test_transaction_rollback_persists_across_queries` showing that the
//! SAME sequence via `Engine::execute_cypher` (the executor-driven CREATE
//! path Step 2 will reroute HTTP onto) does NOT roll back correctly. Case
//! 17b should be re-run immediately after Step 2 lands — it may flip red.
//!
//! Every `#[ignore]` below documents which class (or, where the actual
//! observed behaviour didn't match the proposal's hypothesis, the
//! concretely observed divergence) applies, per checklist item 1.3.

#![allow(unused_imports)]
use super::*;
use crate::NexusServer;
use nexus_core::auth::RoleBasedAccessControl;
use nexus_core::database::DatabaseManager;
use nexus_core::testing::TestContext;
use parking_lot::RwLock as PlRwLock;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Build a fresh, isolated `NexusServer` backed by a temp data dir.
/// Mirrors the construction pattern shared by every test in
/// `api/cypher/tests.rs` (e.g. `create_with_parameterized_node_property_persists`).
fn build_test_server(ctx: &TestContext) -> Arc<NexusServer> {
    let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
    let engine_arc = Arc::new(RwLock::new(engine));
    let executor = nexus_core::executor::Executor::default();
    let executor_arc = Arc::new(executor);
    let database_manager = DatabaseManager::new(ctx.path().join("databases")).unwrap();
    let database_manager_arc = Arc::new(PlRwLock::new(database_manager));
    let rbac = RoleBasedAccessControl::new();
    let rbac_arc = Arc::new(RwLock::new(rbac));
    let auth_config = nexus_core::auth::AuthConfig::default();
    let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));
    let jwt_config = nexus_core::auth::JwtConfig::default();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
            enabled: false,
            log_dir: std::path::PathBuf::from("./logs"),
            retention_days: 30,
            compress_logs: false,
        })
        .unwrap(),
    );
    Arc::new(NexusServer::new(
        executor_arc,
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager,
        jwt_manager,
        audit_logger,
        crate::config::RootUserConfig::default(),
    ))
}

/// Run a query through the PUBLIC `/cypher` HTTP handler (`execute_cypher`)
/// and return the deserialized response. Shared by every case in this
/// harness so each stays compact — no direct engine calls, no `write_ops`
/// internals.
async fn run_query(
    server: &Arc<NexusServer>,
    query: &str,
    params: HashMap<String, serde_json::Value>,
) -> CypherResponse {
    execute_cypher(
        State(server.clone()),
        None,
        Json(CypherRequest {
            query: query.to_string(),
            params,
            database: None,
        }),
    )
    .await
    .0
}

fn no_params() -> HashMap<String, serde_json::Value> {
    HashMap::new()
}

fn params(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn assert_no_error(resp: &CypherResponse, context: &str) {
    assert!(
        resp.error.is_none(),
        "{}: unexpected error: {:?}",
        context,
        resp.error
    );
}

/// Extract `row[0]` of `rows[0]` as an `i64`, panicking with useful context
/// on shape mismatches (empty rows, non-array row, non-numeric value).
fn first_i64(resp: &CypherResponse, context: &str) -> i64 {
    resp.rows
        .first()
        .unwrap_or_else(|| panic!("{}: no rows returned", context))
        .as_array()
        .unwrap_or_else(|| panic!("{}: row is not an array", context))
        .first()
        .unwrap_or_else(|| panic!("{}: row is empty", context))
        .as_i64()
        .unwrap_or_else(|| panic!("{}: value is not an integer", context))
}

// ---------------------------------------------------------------------
// 1. Node writes (expected GREEN today)
// ---------------------------------------------------------------------

#[tokio::test]
async fn case_01_create_node_literal_props_return() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(
        &server,
        "CREATE (n:Case1 {name: \"Alice\", age: 30}) RETURN n.name AS name, n.age AS age",
        no_params(),
    )
    .await;
    assert_no_error(&resp, "CREATE literal props");
    assert_eq!(resp.columns, vec!["name".to_string(), "age".to_string()]);
    let row = resp.rows[0].as_array().expect("row is array");
    assert_eq!(row[0].as_str(), Some("Alice"));
    assert_eq!(row[1].as_i64(), Some(30));

    // Fresh re-read — the response alone is not sufficient evidence.
    let read = run_query(
        &server,
        "MATCH (n:Case1) RETURN n.name AS name, n.age AS age",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after CREATE literal props");
    let row = read.rows[0].as_array().expect("row is array");
    assert_eq!(row[0].as_str(), Some("Alice"));
    assert_eq!(row[1].as_i64(), Some(30));
}

#[tokio::test]
async fn case_02_create_node_param_props_return_and_reread() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(
        &server,
        "CREATE (n:Case2 {x: $v}) RETURN n.x AS x",
        params(&[("v", serde_json::json!(9))]),
    )
    .await;
    assert_no_error(&resp, "CREATE $param props");
    assert_eq!(
        resp.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(9),
        "parameterized property must be 9 in the CREATE...RETURN response, not null"
    );

    let read = run_query(&server, "MATCH (n:Case2) RETURN n.x AS x", no_params()).await;
    assert_no_error(&read, "MATCH after $param CREATE");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(9),
        "parameterized property must persist as 9 across a fresh MATCH"
    );
}

#[tokio::test]
async fn case_03_merge_node_creates_then_idempotent() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let merge = "MERGE (n:Case3 {id: 1})";
    let first = run_query(&server, merge, no_params()).await;
    assert_no_error(&first, "first MERGE");
    let second = run_query(&server, merge, no_params()).await;
    assert_no_error(&second, "second (idempotent) MERGE");

    let count = run_query(&server, "MATCH (n:Case3) RETURN count(n) AS c", no_params()).await;
    assert_no_error(&count, "MATCH count after repeated MERGE");
    assert_eq!(
        first_i64(&count, "MERGE idempotency count"),
        1,
        "repeated MERGE on the same key must not duplicate the node"
    );
}

#[tokio::test]
async fn case_04_merge_node_on_create_and_on_match() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let merge =
        "MERGE (n:Case4 {id: 1}) ON CREATE SET n.created = true ON MATCH SET n.matched = true";

    // First run: node is absent, so ON CREATE fires.
    let first = run_query(&server, merge, no_params()).await;
    assert_no_error(&first, "MERGE ON CREATE branch");
    let after_create = run_query(
        &server,
        "MATCH (n:Case4 {id: 1}) RETURN n.created AS created, n.matched AS matched",
        no_params(),
    )
    .await;
    assert_no_error(&after_create, "MATCH after ON CREATE branch");
    let row = after_create.rows[0].as_array().expect("row is array");
    assert_eq!(
        row[0].as_bool(),
        Some(true),
        "ON CREATE SET must persist n.created = true"
    );
    assert!(
        row[1].is_null(),
        "ON MATCH SET must not have fired on the first (creating) run"
    );

    // Second run: node now exists, so ON MATCH fires instead.
    let second = run_query(&server, merge, no_params()).await;
    assert_no_error(&second, "MERGE ON MATCH branch");
    let after_match = run_query(
        &server,
        "MATCH (n:Case4 {id: 1}) RETURN n.created AS created, n.matched AS matched",
        no_params(),
    )
    .await;
    assert_no_error(&after_match, "MATCH after ON MATCH branch");
    let row = after_match.rows[0].as_array().expect("row is array");
    assert_eq!(
        row[0].as_bool(),
        Some(true),
        "n.created must remain true from the first run"
    );
    assert_eq!(
        row[1].as_bool(),
        Some(true),
        "ON MATCH SET must persist n.matched = true on the second run"
    );

    let count = run_query(&server, "MATCH (n:Case4) RETURN count(n) AS c", no_params()).await;
    assert_eq!(
        first_i64(&count, "Case4 node count"),
        1,
        "ON MATCH run must not create a second node"
    );
}

// Case 5 is split into one function per SET form: literal / map-merge /
// null-removal all go through the same `MATCH ... SET` engine path and are
// GREEN; the `$param` form is its own function because it hits **B4**
// (see the module doc comment) and is `#[ignore]`d on its own so it
// doesn't hide the other three green forms behind one failing test.

#[tokio::test]
async fn case_05a_set_literal_value_persists() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let create = run_query(&server, "CREATE (n:Case5a {id: 1})", no_params()).await;
    assert_no_error(&create, "baseline CREATE for Case5a");

    let set_literal = run_query(
        &server,
        "MATCH (n:Case5a {id: 1}) SET n.p = 'x'",
        no_params(),
    )
    .await;
    assert_no_error(&set_literal, "SET n.p = literal");
    let read = run_query(
        &server,
        "MATCH (n:Case5a {id: 1}) RETURN n.p AS p",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after SET n.p = literal");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_str(),
        Some("x"),
        "SET n.p = 'x' must persist"
    );
}

// **B4** — `SET target.prop = $param` via the `MATCH ... SET` engine path
// never resolves the parameter. `Engine::evaluate_set_expression`
// (`crates/nexus-core/src/engine/match_exec.rs`) has no arm for
// `Expression::Parameter`; it deliberately maps every parameter to
// `Value::Null` (see the comment directly above that match arm: "Parameter
// placeholders surface as NULL in this narrow evaluator — parameter-binding
// lives on the executor side"). This is an ENGINE-level gap, not a
// `write_ops.rs` bug — rerouting HTTP writes through the engine (Step 2)
// will NOT fix it, and will actually *regress* the combined
// `CREATE (n) SET n.x = $v` form, which today works because `write_ops.rs`
// resolves `$param` itself via `expression_to_json_value` in
// `api/cypher/mod.rs`. Flagging as a required companion fix for the
// unification work (see `create_with_parameterized_node_property_persists`-
// style tests in `api/cypher/tests.rs`, which only exercise the
// write_ops-routed combined-statement form and would start failing).
#[tokio::test]
#[ignore = "known-divergence B4: engine `evaluate_set_expression` resolves $param to Null in SET clauses (match_exec.rs); un-ignore once the engine SET evaluator threads self.current_params"]
async fn case_05b_set_param_value_persists() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let create = run_query(&server, "CREATE (n:Case5b {id: 1})", no_params()).await;
    assert_no_error(&create, "baseline CREATE for Case5b");

    let set_param = run_query(
        &server,
        "MATCH (n:Case5b {id: 1}) SET n.p2 = $v",
        params(&[("v", serde_json::json!(42))]),
    )
    .await;
    assert_no_error(&set_param, "SET n.p2 = $param");
    let read = run_query(
        &server,
        "MATCH (n:Case5b {id: 1}) RETURN n.p2 AS p2",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after SET n.p2 = $param");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(42),
        "SET n.p2 = $param must persist the parameterized value"
    );
}

#[tokio::test]
async fn case_05c_set_map_merge_persists_both_keys() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let create = run_query(&server, "CREATE (n:Case5c {id: 1})", no_params()).await;
    assert_no_error(&create, "baseline CREATE for Case5c");

    let set_merge = run_query(
        &server,
        "MATCH (n:Case5c {id: 1}) SET n += {a: 1, b: 2}",
        no_params(),
    )
    .await;
    assert_no_error(&set_merge, "SET n += {map}");
    let read = run_query(
        &server,
        "MATCH (n:Case5c {id: 1}) RETURN n.a AS a, n.b AS b",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after SET n += {map}");
    let row = read.rows[0].as_array().expect("row is array");
    assert_eq!(row[0].as_i64(), Some(1), "SET n += {{..}} must persist a");
    assert_eq!(row[1].as_i64(), Some(2), "SET n += {{..}} must persist b");
}

// **B8 (new, not in proposal.md)** — `SET n.p = null` via the `MATCH ...
// SET` engine path stores a literal JSON `null` (`state.properties.insert(
// property.clone(), json_value)` in `write_exec.rs::apply_set_clause` has
// no null-removal special case) instead of removing the key, diverging
// from Neo4j semantics (`SET x = null` removes the property). This is an
// engine-level gap independent of HTTP routing.
#[tokio::test]
#[ignore = "known-divergence B8: engine apply_set_clause stores literal null instead of removing the key on SET n.p = null"]
async fn case_05d_set_null_removes_key() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let create = run_query(
        &server,
        "CREATE (n:Case5d {id: 1, p: 'to-remove'})",
        no_params(),
    )
    .await;
    assert_no_error(&create, "baseline CREATE for Case5d");

    let set_null = run_query(
        &server,
        "MATCH (n:Case5d {id: 1}) SET n.p = null",
        no_params(),
    )
    .await;
    assert_no_error(&set_null, "SET n.p = null");
    let read = run_query(&server, "MATCH (n:Case5d {id: 1}) RETURN n", no_params()).await;
    assert_no_error(&read, "MATCH n after SET n.p = null");
    let node = read.rows[0].as_array().expect("row is array")[0]
        .as_object()
        .expect("RETURN n is a node object");
    assert!(
        !node.contains_key("p"),
        "SET n.p = null must remove the key p, not store a null value; node = {:?}",
        node
    );
}

#[tokio::test]
async fn case_06a_delete_node() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let delete = run_query(
        &server,
        "CREATE (n:Case6Delete {name: \"Bob\"}) DELETE n",
        no_params(),
    )
    .await;
    assert_no_error(&delete, "CREATE + DELETE n");
    let count_deleted = run_query(
        &server,
        "MATCH (n:Case6Delete) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&count_deleted, "Case6Delete count"),
        0,
        "DELETE n must remove the node"
    );
}

#[tokio::test]
async fn case_06b_detach_delete_with_relationship() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let seed = run_query(
        &server,
        "CREATE (a:Case6DetachA {x: 1})-[:Case6DetachRel]->(b:Case6DetachB {x: 2})",
        no_params(),
    )
    .await;
    assert_no_error(&seed, "seed a-[:REL]->b for DETACH DELETE");
    let detach = run_query(
        &server,
        "MATCH (a:Case6DetachA {x: 1}) DETACH DELETE a",
        no_params(),
    )
    .await;
    assert_no_error(&detach, "MATCH ... DETACH DELETE a");

    let a_count = run_query(
        &server,
        "MATCH (a:Case6DetachA) RETURN count(a) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&a_count, "Case6DetachA count"),
        0,
        "DETACH DELETE must remove node a"
    );
    let rel_count = run_query(
        &server,
        "MATCH ()-[r:Case6DetachRel]->() RETURN count(r) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&rel_count, "Case6DetachRel count"),
        0,
        "DETACH DELETE must remove the relationship attached to the deleted node"
    );
    let b_count = run_query(
        &server,
        "MATCH (b:Case6DetachB) RETURN count(b) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&b_count, "Case6DetachB count"),
        1,
        "DETACH DELETE on a must not remove the unrelated endpoint b"
    );
}

// **B7 (new, not in proposal.md)** — `CREATE (n {p:1}) REMOVE n.p` in ONE
// write_ops-routed statement does not persist the removal (empirically
// confirmed: a fresh `MATCH ... RETURN n` still shows `p: 1`). The
// equivalent two-statement form (separate `CREATE`, then a `MATCH ...
// REMOVE` that routes to the engine) removes the key correctly — proving
// this is specific to `write_ops.rs`'s combined-statement REMOVE handling,
// not a general engine defect. Will self-heal once Step 2/4 reroute HTTP
// writes through the engine.
#[tokio::test]
#[ignore = "known-divergence B7: write_ops.rs combined CREATE+REMOVE in one statement drops the removal; un-ignore after write-path unification"]
async fn case_06c_remove_property_combined_with_create() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let remove = run_query(
        &server,
        "CREATE (n:Case6Remove {p: 1}) REMOVE n.p",
        no_params(),
    )
    .await;
    assert_no_error(&remove, "CREATE + REMOVE n.p");
    let read = run_query(&server, "MATCH (n:Case6Remove) RETURN n", no_params()).await;
    assert_no_error(&read, "MATCH after CREATE + REMOVE n.p");
    let node = read.rows[0].as_array().expect("row is array")[0]
        .as_object()
        .expect("RETURN n is a node object");
    assert!(
        !node.contains_key("p"),
        "REMOVE n.p must remove the key; node = {:?}",
        node
    );
}

// **B6 (new, not in proposal.md)** — `UNWIND $rows AS row` (a
// `$param`-bound list) is entirely unsupported by the engine's UNWIND-write
// path: `Engine::eval_write_value` falls through to
// `expression_to_json_value`, which has no `Expression::Parameter` arm and
// errors with "Complex expressions not supported in CREATE properties" —
// even though this failure has nothing to do with CREATE (it happens while
// evaluating the UNWIND list expression itself, before any clause runs).
// Only a LITERAL UNWIND list (`UNWIND [{...}, {...}] AS row`) works today
// (see `crates/nexus-core/src/engine/tests/transactions.rs
// unwind_write_merge_persists_each_row`). This is a pre-existing engine
// limitation independent of HTTP routing — this query already goes
// through `Engine::execute_cypher_with_params` directly (the "has_unwind
// && has_write" branch in handler.rs), so rerouting HTTP writes to the
// engine will not change this behavior.
#[tokio::test]
#[ignore = "known-divergence B6: engine UNWIND-write path has no Expression::Parameter support for the UNWIND list itself; only literal UNWIND lists work"]
async fn case_07_unwind_merge_batch_upsert() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(
        &server,
        "UNWIND $rows AS row MERGE (n:Case7 {id: row.id})",
        params(&[("rows", serde_json::json!([{"id": 1}, {"id": 2}, {"id": 3}]))]),
    )
    .await;
    assert_no_error(&resp, "UNWIND + MERGE batch upsert");

    let count = run_query(&server, "MATCH (n:Case7) RETURN count(n) AS c", no_params()).await;
    assert_no_error(&count, "MATCH count after UNWIND + MERGE");
    assert_eq!(
        first_i64(&count, "Case7 batch upsert count"),
        3,
        "UNWIND + MERGE must persist all 3 distinct rows"
    );

    // Re-running the same batch must stay idempotent (MERGE semantics).
    let rerun = run_query(
        &server,
        "UNWIND $rows AS row MERGE (n:Case7 {id: row.id})",
        params(&[("rows", serde_json::json!([{"id": 1}, {"id": 2}, {"id": 3}]))]),
    )
    .await;
    assert_no_error(&rerun, "re-run UNWIND + MERGE batch upsert");
    let count2 = run_query(&server, "MATCH (n:Case7) RETURN count(n) AS c", no_params()).await;
    assert_eq!(
        first_i64(&count2, "Case7 batch upsert re-run count"),
        3,
        "re-running the UNWIND + MERGE batch must not duplicate rows"
    );
}

// ---------------------------------------------------------------------
// 2. Relationship writes (several expected RED today — see B1/B2/B3
//    above for the specific bug each `#[ignore]` references)
// ---------------------------------------------------------------------

#[tokio::test]
#[ignore = "known-divergence B3: write_ops.rs CREATE...RETURN projects a relationship-variable property as null; a separate MATCH reads the correct value (case 9)"]
async fn case_08_create_rel_return_property_same_statement() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(
        &server,
        "CREATE (a:Case8A)-[r:Case8T {w: 5}]->(b:Case8B) RETURN r.w AS w",
        no_params(),
    )
    .await;
    assert_no_error(&resp, "CREATE rel + same-statement RETURN r.w");
    assert_eq!(
        resp.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(5),
        "CREATE ... RETURN r.w must project the relationship property in the same statement"
    );

    // The value is stored correctly regardless — confirm via a fresh MATCH.
    let read = run_query(
        &server,
        "MATCH (:Case8A)-[r:Case8T]->(:Case8B) RETURN r.w AS w",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after CREATE rel with literal prop");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(5),
        "relationship property must persist as 5 regardless of the same-statement RETURN bug"
    );
}

#[tokio::test]
async fn case_09_create_rel_param_props_persist() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(
        &server,
        "CREATE (a:Case9A)-[r:Case9T {w: $w}]->(b:Case9B)",
        params(&[("w", serde_json::json!(8))]),
    )
    .await;
    assert_no_error(&resp, "CREATE rel with $param prop");

    let read = run_query(
        &server,
        "MATCH (:Case9A)-[r:Case9T]->(:Case9B) RETURN r.w AS w",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after CREATE rel with $param prop");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(8),
        "parameterized relationship property must persist as 8, not null"
    );
}

// **B1** — the MERGE loop in `write_ops.rs::execute_create_or_merge` only
// matches `PatternElement::Node(_)`; the `PatternElement::Relationship(_)`
// in a `MERGE (a:L {..})-[r:T]->(b:L2 {..})` path pattern is silently
// skipped, so no edge is ever created.
#[tokio::test]
#[ignore = "known-divergence B1: write_ops.rs MERGE loop skips PatternElement::Relationship, so the edge is never created"]
async fn case_10_merge_rel_creates_edge_idempotently() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let merge = "MERGE (a:Case10A {id: 1})-[r:Case10T]->(b:Case10B {id: 2})";
    let first = run_query(&server, merge, no_params()).await;
    assert_no_error(&first, "first MERGE rel");
    let count1 = run_query(
        &server,
        "MATCH ()-[r:Case10T]->() RETURN count(r) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&count1, "Case10T count after first MERGE"),
        1,
        "MERGE (a)-[r:T]->(b) must create exactly one edge"
    );

    let second = run_query(&server, merge, no_params()).await;
    assert_no_error(&second, "second (idempotent) MERGE rel");
    let count2 = run_query(
        &server,
        "MATCH ()-[r:Case10T]->() RETURN count(r) AS c",
        no_params(),
    )
    .await;
    assert_eq!(
        first_i64(&count2, "Case10T count after second MERGE"),
        1,
        "repeated MERGE on the same edge must not duplicate it"
    );
}

#[tokio::test]
#[ignore = "known-divergence B1: write_ops.rs MERGE loop skips PatternElement::Relationship, so inline relationship properties never persist"]
async fn case_11_merge_rel_inline_props_persist() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let merge = "MERGE (a:Case11A {id: 1})-[r:Case11T {w: 5}]->(b:Case11B {id: 2})";
    let resp = run_query(&server, merge, no_params()).await;
    assert_no_error(&resp, "MERGE rel with inline props");

    let read = run_query(
        &server,
        "MATCH ()-[r:Case11T]->() RETURN count(r) AS c, r.w AS w",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after MERGE rel with inline props");
    let row = read.rows[0].as_array().expect("row is array");
    assert_eq!(
        row[0].as_i64(),
        Some(1),
        "MERGE must create exactly one edge"
    );
    assert_eq!(
        row[1].as_i64(),
        Some(5),
        "MERGE (a)-[r:T {{w:5}}]->(b) must persist the inline property w=5"
    );
}

#[tokio::test]
async fn case_12_match_set_rel_property_forms() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let seed = run_query(
        &server,
        "CREATE (a:Case12A {id: 1})-[r:Case12T {k: 1}]->(b:Case12B {id: 1})",
        no_params(),
    )
    .await;
    assert_no_error(&seed, "seed a-[r:Case12T]->b");

    // SET r.k = <literal>
    let set_k = run_query(
        &server,
        "MATCH (a:Case12A)-[r:Case12T]->(b:Case12B) SET r.k = 9",
        no_params(),
    )
    .await;
    assert_no_error(&set_k, "MATCH ... SET r.k = 9");
    let read_k = run_query(
        &server,
        "MATCH ()-[r:Case12T]->() RETURN r.k AS k",
        no_params(),
    )
    .await;
    assert_no_error(&read_k, "MATCH after SET r.k = 9");
    assert_eq!(
        read_k.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(9),
        "MATCH ... SET r.k = 9 must persist on the relationship"
    );

    // SET r += {map}
    let set_merge = run_query(
        &server,
        "MATCH ()-[r:Case12T]->() SET r += {m: 1, n: 2}",
        no_params(),
    )
    .await;
    assert_no_error(&set_merge, "SET r += {map}");
    let read_merge = run_query(
        &server,
        "MATCH ()-[r:Case12T]->() RETURN r.m AS m, r.n AS n",
        no_params(),
    )
    .await;
    assert_no_error(&read_merge, "MATCH after SET r += {map}");
    let row = read_merge.rows[0].as_array().expect("row is array");
    assert_eq!(row[0].as_i64(), Some(1), "SET r += {{..}} must persist m");
    assert_eq!(row[1].as_i64(), Some(2), "SET r += {{..}} must persist n");

    // SET r.k = null removes the key.
    let set_null = run_query(
        &server,
        "MATCH ()-[r:Case12T]->() SET r.k = null",
        no_params(),
    )
    .await;
    assert_no_error(&set_null, "SET r.k = null");
    let read_null = run_query(&server, "MATCH ()-[r:Case12T]->() RETURN r", no_params()).await;
    assert_no_error(&read_null, "MATCH after SET r.k = null");
    let rel = read_null.rows[0].as_array().expect("row is array")[0]
        .as_object()
        .expect("RETURN r is a relationship object");
    assert!(
        !rel.contains_key("k"),
        "SET r.k = null must remove the key k, not store a null value; rel = {:?}",
        rel
    );
}

#[tokio::test]
async fn case_13_set_rel_via_anonymous_endpoints() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let seed = run_query(
        &server,
        "CREATE (:Case13A)-[r:Case13T {k: 0}]->(:Case13B)",
        no_params(),
    )
    .await;
    assert_no_error(&seed, "seed anonymous-endpoint relationship");

    let set_resp = run_query(&server, "MATCH ()-[r:Case13T]->() SET r.k = 1", no_params()).await;
    assert_no_error(&set_resp, "MATCH ()-[r]->() SET r.k = 1");

    let read = run_query(
        &server,
        "MATCH ()-[r:Case13T]->() RETURN r.k AS k",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after SET on anonymous-endpoint relationship");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(1),
        "SET r.k = 1 on an anonymous-endpoint relationship match must persist"
    );
}

// ---------------------------------------------------------------------
// 3. Mixed / routing-sensitive queries (handler.rs's uppercased
//    string-prefix heuristic decides write_ops.rs vs. engine routing;
//    these cases probe where that heuristic breaks down)
// ---------------------------------------------------------------------

#[tokio::test]
async fn case_14_match_then_create_relationship() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let seed_a = run_query(&server, "CREATE (a:Case14A {id: 1})", no_params()).await;
    assert_no_error(&seed_a, "seed node a");
    let seed_b = run_query(&server, "CREATE (b:Case14B {id: 1})", no_params()).await;
    assert_no_error(&seed_b, "seed node b");

    let resp = run_query(
        &server,
        "MATCH (a:Case14A {id: 1}), (b:Case14B {id: 1}) CREATE (a)-[r:Case14T]->(b)",
        no_params(),
    )
    .await;
    assert_no_error(&resp, "MATCH (a),(b) CREATE (a)-[r]->(b)");

    let count = run_query(
        &server,
        "MATCH ()-[r:Case14T]->() RETURN count(r) AS c",
        no_params(),
    )
    .await;
    assert_no_error(&count, "MATCH count after MATCH-then-CREATE");
    assert_eq!(
        first_i64(&count, "Case14T count"),
        1,
        "MATCH (a),(b) CREATE (a)-[r]->(b) must create exactly one edge between the matched nodes"
    );
}

// **B9 (new, not in proposal.md; NOT a write_ops.rs/routing bug — a
// PARSER-level gap)** — a query whose first non-blank line is a `//`
// comment fails to parse at all: `CypherParser::parse` returns
// `Cypher syntax error: Query must contain at least one clause`
// (`crates/nexus-core/src/executor/planner/queries/planner_core.rs:98`)
// before `handler.rs`'s uppercased-prefix routing heuristic is ever
// reached. This is unrelated to the CREATE/MERGE string-prefix routing
// this harness otherwise probes — it never gets that far.
#[tokio::test]
#[ignore = "known-divergence B9: CypherParser rejects a query whose first line is a `//` comment (\"Query must contain at least one clause\"); a parser gap, not a write_ops.rs/routing bug"]
async fn case_15_leading_comment_then_create() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    // The uppercased query text starts with "// C", not "CREATE" — probes
    // whether the leading-comment case still reaches a write path at all.
    let resp = run_query(
        &server,
        "// a leading comment\nCREATE (n:Case15 {x: 1})",
        no_params(),
    )
    .await;
    assert_no_error(&resp, "CREATE preceded by a leading comment line");

    let read = run_query(&server, "MATCH (n:Case15) RETURN n.x AS x", no_params()).await;
    assert_no_error(&read, "MATCH after comment-prefixed CREATE");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(1),
        "a leading comment before CREATE must not prevent the write from persisting"
    );
}

#[tokio::test]
async fn case_16_lowercase_create_keyword() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let resp = run_query(&server, "create (n:Case16 {x: 1})", no_params()).await;
    assert_no_error(&resp, "lowercase create keyword");

    let read = run_query(&server, "MATCH (n:Case16) RETURN n.x AS x", no_params()).await;
    assert_no_error(&read, "MATCH after lowercase-keyword CREATE");
    assert_eq!(
        read.rows[0].as_array().expect("row is array")[0].as_i64(),
        Some(1),
        "a lowercase `create` keyword must route and persist exactly like `CREATE`"
    );
}

// ---------------------------------------------------------------------
// 4. Transactions
// ---------------------------------------------------------------------

#[tokio::test]
async fn case_17a_begin_create_commit_is_visible() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let begin = run_query(&server, "BEGIN TRANSACTION", no_params()).await;
    assert_no_error(&begin, "BEGIN TRANSACTION");
    let create = run_query(&server, "CREATE (n:Case17Commit {x: 1})", no_params()).await;
    assert_no_error(&create, "CREATE inside open transaction");
    let commit = run_query(&server, "COMMIT TRANSACTION", no_params()).await;
    assert_no_error(&commit, "COMMIT TRANSACTION");

    let read = run_query(
        &server,
        "MATCH (n:Case17Commit) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after COMMIT");
    assert_eq!(
        first_i64(&read, "Case17Commit count after COMMIT"),
        1,
        "a CREATE committed inside BEGIN/COMMIT must be visible afterwards"
    );
}

// GREEN today, but for a subtle and important reason worth documenting:
// `nexus-core` has its own `#[ignore]`d test
// (`crates/nexus-core/tests/transaction_session_test.rs
// test_transaction_rollback_persists_across_queries`, "TODO: Fix rollback
// - nodes not being removed from index/storage") that reproduces exactly
// this BEGIN/CREATE/ROLLBACK sequence via `Engine::execute_cypher`
// (the executor-driven CREATE path) and finds the node NOT removed by
// ROLLBACK. This case passes here because `write_ops.rs`'s CREATE calls
// the low-level `Engine::create_node` directly — a different code path
// that happens to participate correctly in the storage-level rollback.
// **Regression risk for write-path unification Step 2**: once HTTP CREATE
// is rerouted through `Engine::execute_cypher_with_params` (the same
// executor-driven path the core-engine test already shows to be buggy),
// this case may start failing. Re-run it immediately after Step 2 lands.
#[tokio::test]
async fn case_17b_begin_create_rollback_is_not_visible() {
    let ctx = TestContext::new();
    let server = build_test_server(&ctx);

    let begin = run_query(&server, "BEGIN TRANSACTION", no_params()).await;
    assert_no_error(&begin, "BEGIN TRANSACTION");
    let create = run_query(&server, "CREATE (n:Case17Rollback {x: 1})", no_params()).await;
    assert_no_error(&create, "CREATE inside open transaction");
    let rollback = run_query(&server, "ROLLBACK TRANSACTION", no_params()).await;
    assert_no_error(&rollback, "ROLLBACK TRANSACTION");

    let read = run_query(
        &server,
        "MATCH (n:Case17Rollback) RETURN count(n) AS c",
        no_params(),
    )
    .await;
    assert_no_error(&read, "MATCH after ROLLBACK");
    assert_eq!(
        first_i64(&read, "Case17Rollback count after ROLLBACK"),
        0,
        "a CREATE rolled back inside BEGIN/ROLLBACK must not be visible afterwards"
    );
}
