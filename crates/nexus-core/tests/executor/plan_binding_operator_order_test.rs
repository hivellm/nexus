//! Plan-order guards for variable-BINDING operators versus `Filter`.
//!
//! `optimize_operator_order` (`crates/nexus-core/src/executor/planner/queries/cost.rs`)
//! buckets operators before recombining them in a fixed order. Any operator
//! that binds a pattern variable (a node or a relationship-traversal target)
//! must land in a bucket that is recombined BEFORE `filters`, or a residual
//! `Filter` referencing that variable runs against an unbound value — which
//! evaluates to `Null` and is therefore always false — and silently drops
//! every row.
//!
//! `Operator::VariableLengthPath` and `Operator::QuantifiedExpand` both bind
//! a `target_var` exactly like `Operator::Expand`, but were falling through
//! the bucketing match's `_ => others` catch-all, which is recombined AFTER
//! `filters`. These tests pin both the observable symptom (zero rows for a
//! query that should match) and the underlying plan-shape defect (operator
//! index ordering), plus a regression lock for the `NodeIndexSeek` case that
//! was already fixed and must keep working.

use nexus_core::Engine;
use nexus_core::executor::types::Operator;
use nexus_core::testing::TestContext;

/// BEHAVIORAL: `b` is reachable from `a:A` via exactly one `:R` hop, which
/// is within the `*1..2` quantifier's range, and `b.name = 'x'` holds. The
/// query must return that one row. Before the fix, the residual `Filter`
/// runs before `VariableLengthPath` binds `b`, so `b` is `Null`, the
/// predicate is always false, and the query wrongly returns zero rows.
#[test]
fn variable_length_path_filter_on_bound_target_returns_matching_row() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:A)-[:R]->(:B {name: 'x'})")
        .expect("seed a reachable target");

    let result = engine
        .execute_cypher("MATCH (a:A)-[:R*1..2]->(b) WHERE b.name = 'x' RETURN b")
        .expect("variable-length path query must succeed");

    assert_eq!(
        result.rows.len(),
        1,
        "expected exactly 1 row for a target reachable within 1..2 hops \
         matching the WHERE predicate; got {} rows",
        result.rows.len()
    );
}

/// PLAN-ORDER guard (DISCRIMINATING): `VariableLengthPath` binds `b`, so it
/// must precede the `Filter` that references `b.name` in the recombined
/// plan. This test fails against the pre-fix bucketing, where
/// `VariableLengthPath` falls into the `others` bucket and `others` is
/// recombined after `filters`.
#[test]
fn variable_length_path_precedes_filter_on_its_bound_target() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:A)-[:R]->(:B {name: 'x'})")
        .expect("seed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (a:A)-[:R*1..2]->(b) WHERE b.name = 'x' RETURN b")
        .expect("plan must succeed");

    let path_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::VariableLengthPath { .. }))
        .unwrap_or_else(|| panic!("VariableLengthPath must be in the plan; plan = {plan:?}"));
    let filter_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::Filter { .. }))
        .unwrap_or_else(|| panic!("Filter must be in the plan; plan = {plan:?}"));

    assert!(
        path_idx < filter_idx,
        "VariableLengthPath must precede the Filter that references the \
         target variable it binds; plan = {plan:?}"
    );
}

