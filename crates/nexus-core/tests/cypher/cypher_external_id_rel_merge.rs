//! Regression coverage for task 5.2 of phase14_fix-external-id-write-path:
//! per-node `_id` (reserved external-id) support on relationship-`MERGE`
//! patterns (`MERGE (a {_id:...})-[r:T]->(b {_id:...})`).
//!
//! Companion to `cypher_external_id_write_paths.rs` (single-node write
//! paths); this file exercises `Engine::merge_single_node` /
//! `Engine::process_merge_relationship`
//! (`crates/nexus-core/src/engine/write_exec.rs`) — the endpoint-resolution
//! path used when a relationship pattern's node endpoints carry their own
//! `_id`, plus the parser gating
//! (`extract_underscore_id_from_pattern`, `executor/parser/clauses/mod.rs`)
//! that still forbids a second `_id` in a plain CREATE pattern.
//!
//! As with the sibling file, `RETURN n._id`/`WHERE n._id = ...` in the SAME
//! statement as the MERGE that assigns it does not project (see the
//! detailed note in `cypher_external_id_write_paths.rs`); every test here
//! verifies persistence via the catalog's external-id reverse index
//! (`internal_id_for`) and/or a SEPARATE follow-up query.
//!
//! Each test uses distinct labels (unique across the whole file) since a
//! label-only pattern with no properties matches ANY existing node of that
//! label once the ext-id lookup misses — shared labels would make
//! assertions ambiguous.

use nexus_core::Engine;
use nexus_core::catalog::external_id::ExternalId;
use nexus_core::testing::TestContext;
use std::collections::HashMap;
use std::str::FromStr;

/// Look up the internal node id mapped to `ext_str` directly through the
/// catalog's external-id reverse index (bypassing Cypher projection
/// entirely), mirroring `cypher_external_id_write_paths.rs`.
fn internal_id_for(engine: &Engine, ext_str: &str) -> Option<u64> {
    let ext = ExternalId::from_str(ext_str).expect("valid external id");
    let txn = engine.catalog.read_txn().expect("open catalog read txn");
    engine
        .catalog
        .external_id_index()
        .get_internal(&txn, &ext)
        .expect("index lookup")
}

/// Count nodes carrying `label`.
fn count_label(engine: &mut Engine, label: &str) -> u64 {
    let q = format!("MATCH (n:{label}) RETURN count(n) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

/// Count relationships of `rel_type` directly connecting a node with
/// `src_label` to a node with `dst_label`.
fn count_rel_between(engine: &mut Engine, src_label: &str, rel_type: &str, dst_label: &str) -> u64 {
    let q = format!("MATCH (a:{src_label})-[r:{rel_type}]->(b:{dst_label}) RETURN count(r) AS c");
    let r = engine.execute_cypher(&q).expect("count query");
    r.rows[0].values[0].as_u64().unwrap_or(u64::MAX)
}

#[test]
fn merge_relationship_with_underscore_id_on_both_endpoints_creates_and_registers_both() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "MERGE (a:RmA1 {_id: 'str:rm1-a', name: 'alpha'})-[r:T]->(b:RmB1 {_id: 'str:rm1-b', name: 'beta'})",
        )
        .expect("relationship MERGE with _id on both endpoints must succeed");

    // Both external ids landed in the catalog index.
    let a_id = internal_id_for(&engine, "str:rm1-a");
    let b_id = internal_id_for(&engine, "str:rm1-b");
    assert!(a_id.is_some(), "endpoint a's _id must be registered");
    assert!(b_id.is_some(), "endpoint b's _id must be registered");
    assert_ne!(a_id, b_id, "the two endpoints must be distinct nodes");

    // Both endpoints are queryable via a follow-up n._id projection.
    let res_a = engine
        .execute_cypher("MATCH (n:RmA1) WHERE n._id = 'str:rm1-a' RETURN n.name")
        .expect("follow-up query for a");
    assert_eq!(res_a.rows.len(), 1);
    assert_eq!(res_a.rows[0].values[0].as_str(), Some("alpha"));

    let res_b = engine
        .execute_cypher("MATCH (n:RmB1) WHERE n._id = 'str:rm1-b' RETURN n.name")
        .expect("follow-up query for b");
    assert_eq!(res_b.rows.len(), 1);
    assert_eq!(res_b.rows[0].values[0].as_str(), Some("beta"));

    // The relationship connects the two endpoints.
    assert_eq!(
        count_rel_between(&mut engine, "RmA1", "T", "RmB1"),
        1,
        "expected exactly one T relationship between the merged endpoints"
    );
}

