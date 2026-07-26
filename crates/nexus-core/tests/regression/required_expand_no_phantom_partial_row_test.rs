//! A REQUIRED (non-`OPTIONAL`) multi-hop `MATCH` whose LAST hop matches
//! nothing must yield ZERO result rows — not a partially-bound row with
//! `Null` standing in for the unmatched tail variables.
//!
//! ROOT CAUSE (confirmed): `execute_expand`
//! (`executor/operators/expand.rs`), the empty-expansion branch (entered
//! when a hop's traversal produces zero output rows) always cleared
//! `context.result_set.rows` in full, but only removed the CURRENT hop's
//! `source_var` / `target_var` / `rel_var` from `context.variables` —
//! leaving earlier-hop bindings (e.g. `a` from a prior successful hop)
//! alive. `execute_project`'s variable-materialization fallback
//! (`executor/operators/project.rs`) treats an empty `result_set.rows`
//! together with a non-empty `context.variables` as "no rows were
//! materialized yet, build one from the leftover variables" — resurrecting
//! the stale earlier-hop binding into a phantom row (`[a, Null]` / `[a,
//! Null, Null]`) instead of the correct zero rows.
//!
//! Fix: when a REQUIRED expand's output is empty, clear ALL of
//! `context.variables` (mirroring the unconditional `result_set.rows`
//! clear already in place), so the whole downstream row-space correctly
//! collapses to zero. `OPTIONAL MATCH` is unaffected — its LEFT-OUTER
//! semantics still null-pad the row and only ever clear this hop's own
//! three vars before re-establishing them as `Null`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use serde_json::Value;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

#[test]
fn required_two_hop_expand_with_failed_last_hop_returns_zero_rows() {
    // (a:A)-[:R1]->(b:B) exists; no :R2 edge and no :C node at all.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:R1]->(b:B {id: 2})")
        .expect("seed");

    let rs = engine
        .execute_cypher("MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN a, b, c")
        .expect("query");

    assert_eq!(
        rs.rows.len(),
        0,
        "a required MATCH whose last hop matches nothing must return zero rows, got {:?}",
        rs.rows
    );
}

#[test]
fn required_two_hop_expand_middle_var_unreturned_still_returns_zero_rows() {
    // (a:A)-[:R1]->(x:X) exists; no :R2 edge from x and no :B node at all.
    // `x` is not even in the RETURN list, isolating the leak to `a`.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:R1]->(x:X {id: 2})")
        .expect("seed");

    let rs = engine
        .execute_cypher("MATCH (a:A)-[:R1]->(x:X)-[:R2]->(b:B) RETURN a, b")
        .expect("query");

    assert_eq!(
        rs.rows.len(),
        0,
        "a required MATCH whose last hop matches nothing must return zero rows, got {:?}",
        rs.rows
    );
}

#[test]
fn required_two_hop_expand_binds_all_vars_when_fully_matched_control() {
    // Full chain exists: both hops must bind. Locks the working baseline
    // so the zero-row fix above cannot regress a legitimately matching
    // multi-hop pattern into an empty result.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:R1]->(b:B {id: 2})-[:R2]->(c:C {id: 3})")
        .expect("seed");

    let rs = engine
        .execute_cypher("MATCH (a:A)-[:R1]->(b:B)-[:R2]->(c:C) RETURN a.id, b.id, c.id")
        .expect("query");

    assert_eq!(rs.rows.len(), 1);
    assert_eq!(
        rs.rows[0].values,
        vec![Value::from(1), Value::from(2), Value::from(3)]
    );
}

#[test]
fn optional_match_hit_still_binds_both_vars_control() {
    // OPTIONAL MATCH with a real match: both vars must bind (unaffected by
    // the required-expand fix, which only touches the non-optional path).
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:R]->(b:B {id: 2})")
        .expect("seed");

    let rs = engine
        .execute_cypher("MATCH (a:A) OPTIONAL MATCH (a)-[:R]->(b:B) RETURN a.id, b.id")
        .expect("query");

    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::from(2)]);
}

#[test]
fn optional_match_miss_still_null_pads_instead_of_dropping_the_row() {
    // OPTIONAL MATCH that finds nothing must still null-pad and keep the
    // row (LEFT OUTER JOIN semantics) — the fix must not turn this into
    // zero rows.
    let (mut engine, _ctx) = engine();
    engine.execute_cypher("CREATE (a:A {id: 1})").expect("seed");

    let rs = engine
        .execute_cypher("MATCH (a:A) OPTIONAL MATCH (a)-[:R]->(b:B) RETURN a.id, b")
        .expect("query");

    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::Null]);
}
