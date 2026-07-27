//! `CREATE (a)-[:R]->(b)` combined in the SAME query with `SET` (or
//! `REMOVE`/`MERGE`/`FOREACH`) must create exactly ONE node per pattern
//! variable and must bind that variable to the CONNECTED node — not a
//! phantom, unconnected duplicate.
//!
//! ROOT CAUSE (confirmed): the linear CREATE arm in
//! `engine/write_exec.rs` (`execute_write_query`) iterates
//! `create_clause.pattern.elements` with a plain `.enumerate()` loop. The
//! `Relationship` arm peeks `elements.get(i + 1)` and creates the TARGET
//! node itself to wire the edge, binding the variable and updating
//! `last_node_id` — but the loop has no mechanism to skip that
//! already-consumed index. On the very next iteration the `Node` arm
//! fires again for the same element, creating a SECOND (orphan) node and
//! silently overwriting the variable binding / `last_node_id` with it.
//! Only `CREATE` with no trailing SET/REMOVE/MERGE/FOREACH is unaffected —
//! that combination routes through the executor's `create` operator
//! (`executor/operators/create.rs`), a separate code path.
//!
//! This suite pins the failing behaviour (before the fix) and the correct
//! behaviour (after): exactly one node per variable, and every later
//! clause referencing a pattern variable observes the connected node.

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
fn create_rel_with_set_does_not_duplicate_the_target_node() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:Alpha)-[:LINKS]->(b:Beta) SET a.k = 1")
        .expect("CREATE + SET");

    assert_eq!(
        count(&mut engine, "MATCH (n:Beta) RETURN count(n)"),
        1,
        "exactly one Beta node must exist after CREATE ... SET, not a phantom duplicate"
    );
    assert_eq!(
        count(
            &mut engine,
            "MATCH (:Alpha)-[:LINKS]->(b:Beta) RETURN count(b)"
        ),
        1,
        "the single Beta node must be reachable through the LINKS relationship, \
         not orphaned"
    );
}

#[test]
fn create_rel_with_set_on_target_lands_on_the_connected_node() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (a:Alpha)-[:R]->(b:Beta) SET b.marked = true")
        .expect("CREATE + SET on target var");

    let rs = engine
        .execute_cypher("MATCH (:Alpha)-[:R]->(b:Beta) RETURN b.marked")
        .expect("follow-up MATCH through the relationship");
    assert_eq!(
        rs.rows.len(),
        1,
        "expected exactly one node reachable through :R"
    );
    assert_eq!(
        rs.rows[0].values[0].as_bool(),
        Some(true),
        "the node reachable THROUGH the relationship must carry marked=true \
         (today SET lands on the orphan copy instead), got {:?}",
        rs.rows[0].values[0]
    );
}

#[test]
fn create_chained_rel_pattern_creates_one_node_per_variable_and_stays_connected() {
    let (mut engine, _ctx) = engine();
    // `b` is BOTH the target of `:R` and the source of `:S` — the hard
    // case that a naive "skip the very next element" fix would still get
    // wrong if it re-derives `last_node_id` from the wrong place.
    engine
        .execute_cypher("CREATE (a:A)-[:R]->(b:B)-[:S]->(c:C) SET a.k = 1")
        .expect("CREATE chain + SET");

    for label in ["A", "B", "C"] {
        assert_eq!(
            count(&mut engine, &format!("MATCH (n:{label}) RETURN count(n)")),
            1,
            "expected exactly one {label} node"
        );
    }

    assert_eq!(
        count(
            &mut engine,
            "MATCH (:A)-[:R]->(:B)-[:S]->(:C) RETURN count(*)"
        ),
        1,
        "the three nodes must form one fully connected A-R->B-S->C chain"
    );
}

#[test]
fn create_rel_without_set_still_produces_exactly_one_target_node_control() {
    let (mut engine, _ctx) = engine();
    // No SET/REMOVE/MERGE/FOREACH following the CREATE — this routes
    // through the executor's `create` operator, a separate path that must
    // stay correct (proves the fix targets only the linear write-query
    // CREATE arm).
    engine
        .execute_cypher("CREATE (a:Alpha)-[:LINKS]->(b:Beta)")
        .expect("plain CREATE");

    assert_eq!(
        count(&mut engine, "MATCH (n:Beta) RETURN count(n)"),
        1,
        "plain CREATE with no trailing write clause must produce exactly one Beta node"
    );
}

// ── UNWIND + CREATE: two DIFFERENT code paths, verified separately ─────
//
// `UNWIND ... CREATE <pattern with a relationship>` dispatches to one of
// two entirely different implementations depending on whether a
// SET/MERGE/REMOVE/FOREACH clause follows in the SAME statement
// (`query_pipeline.rs`'s `has_merge || has_set_clause || has_remove_clause
// || has_foreach` gate, ~line 706):
//
//   * WITHOUT one of those clauses, the query never reaches
//     `execute_write_query` at all — it is dispatched to the executor's
//     `create` operator (`executor/operators/create.rs`), which handles
//     UNWIND natively at the row level and is NOT the code this task's
//     defect lives in.
//   * WITH one of those clauses, the query IS routed through
//     `execute_write_query` -> `execute_unwind_write_query`
//     (`engine/write_exec.rs`), whose `Clause::Create` arm iterates
//     `create_clause.pattern.elements` and only implements the `Node`
//     case — any `Relationship` (or `QuantifiedGroup`) element falls into
//     the catch-all `_` arm, which calls `self.unwind_bindings.clear()`
//     and returns `Err(Error::CypherExecution("Relationship CREATE
//     inside UNWIND is not supported; ..."))` immediately, BEFORE any
//     "peek ahead and create the target node" logic (the mechanism that
//     causes the phantom duplicate in the LINEAR, non-UNWIND CREATE arm)
//     ever runs. Confirmed by direct probing (not assumed): this arm
//     structurally cannot produce a phantom-duplicate node, because it
//     never gets far enough to create a relationship's target — it always
//     bails out on the first relationship element, whether that element
//     is a lone `-[:R]->` or the middle hop of a longer chain.

