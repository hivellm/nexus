use super::*;

// ───────────────────────────────────────────────────────────────────
// phase7_planner-using-index-hints — USING INDEX validation tests
// ───────────────────────────────────────────────────────────────────

/// Helper: parse + plan with an explicit `PropertyIndex` handle.
#[test]
fn where_range_predicate_lifts_to_node_index_range_seek() {
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Person").expect("label");
    let key_id = catalog.get_or_create_key("age").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_id)
        .expect("create index");

    for (cypher, expected) in [
        (
            "MATCH (n:Person) WHERE n.age > 30 RETURN n",
            RangeSeekOp::Gt,
        ),
        (
            "MATCH (n:Person) WHERE n.age >= 30 RETURN n",
            RangeSeekOp::Ge,
        ),
        (
            "MATCH (n:Person) WHERE n.age < 30 RETURN n",
            RangeSeekOp::Lt,
        ),
        (
            "MATCH (n:Person) WHERE n.age <= 30 RETURN n",
            RangeSeekOp::Le,
        ),
        // Literal on the left mirrors the operator.
        (
            "MATCH (n:Person) WHERE 30 < n.age RETURN n",
            RangeSeekOp::Gt,
        ),
    ] {
        let ops = plan_with_property_index(cypher, &catalog, &prop_idx).expect("plan");
        let found = ops.iter().find_map(|op| match op {
            Operator::NodeIndexRangeSeek { op, .. } => Some(*op),
            _ => None,
        });
        assert_eq!(
            found,
            Some(expected),
            "`{cypher}` must lift to a range seek; plan = {ops:?}"
        );
    }
}

#[test]
fn where_range_predicate_without_index_stays_a_scan() {
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Person").expect("label");
    catalog.get_or_create_key("age").expect("key");
    // No create_index -> no seek, plain NodeByLabel + Filter.
    let prop_idx = crate::index::PropertyIndex::new();
    let ops = plan_with_property_index(
        "MATCH (n:Person) WHERE n.age > 30 RETURN n",
        &catalog,
        &prop_idx,
    )
    .expect("plan");
    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexRangeSeek { .. })),
        "no index -> no range seek; plan = {ops:?}"
    );
}

/// `IN`, `STARTS WITH`, and `= $param` on an indexed property each lift to
/// their own seek operator; `CONTAINS` — unanchored, unseekable — does not.
/// See phase0_fix-where-in-prefix-param-index-seek.
#[test]
fn where_in_prefix_and_param_predicates_lift_to_their_seeks() {
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Person").expect("label");
    let age_id = catalog.get_or_create_key("age").expect("key");
    let name_id = catalog.get_or_create_key("name").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx.create_index(label_id, age_id).expect("index age");
    prop_idx
        .create_index(label_id, name_id)
        .expect("index name");

    let lifts = |cypher: &str| -> Vec<Operator> {
        plan_with_property_index(cypher, &catalog, &prop_idx).expect("plan")
    };

    let ops = lifts("MATCH (n:Person) WHERE n.age IN [1, 2, 3] RETURN n");
    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexInSeek { values, .. } if values.len() == 3)),
        "`IN` must lift to a 3-value NodeIndexInSeek; plan = {ops:?}"
    );

    let ops = lifts("MATCH (n:Person) WHERE n.name STARTS WITH 'A' RETURN n");
    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexPrefixSeek { prefix, .. } if prefix == "A")),
        "`STARTS WITH` must lift to a NodeIndexPrefixSeek; plan = {ops:?}"
    );

    let ops = lifts("MATCH (n:Person) WHERE n.age = $age RETURN n");
    assert!(
        ops.iter().any(
            |op| matches!(op, Operator::NodeIndexParamSeek { parameter, .. } if parameter == "age")
        ),
        "`= $param` must lift to a NodeIndexParamSeek; plan = {ops:?}"
    );
    assert!(
        ops.iter().any(|op| matches!(op, Operator::Filter { .. })),
        "the parameter predicate must survive as a residual Filter; plan = {ops:?}"
    );

    // CONTAINS is not seekable by any ordered index — it must stay a scan.
    let ops = lifts("MATCH (n:Person) WHERE n.name CONTAINS 'A' RETURN n");
    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "`CONTAINS` must stay a NodeByLabel scan; plan = {ops:?}"
    );
}

