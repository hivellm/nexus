use super::*;

// ---------------------------------------------------------------
// QPP planner-shape tests — phase6_opencypher-quantified-path-patterns §4.4
// ---------------------------------------------------------------

/// Shared catalog for the QPP planner-shape tests. Spinning up a
/// fresh LMDB env per QPP test was tripping `MDB_TLS_FULL` when the
/// full lib suite ran in parallel — too many envs across too many
/// threads exhausted the LMDB TLS slot pool. Sharing one read-only
/// catalog across all `parse_and_plan` callers stays safe because
/// the planner only reads label/type ids; nothing under test
/// mutates catalog state.
fn shared_qpp_test_catalog() -> std::sync::MutexGuard<'static, Catalog> {
    use std::sync::{Mutex, OnceLock};
    static SHARED: OnceLock<Mutex<Catalog>> = OnceLock::new();
    let mutex = SHARED.get_or_init(|| {
        let ctx = TestContext::new();
        let path = ctx.path().join("qpp_planner_catalog.mdb");
        // Leak the temp-dir guard for the process lifetime so the
        // backing files survive every test run.
        let _leaked = Box::leak(Box::new(ctx));
        let catalog = Catalog::with_isolated_path(path, crate::catalog::CATALOG_MMAP_INITIAL_SIZE)
            .expect("Failed to create shared QPP catalog");
        Mutex::new(catalog)
    });
    mutex.lock().expect("shared QPP catalog poisoned")
}

