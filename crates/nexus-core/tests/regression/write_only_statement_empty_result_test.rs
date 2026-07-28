//! Write-only statements (no `RETURN`/`WITH` downstream) must yield an
//! EMPTY result set per openCypher/TCK semantics ("the result should be
//! empty"), independent of whatever side effects the write produced.
//!
//! `CREATE` without `RETURN` used to leak a phantom row synthesized from
//! its named variables. Two independent spots did this and both are fixed
//! here:
//!
//! - The standalone-CREATE fast path in
//!   `executor/dispatch/operator_loop.rs` (no preceding `MATCH`) — gated
//!   its result-set synthesis on a downstream `Project`/`With` lookahead
//!   over the operator list.
//! - `execute_create_with_context` (`executor/operators/create.rs`), used
//!   by `MATCH ... CREATE` via the main operator loop's `Operator::Create`
//!   arm — took a new `has_downstream_projection: bool` parameter computed
//!   the same way by that arm. Its OTHER caller
//!   (`executor/operators/dispatch.rs`'s `execute_operator`, used by
//!   UNION/JOIN branches and `CALL` subquery bodies) has no operator-list
//!   lookahead available and keeps the pre-existing always-synthesize
//!   behaviour (passes `true`) — out of scope here, those callers own
//!   their own row consumption.
//!
//! This file also pins the sibling write-only families that share the
//! "no RETURN clause -> empty ResultSet" contract but route through a
//! different code path (`Engine::execute_write_query` in
//! `engine/write_exec/dispatch.rs`, whose `result` stays `None` — and
//! therefore empty — unless a `Clause::Return` is present) or the
//! dedicated DELETE dispatch branch in `engine/query_pipeline.rs`. None of
//! MERGE/SET/REMOVE/DELETE needed a code change: only the two CREATE
//! synthesis sites above were populating a row unconditionally.

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

/// (a) The anchor bug: a multi-clause standalone `CREATE` with bound
/// variables and no `RETURN` must return zero rows, not a phantom row
/// synthesized from the created variables.
#[test]
fn multi_clause_create_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher("CREATE (a:Wr1) CREATE (b:Wr1) CREATE (a)-[:R]->(b)")
        .expect("multi-clause create");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only CREATE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effects still happened.
    assert_eq!(count(&mut engine, "MATCH (n:Wr1) RETURN count(n)"), 2);
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
}

/// (a) The sibling mechanism: `MATCH ... CREATE` with bound variables and
/// no `RETURN` (TCK Create2[5]/[10]/[11]) routes through
/// `execute_create_with_context` via the main operator loop, a different
/// function from the standalone fast path above but with the identical
/// unconditional-synthesis bug.
#[test]
fn match_create_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:Wr1b) CREATE (:Wr1c)")
        .expect("seed");

    let result = engine
        .execute_cypher("MATCH (x:Wr1b), (y:Wr1c) CREATE (x)-[:R]->(y)")
        .expect("match create");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only MATCH...CREATE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effect still happened.
    assert_eq!(count(&mut engine, "MATCH ()-[r:R]->() RETURN count(r)"), 1);
}

/// (b) Control: an aggregating `RETURN` (`count(*)`) after a standalone
/// `CREATE` must still produce exactly one row with the count — the
/// planner does not always emit a `Project` for an aggregate-only RETURN
/// (`projection_items` stays empty when there is no plain projection
/// item), so the downstream-consumer lookahead must also recognise
/// `Operator::Aggregate`, not just `Project`/`With`. Recognising it in the
/// lookahead alone is not sufficient: the standalone-CREATE fast path's
/// own operator-consuming loop (`operator_loop.rs`, right after the
/// lookahead) previously had no `Operator::Aggregate` arm either — it
/// fell into the silent `_ => {}` catch-all — so a bare `RETURN count(*)`
/// still produced the raw CREATE phantom row (empirically confirmed
/// before adding the arm: 1 row, column `n`, a node object, not
/// `count(*)`). Both the lookahead AND that execution arm needed the fix
/// for this scenario to return the correct `count(*) = 1`.
#[test]
fn standalone_create_with_return_count_star_preserves_row() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher("CREATE (n:Wr8) RETURN count(*)")
        .expect("create with aggregating return");

    assert_eq!(result.rows.len(), 1, "RETURN count(*) must produce a row");
    assert_eq!(result.rows[0].values[0].as_i64(), Some(1));
}

