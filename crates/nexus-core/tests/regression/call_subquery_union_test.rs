//! `UNION` / `UNION ALL` inside `CALL { }` and the duplicate-preservation
//! contract of a plain `RETURN`.
//!
//! `CALL { A UNION B }` is planned by `plan_ast` → `QueryPlanner::plan_query`,
//! which already splits on `Clause::Union` and emits `Operator::Union`, so the
//! union itself works. The subtle defect these tests lock down lived in the
//! *outer* projection: a plain `RETURN x` after the CALL used to deduplicate
//! legitimately-duplicate primitive rows (the projection's node-id dedup pass
//! collapsed every pure-primitive row to one because they share an empty key).
//! `UNION ALL` — and, more broadly, `UNWIND ['a','a']` — must preserve
//! duplicates; only `RETURN DISTINCT` removes them.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn sorted_i64(engine: &mut Engine, query: &str) -> Vec<i64> {
    let rs = engine.execute_cypher(query).expect("query must execute");
    let mut v: Vec<i64> = rs
        .rows
        .iter()
        .filter_map(|r| r.values.first().and_then(|x| x.as_i64()))
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn call_union_combines_branches() {
    let (mut engine, _ctx) = engine();
    assert_eq!(
        sorted_i64(
            &mut engine,
            "CALL { RETURN 1 AS x UNION RETURN 2 AS x } RETURN x"
        ),
        vec![1, 2]
    );
}

#[test]
fn call_union_deduplicates() {
    let (mut engine, _ctx) = engine();
    // Plain UNION removes duplicate rows across branches.
    assert_eq!(
        sorted_i64(
            &mut engine,
            "CALL { RETURN 1 AS x UNION RETURN 1 AS x } RETURN x"
        ),
        vec![1]
    );
}

#[test]
fn call_union_all_keeps_duplicates() {
    let (mut engine, _ctx) = engine();
    assert_eq!(
        sorted_i64(
            &mut engine,
            "CALL { RETURN 1 AS x UNION ALL RETURN 1 AS x } RETURN x"
        ),
        vec![1, 1]
    );
}

#[test]
fn call_union_over_matched_data() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {v: 10}), (:A {v: 11}), (:B {v: 20})")
        .expect("seed");
    assert_eq!(
        sorted_i64(
            &mut engine,
            "CALL { MATCH (a:A) RETURN a.v AS v UNION MATCH (b:B) RETURN b.v AS v } RETURN v",
        ),
        vec![10, 11, 20]
    );
}

#[test]
fn toplevel_union_all_keeps_duplicates_control() {
    let (mut engine, _ctx) = engine();
    // Control: top-level UNION ALL (no CALL) — proves the union operator keeps
    // duplicates on its own, so the CALL-path defect was in the outer RETURN.
    assert_eq!(
        sorted_i64(&mut engine, "RETURN 1 AS x UNION ALL RETURN 1 AS x"),
        vec![1, 1],
        "top-level UNION ALL must keep duplicates"
    );
}

#[test]
fn unwind_identical_primitives_keeps_duplicates() {
    let (mut engine, _ctx) = engine();
    // The same authoritative-rows contract: RETURN must not collapse
    // fully-identical primitive rows produced by UNWIND.
    assert_eq!(
        sorted_i64(&mut engine, "UNWIND [1, 1, 2] AS x RETURN x"),
        vec![1, 1, 2]
    );
    let rs = engine
        .execute_cypher("UNWIND ['a', 'a'] AS x RETURN x")
        .expect("query");
    assert_eq!(rs.rows.len(), 2, "UNWIND of identical strings keeps both");
}
