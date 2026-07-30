//! Regression tests for inline label enforcement on `Expand` TARGET nodes.
//!
//! Before this fix, `Operator::Expand` carried no notion of the target
//! node pattern's inline label(s) at all: the planner's pattern-lowering
//! pass collected every relationship's next-node variable into
//! `all_target_nodes` and then skipped emitting a `NodeByLabel`/`Filter`
//! for it (the intent, per an existing comment, was "that case is handled
//! by the Expand operator's target_var path" — but `execute_expand` never
//! actually checked labels). Two confirmed manifestations, both probed on
//! the openCypher TCK `useCases/triadicSelection` binary-tree-2 fixture:
//!
//! - REQUIRED expand: `MATCH (a:A)-[:KNOWS]->(b:X)-->(c:X)` returned
//!   candidates for `c` that did not carry `:X`.
//! - OPTIONAL expand: `MATCH (a:A) OPTIONAL MATCH (a)-[r:T]->(c:C)` bound
//!   `c` to a wrongly-labeled candidate (making `r IS NULL` false) instead
//!   of NULL-padding the row per LEFT-OUTER semantics.

use nexus_core::testing::setup_isolated_test_engine;
use serde_json::Value;
use std::collections::HashMap;

fn engine() -> nexus_core::Engine {
    let (engine, ctx) = setup_isolated_test_engine().unwrap();
    std::mem::forget(ctx);
    engine
}

/// Probe 1: a 2-hop REQUIRED chain where the last hop's target label
/// discriminates between two otherwise-identical candidates.
#[test]
fn required_expand_rejects_wrongly_labeled_multi_hop_target() {
    let mut engine = engine();
    engine
        .execute_cypher(
            "CREATE (a:A {name:'a'})-[:KNOWS]->(b:X {name:'b'}), \
             (b)-[:R]->(c1:X {name:'c1'}), \
             (b)-[:R]->(c2:Y {name:'c2'})",
        )
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:A)-[:KNOWS]->(b:X)-->(c:X) RETURN c.name")
        .unwrap();

    let names: Vec<Value> = rs.rows.into_iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        names,
        vec![Value::from("c1")],
        "must include only the :X-labeled candidate, excluding the :Y-labeled sibling"
    );
}

/// Probe 2: OPTIONAL expand whose only reachable candidate carries the
/// wrong label must NULL-pad (`c` and `r` both NULL), not bind to it.
#[test]
fn optional_expand_null_pads_when_only_candidate_has_wrong_label() {
    let mut engine = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:T]->(d:D {id: 2})")
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:A) OPTIONAL MATCH (a)-[r:T]->(c:C) RETURN a.id, c, r")
        .unwrap();

    assert_eq!(rs.rows.len(), 1, "the anchor row must still be preserved");
    assert_eq!(
        rs.rows[0].values,
        vec![Value::from(1), Value::Null, Value::Null],
        "a label-rejected candidate must count as no match, not a wrong binding"
    );
}

/// Control for probe 2: OPTIONAL expand must still bind normally when the
/// candidate's label DOES match — guards against the fix over-rejecting.
#[test]
fn optional_expand_binds_when_candidate_label_matches() {
    let mut engine = engine();
    engine
        .execute_cypher("CREATE (a:A {id: 1})-[:T]->(c:C {id: 2})")
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:A) OPTIONAL MATCH (a)-[r:T]->(c:C) RETURN a.id, c.id")
        .unwrap();

    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::from(2)]);
}

/// Distinct multi-hop case beyond probe 1: a 3-hop chain where EVERY hop
/// declares its own target label, and every hop has a same-source
/// wrongly-labeled sibling that must be excluded independently — proves
/// the fix is not a one-hop-only special case (it must fire once per
/// `Expand` operator emitted for the chain).
#[test]
fn required_expand_enforces_label_on_every_hop_of_three_hop_chain() {
    let mut engine = engine();
    engine
        .execute_cypher(
            "CREATE (a:A {name:'a'})-[:R]->(b:X {name:'b_ok'}), \
             (a)-[:R]->(bw:Y {name:'b_wrong'}), \
             (b)-[:R]->(c:X {name:'c_ok'}), \
             (b)-[:R]->(cw:Y {name:'c_wrong'}), \
             (c)-[:R]->(d:X {name:'d_ok'}), \
             (c)-[:R]->(dw:Y {name:'d_wrong'})",
        )
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:A)-->(b:X)-->(c:X)-->(d:X) RETURN d.name")
        .unwrap();

    let names: Vec<Value> = rs.rows.into_iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        names,
        vec![Value::from("d_ok")],
        "every hop's :X target-label constraint must independently reject its :Y sibling"
    );
}

/// Undirected variant: the target-label check must also fire on the
/// `Direction::Both` candidate-resolution branch, not just Outgoing.
#[test]
fn required_expand_enforces_target_label_on_undirected_hop() {
    let mut engine = engine();
    engine
        .execute_cypher(
            "CREATE (a:A {name:'a'})-[:KNOWS]->(b:X {name:'b'}), \
             (b)-[:R]->(c1:X {name:'c1'}), \
             (c2:Y {name:'c2'})-[:R]->(b)",
        )
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:A)-[:KNOWS]->(b:X)--(c:X) RETURN c.name")
        .unwrap();

    let names: Vec<Value> = rs.rows.into_iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        names,
        vec![Value::from("c1")],
        "undirected hop must still exclude the :Y-labeled candidate reached via the incoming edge"
    );
}