/// BEHAVIORAL: a real Quantified Path Pattern (Cypher 25) with a named
/// inner boundary node forces the planner past the slice-1 legacy lowering
/// (`QuantifiedGroup::try_lower_to_var_length_rel` requires pure-glue inner
/// nodes) and into `Operator::QuantifiedExpand` — see
/// `crates/nexus-core/src/executor/planner/tests.rs`,
/// `test_plan_qpp_named_inner_node_emits_quantified_expand_with_one_hop`,
/// for the same query shape used to pin the plan-only contract.
///
/// The seed is deliberately adversarial: `a:A`'s only reachable target
/// within `{1,2}` iterations is `Target {name: 'y'}` — it does NOT satisfy
/// `b.name = 'x'`. A separate, unconnected `Decoy {name: 'x'}` node exists
/// elsewhere in the graph purely so the query's own (pre-existing, planner-
/// added) `AllNodesScan` for the trailing boundary node `b` has a candidate
/// that passes the `b.name = 'x'` predicate.
///
/// Before the fix, the residual `Filter` runs against whatever `b` the
/// `AllNodesScan` bound (matches `Decoy`, unrelated to reachability), and
/// only THEN does `QuantifiedExpand` overwrite `b` with the real reachable
/// target — without re-checking the predicate. The query wrongly returns
/// `Target` even though `Target.name != 'x'`. After the fix,
/// `QuantifiedExpand` overwrites `b` with `Target` BEFORE the `Filter`
/// runs, so the (correct) final `b.name = 'y'` fails the predicate and the
/// query correctly returns zero rows.
#[test]
fn quantified_expand_filter_runs_against_its_bound_target_not_a_stale_binding() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:A)-[:R]->(:Target {name: 'y'}), (:Decoy {name: 'x'})")
        .expect("seed a reachable non-matching target and an unreachable decoy");

    let result = engine
        .execute_cypher("MATCH (a:A)( (n)-[:R]->(m) ){1,2}(b) WHERE b.name = 'x' RETURN b")
        .expect("quantified path pattern query must succeed");

    assert_eq!(
        result.rows.len(),
        0,
        "the only node reachable via the QPP body has name 'y', not 'x'; \
         the WHERE predicate must reject it regardless of an unrelated \
         'x'-named decoy node existing elsewhere in the graph; got {} rows",
        result.rows.len()
    );
}

/// PLAN-ORDER guard (DISCRIMINATING): `QuantifiedExpand` binds `b`
/// (`target_var`), so it must precede the `Filter` that references
/// `b.name`. Fails against the pre-fix bucketing for the same reason as
/// `VariableLengthPath` above — `QuantifiedExpand` also fell into the
/// `others` catch-all.
#[test]
fn quantified_expand_precedes_filter_on_its_bound_target() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:A)-[:R]->(:B {name: 'x'})")
        .expect("seed");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (a:A)( (n)-[:R]->(m) ){1,2}(b) WHERE b.name = 'x' RETURN b")
        .expect("plan must succeed");

    let qpp_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::QuantifiedExpand { .. }))
        .unwrap_or_else(|| panic!("QuantifiedExpand must be in the plan; plan = {plan:?}"));
    let filter_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::Filter { .. }))
        .unwrap_or_else(|| panic!("Filter must be in the plan; plan = {plan:?}"));

    assert!(
        qpp_idx < filter_idx,
        "QuantifiedExpand must precede the Filter that references the \
         target variable it binds; plan = {plan:?}"
    );
}

/// REGRESSION LOCK: `NodeIndexSeek` binds its node variable exactly like a
/// label scan, and the bucketing already routes it into `scans` (fixed
/// separately, see `correlated_index_seek_e2e_test.rs`). This must keep
/// passing after the `VariableLengthPath` / `QuantifiedExpand` bucketing
/// change above — the fix must not touch the scans arm's existing members.
#[test]
fn node_index_seek_precedes_filter_and_correctly_returns_no_rows() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:Person {id: 'b', age: 25})")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Person) ON (n.id)")
        .expect("create index");

    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:Person {id: 'b'}) WHERE n.age > 30 RETURN n")
        .expect("plan must succeed");

    let seek_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::NodeIndexSeek { .. }))
        .unwrap_or_else(|| panic!("NodeIndexSeek must be in the plan; plan = {plan:?}"));
    let filter_idx = plan
        .iter()
        .position(|op| matches!(op, Operator::Filter { .. }))
        .unwrap_or_else(|| panic!("Filter must be in the plan; plan = {plan:?}"));

    assert!(
        seek_idx < filter_idx,
        "NodeIndexSeek must precede the Filter that references the node \
         variable it binds; plan = {plan:?}"
    );

    let result = engine
        .execute_cypher("MATCH (n:Person {id: 'b'}) WHERE n.age > 30 RETURN n")
        .expect("query must succeed");

    assert_eq!(
        result.rows.len(),
        0,
        "age 25 does not satisfy `age > 30`; expected 0 rows, got {}",
        result.rows.len()
    );
}
