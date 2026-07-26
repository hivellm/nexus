//! `WITH` → `MATCH` clause-boundary binding (phase7 §4.11).
//!
//! ROOT CAUSE (confirmed): `QueryPlanner::plan_query`
//! (`executor/planner/queries/planner_core.rs`) is a "bucket" planner — it
//! collects the pattern of EVERY `MATCH` clause into a single `patterns` Vec
//! (`:373`) and hands them all to one `plan_execution_strategy` call
//! (`:655`), while `WITH` clauses are collected separately into
//! `with_operators` (`:466`). The interleaving order between `MATCH` and
//! `WITH` is therefore lost: a `MATCH` that textually FOLLOWS a `WITH` is
//! matched in the same up-front pattern phase, and the `WITH` projection —
//! which only references its own items — then drops every variable that the
//! post-`WITH` `MATCH` introduced. The carried variable survives; the newly
//! introduced ones come back `Null`.
//!
//! This is broader than the original filing (which believed it was specific
//! to variable-length expands carried across `WITH … LIMIT`): ANY `MATCH`
//! after a `WITH` that binds new variables loses them, including a plain
//! fresh scan (`WITH 1 AS x MATCH (p:Post) RETURN x, p`).
//!
//! The correct fix is segment-based planning: split the clause list at each
//! `WITH` boundary and plan each segment as a pipeline stage feeding the
//! next. That is a core planner change, tracked as §4.11; the failing case
//! below is `#[ignore]`d as its executable spec, and the two controls lock
//! the shapes that already work so a future fix cannot silently regress them.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use serde_json::Value;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn seed(engine: &mut Engine) {
    engine
        .execute_cypher(
            "CREATE (p:Person {id: 933}),
                    (m:Message {id: 1}),
                    (post:Post {id: 100}),
                    (m)-[:HAS_CREATOR]->(p),
                    (m)-[:REPLY_OF]->(post)",
        )
        .expect("seed");
}

#[test]
fn fresh_match_expand_binds_target_control() {
    // No WITH: the expand binds its target. Locks the working baseline.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let rs = engine
        .execute_cypher("MATCH (m:Message)-[:REPLY_OF]->(post:Post) RETURN m.id, post.id")
        .expect("query");
    assert_eq!(
        rs.rows[0].values,
        vec![Value::from(1), Value::from(100)],
        "expand without an intervening WITH binds post"
    );
}

#[test]
fn multi_match_without_with_binds_both_control() {
    // Two MATCH clauses with NO WITH between them: both bind (cartesian).
    // Proves multi-MATCH itself is fine — the defect is the WITH boundary.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let rs = engine
        .execute_cypher("MATCH (m:Message) MATCH (post:Post) RETURN m.id, post.id")
        .expect("query");
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::from(100)]);
}

#[test]
fn with_then_match_expand_binds_target() {
    // Expand from the carried variable after WITH (the LDBC IS2 shape).
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let carried = engine
        .execute_cypher(
            "MATCH (m:Message) WITH m \
             MATCH (m)-[:REPLY_OF]->(post:Post) RETURN m.id, post.id",
        )
        .expect("query");
    assert_eq!(
        carried.rows[0].values,
        vec![Value::from(1), Value::from(100)],
        "post-WITH expand must bind post"
    );
}

#[test]
fn with_literal_then_match_binds_fresh_scan() {
    // Fresh scan after a query-initial WITH: even an unrelated pattern must
    // bind (Cartesian against the carried scope). The minimal trigger.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let fresh = engine
        .execute_cypher("WITH 1 AS x MATCH (post:Post) RETURN x, post.id")
        .expect("query");
    assert_eq!(
        fresh.rows[0].values,
        vec![Value::from(1), Value::from(100)],
        "post-WITH fresh scan must bind post"
    );
}

#[test]
fn with_carried_then_match_fresh_pattern_cartesians() {
    // Fresh, unrelated pattern after WITH cartesians against the carried m.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let rs = engine
        .execute_cypher("MATCH (m:Message) WITH m MATCH (post:Post) RETURN m.id, post.id")
        .expect("query");
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::from(100)]);
}

#[test]
fn with_then_variable_length_expand_binds_target() {
    // The original §4.11 filing: a *0.. variable-length expand from a
    // WITH-carried source. `*0..` also yields the zero-hop row, so assert
    // the one-hop Post is present rather than pinning the exact row set.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);
    let rs = engine
        .execute_cypher(
            "MATCH (m:Message) WITH m \
             MATCH (m)-[:REPLY_OF*1..]->(post:Post) RETURN post.id",
        )
        .expect("query");
    assert!(
        rs.rows
            .iter()
            .any(|r| r.values.first() == Some(&Value::from(100))),
        "variable-length expand after WITH must reach the Post, got {:?}",
        rs.rows
    );
}
