//! Consecutive `CREATE` clauses in one statement must share variable
//! bindings: `CREATE (a:A), (b:B) CREATE (a)-[:T]->(b)` has to wire the
//! relationship onto the ORIGINAL `a`/`b`, not mint anonymous duplicates.
//!
//! Two independent code paths create nodes/relationships for a Cypher
//! statement, and both needed the same fix:
//!
//! - A plain multi-clause `CREATE` statement with no `MATCH` (the TCK
//!   Create2 shapes, e.g. `CREATE (a) CREATE (b) CREATE (a)-[:R]->(b)`)
//!   never reaches the engine's write-clause loop at all — it is dispatched
//!   straight to the executor's own operator pipeline
//!   (`Executor::execute_inner`'s standalone-CREATE fast path in
//!   `executor/dispatch/operator_loop.rs`). That fast path executed each
//!   `Operator::Create` (one per `CREATE` clause) through
//!   `execute_create_pattern_with_variables`, which starts a brand-new,
//!   clause-local `created_nodes` map on every call — so a bare `(a)`
//!   reference in a later clause could never see the node an earlier clause
//!   had already bound, and minted an unbound duplicate instead. Fixed by
//!   threading one `created_nodes` / `created_relationships` accumulator
//!   across every clause via `execute_create_pattern_internal` (the same
//!   function `execute_create_pattern_with_variables` wraps), whose
//!   node/relationship-target arms already prefer an existing entry in that
//!   map over creating a fresh node — they just never had one that outlived
//!   a single clause before.
//! - When a `CREATE` is combined with `MERGE`/`SET`/`REMOVE`/`FOREACH` in the
//!   same statement, routing instead goes through
//!   `Engine::execute_write_query`'s clause loop
//!   (`engine/write_exec/dispatch.rs`), whose `Create` arm had the same
//!   blind spot: it always called `create_node_with_external_id` for a node
//!   element and unconditionally overwrote `context[var]`, never consulting
//!   `context` for a variable already bound by an earlier clause in the
//!   loop. Fixed by reusing a pre-bound single-id variable (both the plain
//!   `Node` arm and the relationship's target-node resolution) instead of
//!   creating a new entity for it.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn count(engine: &mut Engine, cypher: &str) -> i64 {
    let result = engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("`{cypher}` failed: {e}"));
    assert!(!result.rows.is_empty(), "`{cypher}` returned no rows");
    result.rows[0].values[0]
        .as_i64()
        .expect("count is an integer")
}

/// Standalone (no-`MATCH`) multi-clause `CREATE` — the shape that routes
/// through `executor/dispatch/operator_loop.rs`'s fast path.
#[test]
fn two_clause_create_wires_relationships_onto_original_nodes() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher(
            "CREATE (a:A), (b:B), (c:C)
             CREATE (a)-[:T1]->(b), (b)-[:T2]->(c)",
        )
        .expect("two-clause create");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 3);
    assert_eq!(count(&mut engine, "MATCH ()-[r]->() RETURN count(r)"), 2);
    assert_eq!(
        count(&mut engine, "MATCH (:A)-[:T1]->(:B) RETURN count(*)"),
        1,
        "the second clause's `(a)-[:T1]->(b)` must reuse the first clause's a/b, \
         not wire onto anonymous duplicates"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:B)-[:T2]->(:C) RETURN count(*)"),
        1
    );
}

/// TCK `Create2[3]`: "Create two nodes and a single relationship in
/// separate clauses".
///
/// Deliberately does NOT assert an empty result set: the standalone-CREATE
/// fast path still synthesizes a result row from named CREATE variables even
/// without a `RETURN` (see `operator_loop.rs`, the `!columns.is_empty()`
/// block) — a separate pre-existing bug. Do not add that assertion here
/// until that bug is fixed.
#[test]
fn three_clause_chain_create_reuses_across_clauses() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a) CREATE (b) CREATE (a)-[:R]->(b)")
        .expect("three-clause chain");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 2);
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
}

/// TCK `Create2[2]` control: a single-clause comma pattern must keep
/// working exactly as before (its reuse is driven by intra-pattern
/// mechanics that this fix does not touch).
#[test]
fn single_clause_create_control_is_unchanged() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a), (b), (a)-[:R]->(b)")
        .expect("single clause");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 2);
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
}

