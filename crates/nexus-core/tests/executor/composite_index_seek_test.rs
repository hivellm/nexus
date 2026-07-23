//! End-to-end coverage for composite-index detection on inline
//! multi-property selectors.
//!
//! Before this fix, `node_index_seek_for`
//! (`crates/nexus-core/src/executor/planner/queries/strategy.rs`) only ever
//! probed SINGLE-property indexes (`prop_idx.has_index(label_id, key_id)`
//! per property) when deciding how to plan a node selector. A query like
//! `MATCH (n:Person {tenantId: 't1', id: 1})` full-scanned even when a
//! COMPOSITE index (or NODE KEY constraint, which registers a UNIQUE
//! composite index under the hood) covered exactly `(tenantId, id)` —
//! the planner never had a handle to the composite-index registry at
//! plan time, so `Operator::CompositeBtreeSeek` was defined and executed
//! but never emitted.
//!
//! The fix threads a `CompositeBtreeRegistry` handle through the planner
//! (`QueryPlanner::with_composite_index`, wired at `Executor::plan_ast`)
//! and adds `composite_index_seek_for`, tried BEFORE the single-property
//! seek at both node-processing loops in `plan_execution_strategy`.

use nexus_core::Engine;
use nexus_core::executor::types::Operator;
use nexus_core::testing::TestContext;

/// PLAN-SHAPE (fails pre-fix): an inline selector binding every column of
/// a registered composite index (here, via a `NODE KEY` constraint) must
/// plan a `CompositeBtreeSeek`, not a `NodeByLabel` scan.
#[test]
fn full_inline_selector_on_composite_index_produces_composite_btree_seek() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {tenantId: 't1', id: 1}) RETURN n")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "a full inline selector over a registered composite index must \
         plan a CompositeBtreeSeek; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "a composite-index-covered selector must not fall back to a \
         NodeByLabel scan; plan = {plan:?}"
    );
}

/// PLAN-SHAPE + BEHAVIORAL: the composite seek returns exactly the node
/// whose tuple matches, on a dataset with several distinct `(tenantId,
/// id)` combinations.
#[test]
fn composite_index_seek_returns_correct_row() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    // Populate the composite B-tree via MERGE — the Cypher CREATE
    // clause's executor operator does not maintain the composite index
    // (see `node_key_delete_reuse_test.rs`), so seeding must go through
    // MERGE for the seek to have anything to find.
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 1, name: 'Alice'})")
        .expect("seed t1/1");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 2, name: 'Bob'})")
        .expect("seed t1/2");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't2', id: 1, name: 'Carol'})")
        .expect("seed t2/1");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {tenantId: 't1', id: 1}) RETURN n.name")
        .expect("plan must succeed");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "plan = {plan:?}"
    );

    let result = engine
        .execute_cypher("MATCH (n:Person {tenantId: 't1', id: 1}) RETURN n.name")
        .expect("query must succeed");

    let names: Vec<String> = result
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name must be a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        vec!["Alice".to_string()],
        "the composite seek must return exactly the (t1, 1) tuple's node, \
         not any other (tenantId, id) combination; got {names:?}"
    );
}

/// BEHAVIORAL parity: the composite-seeked inline-form query and an
/// equivalent WHERE-form query (which the planner does NOT lift into a
/// composite seek — `where_equality_index_seek_for` only ever consults
/// single-property indexes, so this stays a full `NodeByLabel` + `Filter`
/// scan) must return identical rows. The fix is a plan-selection
/// optimisation, not a semantics change.
#[test]
fn composite_seek_and_full_scan_equivalent_return_identical_rows() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 1, name: 'Alice'})")
        .expect("seed t1/1");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 2, name: 'Bob'})")
        .expect("seed t1/2");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't2', id: 1, name: 'Carol'})")
        .expect("seed t2/1");

    // Inline form: seeked via CompositeBtreeSeek post-fix.
    let inline_plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {tenantId: 't1', id: 1}) RETURN n.name")
        .expect("plan must succeed");
    assert!(
        inline_plan
            .iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "inline_plan = {inline_plan:?}"
    );

    // WHERE form: out of scope for this fix — stays a full scan.
    let where_plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person) WHERE n.tenantId = 't1' AND n.id = 1 RETURN n.name")
        .expect("plan must succeed");
    assert!(
        where_plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "WHERE-form composite equality is out of this fix's scope and \
         must remain a full scan; where_plan = {where_plan:?}"
    );

    let inline_result = engine
        .execute_cypher("MATCH (n:Person {tenantId: 't1', id: 1}) RETURN n.name")
        .expect("inline-form query must succeed");
    let where_result = engine
        .execute_cypher("MATCH (n:Person) WHERE n.tenantId = 't1' AND n.id = 1 RETURN n.name")
        .expect("where-form query must succeed");

    let inline_names: Vec<String> = inline_result
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name is a string")
                .to_string()
        })
        .collect();
    let where_names: Vec<String> = where_result
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name is a string")
                .to_string()
        })
        .collect();

    assert_eq!(
        inline_names, where_names,
        "the composite-seek plan and the full-scan plan must return \
         identical rows"
    );
    assert_eq!(inline_names, vec!["Alice".to_string()]);
}

/// PLAN-SHAPE (partial-match guard): an inline selector binding only
/// SOME of a registered composite index's columns must NOT plan a
/// `CompositeBtreeSeek` — the planner has no residual-filter wiring for
/// the un-seeked trailing column(s) on this path, so seeking on the
/// incomplete key would silently widen the result set. It must fall back
/// to a scan (or a single-property seek, if one exists).
#[test]
fn partial_inline_selector_does_not_mis_seek_composite_index() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher(
            "CREATE CONSTRAINT person_key FOR (p:Person) \
             REQUIRE (p.tenantId, p.id) IS NODE KEY",
        )
        .expect("NODE KEY DDL must succeed");

    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 1, name: 'Alice'})")
        .expect("seed t1/1");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't1', id: 2, name: 'Bob'})")
        .expect("seed t1/2");
    engine
        .execute_cypher("MERGE (p:Person {tenantId: 't2', id: 1, name: 'Carol'})")
        .expect("seed t2/1");

    // Only `tenantId` is bound — `id` is missing from the inline map.
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {tenantId: 't1'}) RETURN n.name")
        .expect("plan must succeed");

    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "a partial-key selector must never plan a CompositeBtreeSeek; \
         plan = {plan:?}"
    );

    let result = engine
        .execute_cypher("MATCH (n:Person {tenantId: 't1'}) RETURN n.name ORDER BY n.name")
        .expect("query must succeed");
    let names: Vec<String> = result
        .rows
        .iter()
        .map(|row| {
            row.values[0]
                .as_str()
                .expect("name is a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        names,
        vec!["Alice".to_string(), "Bob".to_string()],
        "both t1 tuples must be returned — the partial selector must not \
         mis-seek down to a single (or zero) tuple; got {names:?}"
    );
}
