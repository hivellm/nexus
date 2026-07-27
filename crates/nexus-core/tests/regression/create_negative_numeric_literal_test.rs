//! `CREATE (:T {v: -7})` — a negative numeric literal in a CREATE property
//! map — was rejected with "Complex expressions not supported in CREATE
//! properties" while `{v: 7}` succeeded (Neo4j accepts both).
//!
//! Root cause: a leading `-`/`+` parses as a `UnaryOp` wrapping the literal,
//! so it fell through the literal-only CREATE property-value evaluators to
//! their catch-all. The evaluators now constant-fold a unary sign over an
//! integer/float literal (`Expression::fold_signed_numeric_literal`) — both
//! the executor path (`operators/create.rs`) and the engine path
//! (`engine/match_exec.rs`). The `SET` path already evaluated the operand
//! and negated, so it is covered here only as a regression lock.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn one_i64(engine: &mut Engine, query: &str) -> i64 {
    let rs = engine.execute_cypher(query).expect("query");
    rs.rows[0].values[0].as_i64().expect("i64 value")
}

fn one_f64(engine: &mut Engine, query: &str) -> f64 {
    let rs = engine.execute_cypher(query).expect("query");
    rs.rows[0].values[0].as_f64().expect("f64 value")
}

/// Standalone CREATE (executor `operators/create.rs` path): a negative
/// integer/float and an explicit `+` all round-trip.
#[test]
fn create_node_accepts_signed_numeric_literals() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:NegT {i: -7, f: -1.5, plus: +9, pos: 3})")
        .expect("CREATE with signed literals must not be rejected");

    assert_eq!(one_i64(&mut engine, "MATCH (n:NegT) RETURN n.i"), -7);
    assert_eq!(one_f64(&mut engine, "MATCH (n:NegT) RETURN n.f"), -1.5);
    assert_eq!(one_i64(&mut engine, "MATCH (n:NegT) RETURN n.plus"), 9);
    assert_eq!(one_i64(&mut engine, "MATCH (n:NegT) RETURN n.pos"), 3);
}

/// Relationship inline property map takes the same evaluator path.
#[test]
fn create_relationship_accepts_negative_literal() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A {id: 1})-[:R {w: -2}]->(:B {id: 2})")
        .expect("CREATE rel with a negative property must not be rejected");

    assert_eq!(
        one_i64(&mut engine, "MATCH ()-[r:R]->() RETURN r.w"),
        -2,
        "relationship property must round-trip the negative value"
    );
}

/// MATCH ... CREATE routes property evaluation through the engine
/// (`match_exec.rs`) path — the same fold must apply there.
#[test]
fn match_create_accepts_negative_literal() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:Seed {id: 1})")
        .expect("seed");
    engine
        .execute_cypher("MATCH (s:Seed) CREATE (:Derived {v: -42})")
        .expect("MATCH...CREATE with a negative literal must not be rejected");

    assert_eq!(one_i64(&mut engine, "MATCH (n:Derived) RETURN n.v"), -42);
}

/// SET with a negative literal already worked (the SET evaluator negates the
/// operand) — locked here so the §4.7 change doesn't regress it.
#[test]
fn set_negative_literal_still_works() {
    let (mut engine, _ctx) = engine();
    engine.execute_cypher("CREATE (:S {id: 1})").expect("seed");
    let rs = engine
        .execute_cypher("MATCH (n:S) SET n.g = -4 RETURN n.g")
        .expect("SET with a negative literal");
    assert_eq!(rs.rows[0].values[0].as_i64(), Some(-4));
}