/// Multi-label target predicate (`(b:X:Y)`) must be an AND-intersection —
/// mirrors the `(n:A:B)` semantics used elsewhere for anchor/WHERE label
/// checks. A candidate carrying only ONE of the two declared labels must
/// be rejected just like a candidate carrying neither.
#[test]
fn required_expand_enforces_multi_label_and_intersection_on_target() {
    let mut engine = engine();
    engine
        .execute_cypher(
            "CREATE (a:S {name:'a'})-[:R]->(bx:X {name:'bx'}), \
             (a)-[:R]->(bxy:X:Y {name:'bxy'})",
        )
        .unwrap();

    let rs = engine
        .execute_cypher("MATCH (a:S)-->(b:X:Y) RETURN b.name")
        .unwrap();

    let names: Vec<Value> = rs.rows.into_iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        names,
        vec![Value::from("bxy")],
        "a multi-label target predicate must require ALL declared labels, excluding the :X-only sibling"
    );
}

/// Reversed-pairing shape end-to-end: the OPTIONAL pattern's WRITTEN
/// source (`new`) is unbound while its WRITTEN target (`bound`) is
/// already bound from a prior clause, so the planner reverses the
/// traversal direction and swaps source/target roles (see
/// `add_relationship_operators`'s `target_bound && !source_bound`
/// branch). The label predicate on `new` must still travel with it even
/// though it ends up occupying the operator's `target_var` slot after
/// the swap — a wrongly-labeled candidate must NULL-pad the row, exactly
/// like the non-reversed case.
#[test]
fn optional_expand_reversed_pairing_null_pads_when_new_node_has_wrong_label() {
    let mut engine = engine();
    engine
        .execute_cypher("CREATE (bound:B {id: 1}), (wrong:Y {id: 2})-[:T]->(bound)")
        .unwrap();

    let rs = engine
        .execute_cypher(
            "MATCH (bound:B) OPTIONAL MATCH (new:X)-[r:T]->(bound) RETURN bound.id, new, r",
        )
        .unwrap();

    assert_eq!(rs.rows.len(), 1, "the anchor row must still be preserved");
    assert_eq!(
        rs.rows[0].values,
        vec![Value::from(1), Value::Null, Value::Null],
        "the reversed-traversal target's :X label predicate must reject the :Y-labeled candidate, not bind to it"
    );
}

/// Control for the reversed-pairing case: when the discovered candidate
/// DOES carry the declared label, the swapped Expand must still bind it
/// normally — guards against the fix over-rejecting once roles are
/// swapped.
#[test]
fn optional_expand_reversed_pairing_binds_when_new_node_has_matching_label() {
    let mut engine = engine();
    engine
        .execute_cypher("CREATE (bound:B {id: 1}), (right:X {id: 3})-[:T]->(bound)")
        .unwrap();

    let rs = engine
        .execute_cypher(
            "MATCH (bound:B) OPTIONAL MATCH (new:X)-[r:T]->(bound) RETURN bound.id, new.id",
        )
        .unwrap();

    assert_eq!(rs.rows.len(), 1);
    assert_eq!(rs.rows[0].values, vec![Value::from(1), Value::from(3)]);
}

/// A `$param` dynamic-label sentinel on the target node must be resolved
/// against the runtime parameter map (never at plan time — see
/// `dynamic_label_read_path_test`'s sibling coverage for the anchor-node
/// case) and enforced exactly like a static label: the resolved label
/// name rejects the wrongly-labeled sibling, and a missing/invalid
/// binding raises the same typed `ERR_INVALID_LABEL` the WHERE-clause and
/// anchor-node dynamic-label paths already raise.
#[test]
fn required_expand_target_label_resolves_dynamic_param_sentinel() {
    let mut engine = engine();
    engine
        .execute_cypher(
            "CREATE (a:A {name:'a'})-[:R]->(bx:X {name:'bx'}), \
             (a)-[:R]->(by:Y {name:'by'})",
        )
        .unwrap();

    let mut params: HashMap<String, Value> = HashMap::new();
    params.insert("targetLabel".to_string(), Value::from("X"));
    let rs = engine
        .execute_cypher_with_params("MATCH (a:A)-->(b:$targetLabel) RETURN b.name", params)
        .expect("dynamic target-label sentinel must resolve against the bound param");

    let names: Vec<Value> = rs.rows.into_iter().map(|r| r.values[0].clone()).collect();
    assert_eq!(
        names,
        vec![Value::from("bx")],
        "the resolved $targetLabel param must reject the :Y-labeled sibling"
    );

    let err = engine
        .execute_cypher("MATCH (a:A)-->(b:$targetLabel) RETURN b.name")
        .expect_err("a missing $targetLabel binding must raise a typed error, not silently match nothing or everything");
    assert!(
        err.to_string().contains("ERR_INVALID_LABEL"),
        "expected ERR_INVALID_LABEL, got: {err}"
    );
}
