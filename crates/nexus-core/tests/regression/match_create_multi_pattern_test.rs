//! A comma-joined `MATCH (a:A), (b:B) CREATE (a)-[:R]->(b)` must create exactly
//! ONE relationship per driving row, not the square of the row count.
//!
//! By the time a CREATE runs, the read pipeline has already materialised the
//! driving rows as ALIGNED columns — a comma-joined MATCH puts its cartesian
//! product into `a = [a1, a2]`, `b = [b1, b2]` (index i = one row). The CREATE
//! operator's slow path used `materialize_rows_from_variables`, which
//! re-crosses equal-length multi-element columns, so N driving rows produced
//! N² edges (3 A's × 1 B wrote 9 edges, not 3). Fixed by zipping the aligned
//! columns instead, mirroring the read path's `seed_scan_main_loop`.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn count(engine: &mut Engine, cypher: &str) -> i64 {
    engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("`{cypher}` failed: {e}"))
        .rows[0]
        .values[0]
        .as_i64()
        .expect("count is an integer")
}

#[test]
fn comma_join_create_is_one_edge_per_driving_row() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {id: 1}), (:A {id: 2}), (:A {id: 3}), (:B {id: 9})")
        .expect("seed");

    // 3 A's × 1 B = 3 driving rows.
    assert_eq!(
        count(&mut engine, "MATCH (a:A), (b:B) RETURN count(*)"),
        3,
        "the driving set is 3 rows"
    );

    engine
        .execute_cypher("MATCH (a:A), (b:B) CREATE (a)-[:R]->(b)")
        .expect("create");

    assert_eq!(
        count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"),
        3,
        "one edge per driving row, not 3² = 9"
    );
    // Each A points at the shared B exactly once — no duplicate edges.
    assert_eq!(
        count(
            &mut engine,
            "MATCH (a:A)-[:R]->(b:B) RETURN count(DISTINCT a)"
        ),
        3,
        "every A must be a distinct edge source"
    );
}

#[test]
fn comma_join_create_scales_linearly_not_quadratically() {
    // A wider driving set makes the N² blow-up unmistakable: 5 × 4 = 20 rows,
    // which the bug turned into 400 edges.
    let (mut engine, _ctx) = engine();
    for i in 0..5 {
        engine
            .execute_cypher(&format!("CREATE (:P {{id: {i}}})"))
            .expect("seed p");
    }
    for j in 0..4 {
        engine
            .execute_cypher(&format!("CREATE (:Q {{id: {j}}})"))
            .expect("seed q");
    }

    engine
        .execute_cypher("MATCH (p:P), (q:Q) CREATE (p)-[:R]->(q)")
        .expect("create");

    assert_eq!(
        count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"),
        20,
        "5 P × 4 Q is 20 driving rows and 20 edges, not 20² = 400"
    );
}

#[test]
fn single_row_and_unwind_create_paths_stay_correct() {
    // Guards that the aligned-zip fix did not regress the paths that were
    // already correct.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {id: 1}), (:A {id: 2}), (:B {id: 9})")
        .expect("seed");

    // Both endpoints pinned to one node each → exactly one edge.
    engine
        .execute_cypher("MATCH (a:A {id: 1}), (b:B {id: 9}) CREATE (a)-[:ONE]->(b)")
        .expect("single");
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:ONE]->() RETURN count(r)"),
        1
    );

    // UNWIND creating fresh nodes → one per list element.
    engine
        .execute_cypher("UNWIND [10, 11, 12, 13] AS x CREATE (:U {v: x})")
        .expect("unwind create");
    assert_eq!(count(&mut engine, "MATCH (u:U) RETURN count(u)"), 4);

    // UNWIND resolving both endpoints by id → one edge per element.
    engine
        .execute_cypher(
            "UNWIND [1, 2] AS i MATCH (a:A {id: i}), (b:B {id: 9}) CREATE (a)-[:BY_ID]->(b)",
        )
        .expect("unwind edges");
    assert_eq!(
        count(&mut engine, "MATCH ()-[r:BY_ID]->() RETURN count(r)"),
        2
    );
}
