use super::*;

// ───────────────────────────────────────────────────────────────────
// composite-index detection for inline multi-property selectors
// (`MATCH (n:L {a: 1, b: 2})` against a registered composite index).
// ───────────────────────────────────────────────────────────────────

/// Helper: parse + plan with an explicit `CompositeBtreeRegistry` handle.
fn plan_with_composite_index(
    cypher: &str,
    catalog: &Catalog,
    registry: &crate::index::composite_btree::CompositeBtreeRegistry,
) -> std::result::Result<Vec<Operator>, crate::Error> {
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner =
        QueryPlanner::new(catalog, &label_index, &knn_index).with_composite_index(registry);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser.parse()?;
    planner.plan_query(&query)
}

#[test]
fn composite_index_seek_emitted_for_full_inline_selector_match() {
    // A composite index on (a, b) plus an inline selector that binds
    // BOTH columns as plan-time literals must plan a
    // `CompositeBtreeSeek`, not a `NodeByLabel` scan.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("L").expect("label");
    catalog.get_or_create_key("a").expect("key a");
    catalog.get_or_create_key("b").expect("key b");

    let registry = crate::index::composite_btree::CompositeBtreeRegistry::new();
    registry
        .register(
            label_id,
            vec!["a".to_string(), "b".to_string()],
            false,
            None,
            false,
        )
        .expect("register composite index");

    let ops = plan_with_composite_index("MATCH (n:L {a: 1, b: 2}) RETURN n", &catalog, &registry)
        .expect("plan must succeed");

    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "full inline-selector match on a composite index must plan a \
         CompositeBtreeSeek; plan = {ops:?}"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "a composite-index-covered selector must not fall back to a \
         NodeByLabel scan; plan = {ops:?}"
    );

    if let Some(Operator::CompositeBtreeSeek {
        label,
        variable,
        prefix,
    }) = ops
        .iter()
        .find(|op| matches!(op, Operator::CompositeBtreeSeek { .. }))
    {
        assert_eq!(label, "L");
        assert_eq!(variable, "n");
        assert_eq!(
            prefix,
            &vec![
                ("a".to_string(), serde_json::Value::from(1i64)),
                ("b".to_string(), serde_json::Value::from(2i64)),
            ]
        );
    }
}

#[test]
fn composite_index_seek_not_emitted_without_registry_handle() {
    // No `with_composite_index` call → legacy behaviour: the planner
    // never emits `CompositeBtreeSeek`, even when the inline selector
    // binds every column a composite index would cover.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("L").expect("label");
    catalog.get_or_create_key("a").expect("key a");
    catalog.get_or_create_key("b").expect("key b");

    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser = CypherParser::new("MATCH (n:L {a: 1, b: 2}) RETURN n".to_string());
    let query = parser.parse().expect("parse");
    let ops = planner.plan_query(&query).expect("plan");

    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "without a registry handle, no CompositeBtreeSeek should ever be \
         planned; plan = {ops:?}"
    );
}

#[test]
fn composite_index_partial_selector_does_not_mis_seek() {
    // A composite index on (a, b) with an inline selector that binds
    // ONLY `a` must NOT plan a `CompositeBtreeSeek` — the planner has
    // no residual-filter wiring for the un-seeked trailing column on
    // this path, so seeking on the incomplete key would silently widen
    // the result set. It must fall back to a single-property seek (if
    // one exists) or a full scan instead.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("L").expect("label");
    catalog.get_or_create_key("a").expect("key a");
    catalog.get_or_create_key("b").expect("key b");

    let registry = crate::index::composite_btree::CompositeBtreeRegistry::new();
    registry
        .register(
            label_id,
            vec!["a".to_string(), "b".to_string()],
            false,
            None,
            false,
        )
        .expect("register composite index");

    let ops = plan_with_composite_index("MATCH (n:L {a: 1}) RETURN n", &catalog, &registry)
        .expect("plan must succeed");

    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "a partial-key selector must never plan a CompositeBtreeSeek; \
         plan = {ops:?}"
    );
    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "a partial-key selector with no covering single-property index \
         must fall back to a NodeByLabel scan; plan = {ops:?}"
    );
}

#[test]
fn composite_index_seek_preferred_over_single_property_index() {
    // When BOTH a composite index on (a, b) and a single-property index
    // on `a` alone could apply to the same inline selector, the
    // composite seek wins (narrows straight to the exact tuple instead
    // of leaving a residual Filter on `b`).
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("L").expect("label");
    let key_a = catalog.get_or_create_key("a").expect("key a");
    catalog.get_or_create_key("b").expect("key b");

    let registry = crate::index::composite_btree::CompositeBtreeRegistry::new();
    registry
        .register(
            label_id,
            vec!["a".to_string(), "b".to_string()],
            false,
            None,
            false,
        )
        .expect("register composite index");

    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_a)
        .expect("create index on a");

    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index)
        .with_composite_index(&registry)
        .with_property_index(&prop_idx);
    let mut parser = CypherParser::new("MATCH (n:L {a: 1, b: 2}) RETURN n".to_string());
    let query = parser.parse().expect("parse");
    let ops = planner.plan_query(&query).expect("plan");

    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::CompositeBtreeSeek { .. })),
        "composite seek must win over the single-property seek; plan = {ops:?}"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "the single-property NodeIndexSeek must not also be emitted; plan = {ops:?}"
    );
}
