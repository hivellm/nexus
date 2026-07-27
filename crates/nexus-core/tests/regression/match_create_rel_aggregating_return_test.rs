//! `MATCH (a:L{k:1}),(b:L{k:2}) CREATE (a)-[:R]->(b) RETURN count(*)` must
//! actually create the relationship — silently failing to do so is a
//! data-loss bug.
//!
//! The planner inserted `CREATE` operators before the first `Project` sink
//! only, so when the projection aggregates (`RETURN count(*)`, or a `WITH
//! count(*) AS c`), the sink is `Operator::Aggregate` instead and `CREATE`
//! ended up placed AFTER it. The `Aggregate` operator runs first, collapses
//! all driving rows and overwrites `context.variables`, destroying the
//! matched node objects (their `_nexus_id`) before `CREATE` ever executes,
//! so the write silently no-ops against zero usable rows. Fixed by matching
//! `Operator::Project { .. } | Operator::Aggregate { .. }` when locating the
//! CREATE insertion point, mirroring the WITH-insertion logic already used
//! a few lines above for the same reason (phase6 §5.3).

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

fn seed(engine: &mut Engine) {
    engine
        .execute_cypher("CREATE (:L {k: 1}), (:L {k: 2})")
        .expect("seed");
}

#[test]
fn create_before_bare_count_aggregate_still_writes_relationship() {
    let (mut engine, _ctx) = engine();
    seed(&mut engine);

    let result = engine
        .execute_cypher("MATCH (a:L {k: 1}), (b:L {k: 2}) CREATE (a)-[:R]->(b) RETURN count(*)")
        .expect("create with aggregating return");

    assert_eq!(
        result.side_effects.relationships_created, 1,
        "the aggregating RETURN must not suppress the CREATE side effect"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:L)-[r:R]->(:L) RETURN count(r)"),
        1,
        "the relationship must actually be visible in storage"
    );
}

#[test]
fn create_before_with_count_aggregate_still_writes_relationship() {
    let (mut engine, _ctx) = engine();
    seed(&mut engine);

    let result = engine
        .execute_cypher(
            "MATCH (a:L {k: 1}), (b:L {k: 2}) CREATE (a)-[:R]->(b) WITH count(*) AS c RETURN c",
        )
        .expect("create with aggregating WITH");

    assert_eq!(
        result.side_effects.relationships_created, 1,
        "an aggregating WITH between CREATE and RETURN must not suppress the CREATE side effect"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:L)-[r:R]->(:L) RETURN count(r)"),
        1,
        "the relationship must actually be visible in storage"
    );
}

#[test]
fn create_before_non_aggregating_return_still_works() {
    // Guard for the already-working case — must stay green after the fix.
    let (mut engine, _ctx) = engine();
    seed(&mut engine);

    let result = engine
        .execute_cypher("MATCH (a:L {k: 1}), (b:L {k: 2}) CREATE (a)-[r:R]->(b) RETURN a.k")
        .expect("create with non-aggregating return");

    assert_eq!(
        result.side_effects.relationships_created, 1,
        "the non-aggregating RETURN path must keep creating the relationship"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:L)-[r:R]->(:L) RETURN count(r)"),
        1,
        "the relationship must actually be visible in storage"
    );
}
