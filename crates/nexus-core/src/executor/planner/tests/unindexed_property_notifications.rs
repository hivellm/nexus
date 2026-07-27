use super::*;

// ───────────────────────────────────────────────────────────────────
// phase6_merge-unindexed-property-warning — `Nexus.Performance.
// UnindexedPropertyAccess` notification emission.
// ───────────────────────────────────────────────────────────────────

/// Helper: parse + plan with an explicit `PropertyIndex` handle and
/// return both the operators and the planner's drained notifications.
/// Mirrors `plan_with_property_index` but exposes the diagnostic side
/// channel so tests can assert on hint emission.
fn plan_with_notifications(
    cypher: &str,
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
) -> (Vec<Operator>, Vec<crate::executor::types::Notification>) {
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner =
        QueryPlanner::new(catalog, &label_index, &knn_index).with_property_index(prop_idx);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser.parse().expect("parse");
    let ops = planner.plan_query(&query).expect("plan");
    let notes = planner.take_notifications();
    (ops, notes)
}

#[test]
fn unindexed_property_notification_emitted_for_merge_inline_selector() {
    // `MERGE (n:Artifact { natural_key: $v })` with no index on
    // (Artifact, natural_key) → planner emits one
    // `Nexus.Performance.UnindexedPropertyAccess` notification.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MERGE (n:Artifact { natural_key: 'sha256:abc' }) RETURN count(n) AS c",
        &catalog,
        &prop_idx,
    );

    assert_eq!(
        notes.len(),
        1,
        "expected exactly one notification, got: {notes:?}"
    );
    let n = &notes[0];
    assert_eq!(n.code, "Nexus.Performance.UnindexedPropertyAccess");
    assert!(
        n.title.contains("Artifact"),
        "title should name the label: {}",
        n.title
    );
    assert!(
        n.title.contains("natural_key"),
        "title should name the property: {}",
        n.title
    );
    assert!(
        n.description.contains("MERGE"),
        "description should name the offending clause: {}",
        n.description
    );
    assert!(
        n.description
            .contains("CREATE INDEX FOR (n:Artifact) ON (n.natural_key)"),
        "description should include the suggested DDL verbatim: {}",
        n.description
    );
}

#[test]
fn unindexed_property_notification_emitted_for_match_inline_selector() {
    // Same as the MERGE case but the offending clause is MATCH; the
    // emitter must label the clause correctly.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("path").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (a:Artifact { path: '/etc/x' }) RETURN a",
        &catalog,
        &prop_idx,
    );

    assert_eq!(notes.len(), 1, "expected one notification, got: {notes:?}");
    assert!(
        notes[0].description.contains("MATCH"),
        "description should name MATCH: {}",
        notes[0].description
    );
}

#[test]
fn unindexed_property_notification_emitted_for_where_equality() {
    // `MATCH (n:Label) WHERE n.prop = $v` form — the WHERE walker
    // resolves `n` to its label via the variable-binding map and
    // emits the notification when no index covers (Label, prop).
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (n:Artifact) WHERE n.natural_key = 'x' RETURN n",
        &catalog,
        &prop_idx,
    );

    assert!(
        !notes.is_empty(),
        "WHERE equality on unindexed property should emit a notification"
    );
    assert_eq!(notes[0].code, "Nexus.Performance.UnindexedPropertyAccess");
    assert!(
        notes[0].title.contains("natural_key"),
        "title should name the property"
    );
}

#[test]
fn unindexed_property_notification_suppressed_when_index_exists() {
    // With a registered index for (Artifact, natural_key), the
    // planner must NOT emit the notification — false positives would
    // teach operators to ignore the hint.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Artifact").expect("label");
    let key_id = catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_id)
        .expect("create index");

    let (_ops, notes) = plan_with_notifications(
        "MERGE (n:Artifact { natural_key: 'x' }) RETURN n",
        &catalog,
        &prop_idx,
    );

    assert!(
        notes.is_empty(),
        "no notification expected when the index exists, got: {notes:?}"
    );
}

#[test]
fn unindexed_property_notification_deduplicated_within_single_plan() {
    // Two clauses referencing the same (label, prop) pair — the
    // planner emits ONE notification, not two, so a query with
    // both MATCH and MERGE on the same selector is not noisy.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();

    let (_ops, notes) = plan_with_notifications(
        "MATCH (a:Artifact { natural_key: 'x' }) WITH a \
         MERGE (b:Artifact { natural_key: 'x' }) RETURN a, b",
        &catalog,
        &prop_idx,
    );

    assert_eq!(
        notes.len(),
        1,
        "duplicate (label, prop) selector should emit one notification, got: {notes:?}"
    );
}

#[test]
fn unindexed_property_notification_no_op_without_property_index_handle() {
    // Planners constructed without `with_property_index(...)` — the
    // standalone `Executor::parse_and_plan` path and existing planner
    // unit tests — must not emit notifications. Removing the catalog
    // handle entirely avoids surprises in callers that haven't opted
    // into diagnostics.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Artifact").expect("label");
    catalog.get_or_create_key("natural_key").expect("key");
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser =
        CypherParser::new("MERGE (n:Artifact { natural_key: 'x' }) RETURN n".to_string());
    let query = parser.parse().expect("parse");
    let _ops = planner.plan_query(&query).expect("plan");
    let notes = planner.take_notifications();
    assert!(
        notes.is_empty(),
        "no notifications expected, got: {notes:?}"
    );
}