#[test]
fn unwind_create_with_relationship_element_and_set_is_rejected_not_duplicated() {
    let (mut engine, _ctx) = engine();
    let res = engine.execute_cypher("UNWIND [1] AS x CREATE (a:A)-[:R]->(b:B) SET b.marked = true");

    let err = res.expect_err(
        "UNWIND + CREATE with a relationship element (combined with SET, which routes it \
         through `execute_unwind_write_query`) must be rejected, not silently accepted",
    );
    assert!(
        err.to_string()
            .contains("Relationship CREATE inside UNWIND is not supported"),
        "expected the documented UNWIND+CREATE relationship-rejection error, got: {err}"
    );

    // No phantom DUPLICATE is possible here (the assertion this task
    // cares about): at most the single `a:A` node created before the
    // relationship element was reached can exist, never two.
    //
    // NOTE (separate, pre-existing, out-of-scope finding — NOT the
    // phantom-duplicate-node defect this suite fixes): the rejection
    // happens AFTER `a:A` was already created, so it is left behind as a
    // partial write despite the statement returning an error. Pinned
    // here as documented current behaviour so a future change to this
    // arm's error-path cleanup is a deliberate, visible decision rather
    // than a silent regression.
    assert_eq!(
        count(&mut engine, "MATCH (n:A) RETURN count(n)"),
        1,
        "known pre-existing gap: the rejected UNWIND+CREATE leaves its already-created \
         first node behind; this must stay at exactly one (never two — that would be \
         the phantom-duplicate defect resurfacing), not become a NEW kind of failure"
    );
    assert_eq!(
        count(&mut engine, "MATCH (n:B) RETURN count(n)"),
        0,
        "the relationship's target node must never be created at all when the \
         pattern is rejected"
    );
}

#[test]
fn unwind_create_with_chained_relationship_pattern_and_set_is_rejected_not_duplicated() {
    // The "hard case" double-check requested for this arm: a chained
    // pattern `(a)-[:R]->(b)-[:S]->(c)` where `b` would otherwise be both
    // a relationship target AND a relationship source (the shape that
    // exercises the duplicate bug in the linear, non-UNWIND CREATE arm).
    let (mut engine, _ctx) = engine();
    let res =
        engine.execute_cypher("UNWIND [1] AS x CREATE (a:A)-[:R]->(b:B)-[:S]->(c:C) SET a.k = 1");

    let err = res
        .expect_err("a chained relationship pattern must be rejected the same way a single hop is");
    assert!(
        err.to_string()
            .contains("Relationship CREATE inside UNWIND is not supported"),
        "expected the documented UNWIND+CREATE relationship-rejection error, got: {err}"
    );

    // The rejection fires on the FIRST relationship element (`:R`),
    // before `b` or `c` are ever touched — so neither can be duplicated
    // (or created at all).
    assert_eq!(
        count(&mut engine, "MATCH (n:A) RETURN count(n)"),
        1,
        "same pre-existing partial-write gap as the single-hop case (see above)"
    );
    assert_eq!(
        count(&mut engine, "MATCH (n:B) RETURN count(n)"),
        0,
        "no B node — and critically, no DUPLICATE B node — must be created"
    );
    assert_eq!(
        count(&mut engine, "MATCH (n:C) RETURN count(n)"),
        0,
        "no C node must be created"
    );
}

#[test]
fn unwind_create_with_relationship_and_no_trailing_write_clause_stays_correct_control() {
    // WITHOUT a following SET/MERGE/REMOVE/FOREACH, this dispatches to
    // the executor's `create` operator instead of
    // `execute_unwind_write_query` — a different, already-correct code
    // path. Locks that it stays correct (one node per row per variable,
    // no duplication) across multiple UNWIND rows, so this suite does not
    // accidentally imply relationship patterns are broken under UNWIND in
    // general — only that one specific arm rejects them outright.
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("UNWIND [1, 2] AS x CREATE (a:A)-[:R]->(b:B {v: x})")
        .expect("UNWIND + CREATE with a relationship and no trailing write clause");

    assert_eq!(
        count(&mut engine, "MATCH (n:A) RETURN count(n)"),
        2,
        "exactly one A per UNWIND row, no duplication"
    );
    assert_eq!(
        count(&mut engine, "MATCH (n:B) RETURN count(n)"),
        2,
        "exactly one B per UNWIND row, no duplication"
    );
    assert_eq!(
        count(&mut engine, "MATCH (:A)-[:R]->(:B) RETURN count(*)"),
        2,
        "each row's A and B must be connected by :R, not orphaned"
    );
}