/// TCK `Create2[5]`/`[6]` shape: `MATCH ... CREATE` reuse (a different,
/// already-correct code path — `executor::execute_create_with_context`) must
/// keep working unchanged.
#[test]
fn match_then_create_reuse_is_unaffected() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:X) CREATE (:Y)")
        .expect("seed");
    engine
        .execute_cypher("MATCH (x:X), (y:Y) CREATE (x)-[:R]->(y)")
        .expect("match then create");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 2);
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
}

/// A variable name that never appeared in an earlier clause must still
/// create a fresh entity — reuse only fires for an already-bound name.
#[test]
fn unbound_name_in_later_clause_still_creates_fresh() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:A) CREATE (a)-[:R]->(z:Z)")
        .expect("second clause introduces a fresh variable");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 2);
    assert_eq!(
        count(&mut engine, "MATCH (:A)-[:R]->(:Z) RETURN count(*)"),
        1
    );
}

/// `RETURN` of a variable bound by an earlier `CREATE` clause must reflect
/// the reused entity's own data, not a null/duplicate.
#[test]
fn return_after_later_clause_reuse_reads_the_original_node() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher(
            "CREATE (a:A {name: 'first'})
             CREATE (b:B)
             CREATE (a)-[:R]->(b)
             RETURN a.name",
        )
        .expect("create then return reused variable");

    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("first"),
        "RETURN must read the ORIGINAL node's property, not a fresh/null duplicate"
    );
}

/// The `engine/write_exec` clause loop's own blind spot: `CREATE` combined
/// with `SET` in the same statement forces routing through
/// `Engine::execute_write_query` rather than the executor's standalone-CREATE
/// fast path, exercising the reuse fix added to that loop's `Create` arm.
#[test]
fn create_then_create_then_set_reuses_across_clauses() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher(
            "CREATE (a:A {name: 'orig'})
             CREATE (b:B)
             CREATE (a)-[:R]->(b)
             SET b.tag = 'x'
             RETURN a.name",
        )
        .expect("create/create/create/set");

    assert_eq!(count(&mut engine, "MATCH (n) RETURN count(n)"), 2);
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
    assert_eq!(
        count(&mut engine, "MATCH (b:B {tag: 'x'}) RETURN count(b)"),
        1,
        "SET must still apply to the SAME b the CREATE clauses bound"
    );
    assert_eq!(
        result.rows[0].values[0].as_str(),
        Some("orig"),
        "RETURN must read the original a, not a fresh duplicate the SET-forced \
         write_exec path might have minted"
    );
}

/// Guard (requirement 4): `MERGE` between two pre-bound nodes must keep
/// behaving idempotently — the `write_exec` dispatch loop's `Merge` arm was
/// not touched by this fix, but shares the file with the `Create` arm that
/// was.
#[test]
fn merge_relationship_between_prebound_nodes_still_idempotent() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:P {id: 1}) CREATE (:P {id: 2})")
        .expect("seed");

    let merge = "MATCH (a:P {id: 1}), (b:P {id: 2}) MERGE (a)-[:KNOWS]->(b)";
    engine.execute_cypher(merge).expect("first merge");
    engine.execute_cypher(merge).expect("second merge");

    assert_eq!(
        count(&mut engine, "MATCH ()-[r:KNOWS]->() RETURN count(r)"),
        1,
        "re-running the same MERGE must not create a duplicate edge"
    );
}

/// Guard (requirement 4): `UNWIND` + `CREATE` accumulation across rows must
/// keep working — the `execute_unwind_write_query` path was not touched by
/// this fix. The created node is deliberately NAMED: if a shared binding
/// map ever leaked across UNWIND rows, rows 2 and 3 would reuse `n` and
/// the count would collapse to 1.
#[test]
fn unwind_create_still_accumulates_across_rows() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("UNWIND [1, 2, 3] AS x CREATE (n:N {v: x})")
        .expect("unwind create");

    assert_eq!(count(&mut engine, "MATCH (n:N) RETURN count(n)"), 3);
}