#[test]
fn merge_relationship_with_same_underscore_ids_twice_matches_existing_nodes_and_relationship() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let stmt = "MERGE (a:RmA2 {_id: 'str:rm2-a'})-[r:T]->(b:RmB2 {_id: 'str:rm2-b'})";

    engine.execute_cypher(stmt).expect("first MERGE");
    let first_a = internal_id_for(&engine, "str:rm2-a").expect("a registered after first MERGE");
    let first_b = internal_id_for(&engine, "str:rm2-b").expect("b registered after first MERGE");

    engine
        .execute_cypher(stmt)
        .expect("second identical MERGE must not error");
    let second_a =
        internal_id_for(&engine, "str:rm2-a").expect("a still registered after second MERGE");
    let second_b =
        internal_id_for(&engine, "str:rm2-b").expect("b still registered after second MERGE");

    assert_eq!(
        first_a, second_a,
        "second MERGE must resolve endpoint a to the SAME internal node"
    );
    assert_eq!(
        first_b, second_b,
        "second MERGE must resolve endpoint b to the SAME internal node"
    );
    assert_eq!(
        count_label(&mut engine, "RmA2"),
        1,
        "second MERGE must not create a duplicate a node"
    );
    assert_eq!(
        count_label(&mut engine, "RmB2"),
        1,
        "second MERGE must not create a duplicate b node"
    );
    assert_eq!(
        count_rel_between(&mut engine, "RmA2", "T", "RmB2"),
        1,
        "second MERGE must not create a duplicate relationship"
    );
}

#[test]
fn merge_relationship_with_underscore_id_on_single_endpoint_only() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("MERGE (a:RmA3 {_id: 'str:rm3-a'})-[r:T]->(b:RmB3 {name: 'plain'})")
        .expect("relationship MERGE with _id on one endpoint must succeed");

    // a's external id is registered.
    let a_id = internal_id_for(&engine, "str:rm3-a");
    assert!(a_id.is_some(), "endpoint a's _id must be registered");

    // b was created as a plain node — it has no external id.
    let res_b = engine
        .execute_cypher("MATCH (n:RmB3 {name: 'plain'}) RETURN n._id")
        .expect("follow-up query for b");
    assert_eq!(res_b.rows.len(), 1, "exactly one plain b node expected");
    assert!(
        res_b.rows[0].values[0].is_null(),
        "b was not given an _id, so n._id must project NULL, got {:?}",
        res_b.rows[0].values[0]
    );

    assert_eq!(
        count_rel_between(&mut engine, "RmA3", "T", "RmB3"),
        1,
        "expected exactly one T relationship between a and b"
    );
}

#[test]
fn merge_relationship_reuses_existing_node_matched_by_underscore_id_instead_of_creating_new() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Pre-create the endpoint via a plain CREATE.
    engine
        .execute_cypher("CREATE (n:RmA4 {_id: 'str:rm4-x', name: 'old'})")
        .expect("pre-create endpoint a");
    let pre_existing_id =
        internal_id_for(&engine, "str:rm4-x").expect("pre-created node registered");

    // A relationship-MERGE whose src endpoint carries the same _id must
    // reuse this node, not create a new one.
    engine
        .execute_cypher("MERGE (a:RmA4 {_id: 'str:rm4-x'})-[r:T]->(b:RmB4 {name: 'b'})")
        .expect("relationship MERGE matching by existing _id must succeed");

    let after_merge_id =
        internal_id_for(&engine, "str:rm4-x").expect("node still registered after MERGE");
    assert_eq!(
        pre_existing_id, after_merge_id,
        "relationship MERGE must resolve the src endpoint to the SAME pre-existing internal node"
    );
    assert_eq!(
        count_label(&mut engine, "RmA4"),
        1,
        "relationship MERGE must not create a duplicate a node when matched by _id"
    );

    // The pre-existing node's properties were not touched by the MERGE
    // pattern (it only specified `_id`, no other props).
    let res = engine
        .execute_cypher("MATCH (n:RmA4) WHERE n._id = 'str:rm4-x' RETURN n.name")
        .expect("follow-up query");
    assert_eq!(res.rows.len(), 1);
    assert_eq!(
        res.rows[0].values[0].as_str(),
        Some("old"),
        "the reused node's pre-existing property must be preserved, not overwritten"
    );

    assert_eq!(
        count_rel_between(&mut engine, "RmA4", "T", "RmB4"),
        1,
        "expected the relationship to be created against the reused node"
    );
}

