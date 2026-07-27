use super::*;

// ───────────────────────────────────────────────────────────────────
// Join Inner cost estimation must use CARDINALITY, not COST, for its
// per-side inputs — `estimate_operator_cost`'s `Operator::Join` arm
// previously called `estimate_plan_cost` (a cost figure, in abstract
// cost units) and fed the result into variables consumed as row-count
// cardinality by the cartesian-product / output-cardinality estimate.
// ───────────────────────────────────────────────────────────────────

#[test]
fn join_inner_cost_derives_input_sizes_from_cardinality_not_cost() {
    let (catalog, _ctx) = create_test_catalog();
    let label_a = catalog.get_or_create_label("A").expect("label a");
    let label_b = catalog.get_or_create_label("B").expect("label b");

    // Two disjoint label populations with KNOWN, distinct sizes so the
    // NodeByLabel operands have an unambiguous true cardinality (4 and
    // 3 nodes) that differs sharply from what its COST would be.
    let label_index = LabelIndex::new();
    for node_id in 0..4u64 {
        label_index
            .add_node(node_id, &[label_a])
            .expect("index A node");
    }
    for node_id in 4..7u64 {
        label_index
            .add_node(node_id, &[label_b])
            .expect("index B node");
    }

    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let planner = QueryPlanner::new(&catalog, &label_index, &knn_index);

    let join_op = Operator::Join {
        left: Box::new(Operator::NodeByLabel {
            label_id: label_a,
            variable: "a".to_string(),
        }),
        right: Box::new(Operator::NodeByLabel {
            label_id: label_b,
            variable: "b".to_string(),
        }),
        join_type: JoinType::Inner,
        condition: Some("a.id = b.id".to_string()),
    };

    let join_cost = planner
        .estimate_plan_cost(std::slice::from_ref(&join_op))
        .expect("join cost estimate");

    // True cardinalities of each NodeByLabel side (row counts), matching
    // what `estimate_operator_cardinality` reports for a `NodeByLabel`
    // whose bitmap has 4 / 3 entries respectively.
    let left_cardinality = 4.0_f64;
    let right_cardinality = 3.0_f64;
    // Hash-join cost model from the `Operator::Join` Inner arm:
    // build_cost = left_cardinality * 5.0, probe_cost = right_cardinality * 3.0.
    let expected_join_cost = left_cardinality * 5.0 + right_cardinality * 3.0;

    assert!(
        (join_cost - expected_join_cost).abs() < 1e-9,
        "Join Inner cost must be built from each side's CARDINALITY \
         (row count) via estimate_operator_cardinality, matching the \
         Union arm's pattern — expected {expected_join_cost}, got \
         {join_cost}"
    );

    // Sanity check: the pre-fix formula fed `estimate_plan_cost`'s COST
    // output (NodeByLabel costs 12x its row count: 10 I/O + 2 CPU per
    // node) into these same cardinality slots. Confirm the fixed value
    // is NOT that inflated figure, so this assertion is not
    // accidentally tautological with the bug still present.
    let pre_fix_buggy_cost = (left_cardinality * 12.0) * 5.0 + (right_cardinality * 12.0) * 3.0;
    assert!(
        (join_cost - pre_fix_buggy_cost).abs() > 1.0,
        "join cost must diverge from the pre-fix cost-as-cardinality \
         formula (got {join_cost}, buggy formula would give \
         {pre_fix_buggy_cost})"
    );
}