/// (b) Control: the same shape WITH a trailing `RETURN` must keep
/// returning its rows exactly as before the fix.
#[test]
fn multi_clause_create_with_return_preserves_rows() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher(
            "CREATE (a:Wr2 {name: 'x'}) CREATE (b:Wr2) CREATE (a)-[:R]->(b) RETURN a.name",
        )
        .expect("multi-clause create with return");

    assert_eq!(result.rows.len(), 1, "RETURN must still produce a row");
    assert_eq!(result.rows[0].values[0].as_str(), Some("x"));
}

/// (b) Control for the `MATCH ... CREATE` mechanism: a trailing `RETURN`
/// must keep returning the created relationship's row exactly as before.
#[test]
fn match_create_with_return_preserves_rows() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:Wr2b) CREATE (:Wr2c)")
        .expect("seed");

    let result = engine
        .execute_cypher("MATCH (x:Wr2b), (y:Wr2c) CREATE (x)-[r:R]->(y) RETURN type(r)")
        .expect("match create with return");

    assert_eq!(result.rows.len(), 1, "RETURN must still produce a row");
    assert_eq!(result.rows[0].values[0].as_str(), Some("R"));
}

/// (c) `SET` without `RETURN` (routes through `Engine::execute_write_query`,
/// a different code path from the CREATE fast path) must also return an
/// empty result set.
#[test]
fn set_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (n:Wr3 {p: 1})")
        .expect("seed");

    let result = engine
        .execute_cypher("MATCH (n:Wr3) SET n.p = 2")
        .expect("set without return");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only SET must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effect: the property was actually updated.
    assert_eq!(
        count(&mut engine, "MATCH (n:Wr3 {p: 2}) RETURN count(n)"),
        1
    );
}

/// (c) `REMOVE` without `RETURN` must also return an empty result set.
#[test]
fn remove_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (n:Wr4 {p: 1})")
        .expect("seed");

    let result = engine
        .execute_cypher("MATCH (n:Wr4) REMOVE n.p")
        .expect("remove without return");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only REMOVE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effect: the property was actually removed.
    assert_eq!(
        count(
            &mut engine,
            "MATCH (n:Wr4) WHERE n.p IS NULL RETURN count(n)"
        ),
        1
    );
}

/// (c) Standalone `MERGE` without `RETURN` must also return an empty
/// result set.
#[test]
fn merge_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    let result = engine
        .execute_cypher("MERGE (n:Wr5 {id: 1})")
        .expect("merge without return");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only MERGE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effect: the node was actually created.
    assert_eq!(
        count(&mut engine, "MATCH (n:Wr5 {id: 1}) RETURN count(n)"),
        1
    );

    // Re-running the same MERGE must stay idempotent AND still empty.
    let result2 = engine
        .execute_cypher("MERGE (n:Wr5 {id: 1})")
        .expect("second merge without return");
    assert!(result2.rows.is_empty());
    assert_eq!(
        count(&mut engine, "MATCH (n:Wr5 {id: 1}) RETURN count(n)"),
        1
    );
}

/// (c) `DELETE` without `RETURN` — already fixed ahead of this task
/// (`0b3ae5d8 feat(cypher): RETURN-less DELETE yields empty result set`);
/// pinned here alongside its write-only siblings.
#[test]
fn delete_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    engine.execute_cypher("CREATE (n:Wr6)").expect("seed");

    let result = engine
        .execute_cypher("MATCH (n:Wr6) DELETE n")
        .expect("delete without return");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only DELETE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effect: the node was actually deleted.
    assert_eq!(count(&mut engine, "MATCH (n:Wr6) RETURN count(n)"), 0);
}

/// (c) `DETACH DELETE` without `RETURN` shares the same dispatch branch as
/// plain `DELETE`; pinned separately since it also removes relationships.
#[test]
fn detach_delete_without_return_yields_empty_result() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:Wr7)-[:R]->(b:Wr7)")
        .expect("seed");

    let result = engine
        .execute_cypher("MATCH (n:Wr7) DETACH DELETE n")
        .expect("detach delete without return");

    assert!(result.columns.is_empty());
    assert!(
        result.rows.is_empty(),
        "write-only DETACH DELETE must return an empty result set, got {} rows",
        result.rows.len()
    );

    // (d) side effects: both nodes and the relationship are gone.
    assert_eq!(count(&mut engine, "MATCH (n:Wr7) RETURN count(n)"), 0);
    assert_eq!(count(&mut engine, "MATCH ()-[r]->() RETURN count(r)"), 0);
}