#[test]
fn merge_relationship_with_endpoint_already_bound_by_prior_match_ignores_underscore_id_semantics() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Pre-create the src endpoint with no external id at all.
    engine
        .execute_cypher("CREATE (:RmA5 {name: 'exists'})")
        .expect("pre-create src endpoint");

    // A prior MATCH binds `a`; the relationship-MERGE reuses the bound
    // node directly (context short-circuit in
    // `process_merge_relationship`), never touching `_id` resolution for
    // `a`. Only the dst endpoint carries `_id`.
    engine
        .execute_cypher(
            "MATCH (a:RmA5 {name: 'exists'}) MERGE (a)-[r:T]->(b:RmB5 {_id: 'str:rm5-b'})",
        )
        .expect("relationship MERGE with a bound src endpoint must succeed");

    assert_eq!(
        count_label(&mut engine, "RmA5"),
        1,
        "the bound src endpoint must not be duplicated by the relationship MERGE"
    );

    let b_id = internal_id_for(&engine, "str:rm5-b");
    assert!(
        b_id.is_some(),
        "the dst endpoint's _id must still be registered when src is bound via a prior MATCH"
    );

    assert_eq!(
        count_rel_between(&mut engine, "RmA5", "T", "RmB5"),
        1,
        "expected exactly one T relationship from the bound a to the merged b"
    );
}

#[test]
fn merge_relationship_with_unprefixed_underscore_id_on_endpoint_surfaces_clear_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Src endpoint resolution runs before dst endpoint resolution
    // (`process_merge_relationship`), so an invalid src `_id` must fail
    // before dst is even attempted — neither node nor the relationship
    // should be left behind.
    let res = engine.execute_cypher("MERGE (a:RmA6 {_id: 'bogus'})-[r:T]->(b:RmB6 {name: 'x'})");
    let err = res.expect_err("an unprefixed _id on a relationship-MERGE endpoint must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid _id") && msg.contains("bogus"),
        "expected a descriptive 'invalid _id' error naming the bad value, got: {msg}"
    );

    assert_eq!(
        count_label(&mut engine, "RmA6"),
        0,
        "a relationship MERGE that fails on an invalid src _id must not leave a partial a node behind"
    );
    assert_eq!(
        count_label(&mut engine, "RmB6"),
        0,
        "a relationship MERGE that fails on an invalid src _id must not create the dst node either"
    );
}

#[test]
fn merge_relationship_with_underscore_id_param_on_endpoint_resolves_and_persists() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let mut params = HashMap::new();
    params.insert(
        "eid".to_string(),
        serde_json::Value::String("str:rm7-a".to_string()),
    );
    engine
        .execute_cypher_with_params(
            "MERGE (a:RmA7 {_id: $eid})-[r:T]->(b:RmB7 {name: 'p'})",
            params,
        )
        .expect("relationship MERGE with $param _id on an endpoint must succeed");

    let a_id = internal_id_for(&engine, "str:rm7-a");
    assert!(
        a_id.is_some(),
        "the parameter-supplied endpoint external id must be registered in the catalog"
    );

    assert_eq!(
        count_rel_between(&mut engine, "RmA7", "T", "RmB7"),
        1,
        "expected the relationship to be created against the param-resolved endpoint"
    );
}

#[test]
fn merge_relationship_with_missing_underscore_id_param_on_endpoint_surfaces_clear_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    let res = engine.execute_cypher_with_params(
        "MERGE (a:RmA7b {_id: $eid})-[r:T]->(b:RmB7b {name: 'p'})",
        HashMap::new(),
    );
    let err = res.expect_err(
        "a missing _id parameter on a relationship-MERGE endpoint must be rejected, not treated as absent",
    );
    let msg = err.to_string();
    assert!(
        msg.contains("eid") && msg.contains("not provided"),
        "expected a clear 'parameter not provided' error naming `eid`, got: {msg}"
    );
    assert_eq!(
        count_label(&mut engine, "RmA7b"),
        0,
        "a relationship MERGE that fails resolving its src _id parameter must not create a node"
    );
    assert_eq!(
        count_label(&mut engine, "RmB7b"),
        0,
        "a relationship MERGE that fails resolving its src _id parameter must not create the dst node either"
    );
}

#[test]
fn create_relationship_pattern_with_underscore_id_on_both_endpoints_is_still_a_parse_error() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // CREATE (unlike MERGE) still only reads the clause-level `_id` slot;
    // a second `_id` on a CREATE pattern's other endpoint must remain a
    // parse-time rejection so a future refactor can't silently start
    // dropping the second `_id` in CREATE the way it once dropped it
    // entirely.
    let res = engine
        .execute_cypher("CREATE (a:RmA8 {_id: 'str:rm8-a'})-[r:T]->(b:RmB8 {_id: 'str:rm8-b'})");
    let err = res.expect_err("a second _id on a CREATE relationship pattern must be a parse error");
    let msg = err.to_string();
    assert!(
        msg.contains("may only appear once"),
        "expected the '_id may only appear once' parse error, got: {msg}"
    );

    assert_eq!(
        count_label(&mut engine, "RmA8"),
        0,
        "a CREATE that fails to parse must not create any node"
    );
    assert_eq!(count_label(&mut engine, "RmB8"), 0);
}