/// Helper: parse the cypher source through the public parser so the
/// planner sees the same AST a real query would. The hand-built
/// `CypherQuery` literals above are too verbose for QPP shapes.
fn parse_and_plan(cypher: &str) -> Vec<Operator> {
    let catalog = shared_qpp_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser
        .parse()
        .unwrap_or_else(|e| panic!("parse `{cypher}`: {e}"));
    planner
        .plan_query(&query)
        .unwrap_or_else(|e| panic!("plan `{cypher}`: {e}"))
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_anonymous_body_lowers_to_variable_length_path() {
    // Slice-1: `( ()-[:T]->() ){m,n}` collapses at parse time to
    // a `RelationshipPattern` with quantifier — the planner sees
    // the legacy shape and emits `VariableLengthPath`, never
    // `QuantifiedExpand`.
    let operators = parse_and_plan("MATCH (a)( ()-[:KNOWS]->() ){1,5}(b) RETURN a, b");
    assert!(
        operators
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "anonymous-body QPP must lower to VariableLengthPath: {operators:?}",
    );
    assert!(
        !operators
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "anonymous-body QPP must NOT reach the slice-2 operator: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_named_inner_node_emits_quantified_expand_with_one_hop() {
    // Slice-2: a named or labelled inner boundary node forces
    // list-promotion semantics, so the lowering bows out and the
    // planner emits `QuantifiedExpand` instead. `hops.len() == 1`
    // because the body is single-relationship.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN x");
    let qpp = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand {
                hops, inner_nodes, ..
            } => Some((hops, inner_nodes)),
            _ => None,
        })
        .expect("named-inner QPP must emit QuantifiedExpand");
    assert_eq!(qpp.0.len(), 1, "single-rel body has hops.len() == 1");
    assert_eq!(
        qpp.1.len(),
        2,
        "single-rel body has inner_nodes.len() == hops.len() + 1"
    );
    assert!(
        !operators
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "named-inner QPP must NOT lower to VariableLengthPath: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_multi_hop_body_emits_quantified_expand_with_n_hops() {
    // Slice-3a: multi-hop bodies plan as a single
    // `QuantifiedExpand` with `hops.len() == n`. The planner walks
    // every Node-Relationship-Node alternation and bundles them
    // into the operator's `hops` / `inner_nodes` vectors.
    let operators = parse_and_plan(
        "MATCH (a)( (x:Person)-[:KNOWS]->(y:Person)-[:KNOWS]->(z:Person) ){1,3}(b) \
         RETURN x, y, z",
    );
    let qpp = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand {
                hops, inner_nodes, ..
            } => Some((hops, inner_nodes)),
            _ => None,
        })
        .expect("multi-hop QPP must emit QuantifiedExpand");
    assert_eq!(qpp.0.len(), 2, "two-rel body has hops.len() == 2");
    assert_eq!(
        qpp.1.len(),
        3,
        "two-rel body has inner_nodes.len() == hops.len() + 1"
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_legacy_var_length_rewrite_to_qpp_is_opt_in() {
    // Slice-3b §6.5 — the legacy `*m..n` planner branch can be
    // rewritten to emit `QuantifiedExpand` instead of
    // `VariableLengthPath`. The flag is process-wide
    // (`set_qpp_legacy_rewrite_enabled` / `qpp_legacy_rewrite_enabled`)
    // and starts disabled unless `NEXUS_QPP_REWRITE_LEGACY=1` was
    // set at startup. This test pins both halves of the contract.
    //
    // The shared QPP catalog mutex (`shared_qpp_test_catalog`)
    // serialises every `parse_and_plan` call across the planner
    // test module, so flipping the static flag here is safe even
    // under `cargo test` parallel execution: the lock is held for
    // the entire `parse_and_plan` body, and nobody else reads the
    // flag outside that body. We restore the flag on exit so a
    // failure mid-test doesn't leak state.
    let previous = qpp_legacy_rewrite_enabled();

    // Default branch: legacy operator stays on.
    set_qpp_legacy_rewrite_enabled(false);
    let default_ops = parse_and_plan("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN b");
    assert!(
        default_ops
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "default planner must keep emitting VariableLengthPath: {default_ops:?}"
    );
    assert!(
        !default_ops
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "default planner must NOT emit QuantifiedExpand for legacy *m..n: {default_ops:?}"
    );

    // Opt-in branch: flag flipped on.
    set_qpp_legacy_rewrite_enabled(true);
    let rewrite_ops = parse_and_plan("MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) RETURN b");
    assert!(
        rewrite_ops
            .iter()
            .any(|op| matches!(op, Operator::QuantifiedExpand { .. })),
        "with the rewrite flag set, the planner must emit QuantifiedExpand: {rewrite_ops:?}"
    );
    assert!(
        !rewrite_ops
            .iter()
            .any(|op| matches!(op, Operator::VariableLengthPath { .. })),
        "with rewrite on, VariableLengthPath must not appear: {rewrite_ops:?}"
    );

    set_qpp_legacy_rewrite_enabled(previous);
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_starting_node_uses_label_scan_upstream() {
    // Slice-3b §4.2: the QPP operator does not pick its own
    // source — it consumes whatever upstream operator the
    // surrounding pattern emits. When the source pattern carries
    // a label, the planner must emit `NodeByLabel` (or
    // `IndexScan`, gated on index availability) *before*
    // `QuantifiedExpand` so the source-side rows are already
    // narrowed by the time the expansion runs.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN x");
    let label_scan_idx = operators
        .iter()
        .position(|op| matches!(op, Operator::NodeByLabel { .. }))
        .expect("source-side label scan must be planned");
    let qpp_idx = operators
        .iter()
        .position(|op| matches!(op, Operator::QuantifiedExpand { .. }))
        .expect("QPP must emit QuantifiedExpand");
    assert!(
        label_scan_idx < qpp_idx,
        "source-side NodeByLabel must precede QuantifiedExpand: {operators:?}",
    );
}

#[test]
#[serial_test::serial(qpp_legacy_rewrite_flag)]
fn test_plan_qpp_named_body_target_var_chains_to_following_node() {
    // The QPP planner threads `prev_node_var` so a follow-up
    // pattern element after the QPP gets the right source. When
    // `(b)` follows the QPP without the operator binding the
    // target to `b`, downstream Expands break. Pin the contract:
    // a named trailing boundary node must end up as the operator's
    // `target_var`.
    let operators =
        parse_and_plan("MATCH (a:Person)( (x:Person)-[:KNOWS]->() ){1,3}(b:Person) RETURN b");
    let target_var = operators
        .iter()
        .find_map(|op| match op {
            Operator::QuantifiedExpand { target_var, .. } => Some(target_var),
            _ => None,
        })
        .expect("named-inner QPP must emit QuantifiedExpand");
    assert_eq!(
        target_var, "b",
        "trailing named boundary node must wire to the operator's target_var"
    );
}