/// A literal-keyed seek outranks the parameter seek: it cannot degrade to a
/// scan at execution time, so it is always the better plan.
#[test]
fn literal_seek_wins_over_a_parameter_seek_in_the_same_where() {
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Person").expect("label");
    let age_id = catalog.get_or_create_key("age").expect("key");
    let name_id = catalog.get_or_create_key("name").expect("key");
    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx.create_index(label_id, age_id).expect("index age");
    prop_idx
        .create_index(label_id, name_id)
        .expect("index name");

    let ops = plan_with_property_index(
        "MATCH (n:Person) WHERE n.age = $age AND n.name = 'Alice' RETURN n",
        &catalog,
        &prop_idx,
    )
    .expect("plan");
    assert!(
        ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "the literal equality must win the lift; plan = {ops:?}"
    );
    assert!(
        !ops.iter()
            .any(|op| matches!(op, Operator::NodeIndexParamSeek { .. })),
        "only one seek seeds the scan; plan = {ops:?}"
    );
}

fn plan_with_property_index(
    cypher: &str,
    catalog: &Catalog,
    prop_idx: &crate::index::PropertyIndex,
) -> std::result::Result<Vec<Operator>, crate::Error> {
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner =
        QueryPlanner::new(catalog, &label_index, &knn_index).with_property_index(prop_idx);
    let mut parser = CypherParser::new(cypher.to_string());
    let query = parser.parse()?;
    planner.plan_query(&query)
}

#[test]
fn using_index_hint_accepted_silently_without_property_index_handle() {
    // No `with_property_index` call → planner returns Ok(...) without
    // validating the hint, matching legacy behaviour for unit-test
    // callers that don't carry an `IndexManager` handle.
    let (catalog, _ctx) = create_test_catalog();
    let label_index = LabelIndex::new();
    let knn_index = KnnIndex::new(crate::index::DEFAULT_VECTORIZER_DIMENSION).unwrap();
    let mut planner = QueryPlanner::new(&catalog, &label_index, &knn_index);
    let mut parser = CypherParser::new(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n".to_string(),
    );
    let query = parser.parse().expect("parse");
    let result = planner.plan_query(&query);
    assert!(
        result.is_ok(),
        "USING INDEX hint should be accepted silently when no PropertyIndex handle is installed; got {result:?}"
    );
}

#[test]
fn using_index_hint_validated_when_property_index_handle_installed_and_index_exists() {
    // With a `PropertyIndex` handle and a registered index for
    // (Person, email), the hint passes validation.
    let (catalog, _ctx) = create_test_catalog();
    let label_id = catalog.get_or_create_label("Person").expect("label");
    let key_id = catalog.get_or_create_key("email").expect("key");

    let prop_idx = crate::index::PropertyIndex::new();
    prop_idx
        .create_index(label_id, key_id)
        .expect("create index");

    let result = plan_with_property_index(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n",
        &catalog,
        &prop_idx,
    );
    assert!(
        result.is_ok(),
        "USING INDEX hint should pass validation when a matching property index exists; got {result:?}"
    );
}

#[test]
fn using_index_hint_errors_when_index_missing() {
    // Property index handle installed but no matching index for
    // (Person, email) → planner emits ERR_USING_INDEX_NOT_FOUND.
    let (catalog, _ctx) = create_test_catalog();
    catalog.get_or_create_label("Person").expect("label");
    catalog.get_or_create_key("email").expect("key");

    let prop_idx = crate::index::PropertyIndex::new();
    // Deliberately no `create_index` call — the registry is empty.

    let result = plan_with_property_index(
        "MATCH (n:Person) USING INDEX n:Person(email) WHERE n.email = 'a@b' RETURN n",
        &catalog,
        &prop_idx,
    );
    let err = result.expect_err("missing-index hint must error");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_USING_INDEX_NOT_FOUND"),
        "expected ERR_USING_INDEX_NOT_FOUND, got: {msg}"
    );
    assert!(
        msg.contains(":Person(email)"),
        "error message should name the (label, property) pair: {msg}"
    );
}

#[test]
fn using_index_hint_errors_when_label_missing_in_catalog() {
    // Hint references a label that was never registered in the
    // catalog. Planner short-circuits before consulting the index.
    let (catalog, _ctx) = create_test_catalog();
    let prop_idx = crate::index::PropertyIndex::new();

    let result = plan_with_property_index(
        "MATCH (n:Ghost) USING INDEX n:Ghost(id) WHERE n.id = 1 RETURN n",
        &catalog,
        &prop_idx,
    );
    let err = result.expect_err("hint on unknown label must error");
    let msg = err.to_string();
    assert!(
        msg.contains("ERR_USING_INDEX_NOT_FOUND"),
        "expected ERR_USING_INDEX_NOT_FOUND, got: {msg}"
    );
    assert!(
        msg.contains("Ghost"),
        "error message should name the unknown label: {msg}"
    );
}
