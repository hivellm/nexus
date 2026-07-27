//! `WHERE id(var) = <value>` in a write query.
//!
//! The write executor rejected every `WHERE` clause ("Unsupported clause
//! in write query"), so the natural "update/delete an entity by id" shape
//! a client emits — `MATCH ()-[r]->() WHERE id(r) = $x SET r.k = v` — could
//! never run (it blocked the Rust SDK's `update_relationship`). The write
//! path now applies the `id(var) = <value>` predicate: it binds an
//! otherwise-unbound relationship (the untyped `()-[r]->()` the pattern
//! binder skips) by id, or narrows an already-bound variable to that id.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use serde_json::Value;
use std::collections::HashMap;

fn eng() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let e = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (e, ctx)
}

#[test]
fn update_untyped_relationship_by_id() {
    let (mut e, _c) = eng();
    let rs = e
        .execute_cypher("CREATE (a:X)-[r:R {w: 1}]->(b:X) RETURN id(r) AS rid")
        .expect("create");
    let rid = rs.rows[0].values[0].as_i64().expect("rid");

    let mut params = HashMap::new();
    params.insert("rid".to_string(), Value::from(rid));
    // The exact shape the SDK's `update_relationship` emits.
    e.execute_cypher_with_params(
        "MATCH ()-[r]->() WHERE id(r) = $rid SET r.w = 2 RETURN r",
        params,
    )
    .expect("update-by-id must succeed, not reject WHERE");

    let check = e
        .execute_cypher("MATCH ()-[r:R]->() RETURN r.w AS w")
        .expect("read back");
    assert_eq!(
        check.rows[0].values[0].as_i64(),
        Some(2),
        "the relationship's property must be updated"
    );
}

#[test]
fn update_relationship_by_id_only_touches_the_target() {
    let (mut e, _c) = eng();
    // Two relationships; update exactly one by id.
    e.execute_cypher("CREATE (a:X)-[:R {w: 10}]->(b:X)")
        .expect("rel1");
    let rs = e
        .execute_cypher("CREATE (c:X)-[r:R {w: 20}]->(d:X) RETURN id(r) AS rid")
        .expect("rel2");
    let rid = rs.rows[0].values[0].as_i64().expect("rid");

    let mut params = HashMap::new();
    params.insert("rid".to_string(), Value::from(rid));
    e.execute_cypher_with_params(
        "MATCH ()-[r]->() WHERE id(r) = $rid SET r.w = 99 RETURN r",
        params,
    )
    .expect("update");

    let mut ws: Vec<i64> = e
        .execute_cypher("MATCH ()-[r:R]->() RETURN r.w AS w")
        .expect("read")
        .rows
        .iter()
        .filter_map(|row| row.values[0].as_i64())
        .collect();
    ws.sort_unstable();
    assert_eq!(ws, vec![10, 99], "only the targeted relationship changes");
}

#[test]
fn non_id_where_in_write_still_errors_clearly() {
    let (mut e, _c) = eng();
    e.execute_cypher("CREATE (a:X)-[:R {w: 1}]->(b:X)")
        .expect("seed");
    // A predicate the write path does not support must error (it always
    // did) — never silently mutate the wrong rows.
    let err = e
        .execute_cypher("MATCH ()-[r:R]->() WHERE r.w > 0 SET r.w = 5 RETURN r")
        .unwrap_err();
    assert!(
        format!("{err}").contains("WHERE in a write query"),
        "unsupported write WHERE must be a clear error, got: {err}"
    );
}
