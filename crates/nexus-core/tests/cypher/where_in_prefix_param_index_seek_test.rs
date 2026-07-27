//! `IN`, `STARTS WITH`, and `$parameter` equality WHERE predicates on an
//! indexed property must lift to an index seek instead of a full label scan,
//! and must return exactly the rows the scan would.
//!
//! These run through a real `Engine` (not `create_isolated_test_executor`)
//! because only the engine installs a `PropertyIndex` handle on the executor —
//! without it the planner never lifts anything and a plan assertion would be
//! vacuous. See phase0_fix-where-in-prefix-param-index-seek.

use nexus_core::Engine;
use nexus_core::executor::types::Operator;
use nexus_core::testing::TestContext;
use serde_json::{Value, json};
use std::collections::HashMap;

/// An engine seeded with four `:P` nodes and an index on `:P(age)` /
/// `:P(name)`.
fn seeded() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    engine
        .execute_cypher(
            "CREATE (:P {age: 10, name: 'alpha'}), (:P {age: 20, name: 'alpine'}), \
             (:P {age: 30, name: 'beta'}), (:P {age: 40, name: 'gamma'})",
        )
        .expect("seed CREATE");
    engine
        .execute_cypher("CREATE INDEX age_idx FOR (n:P) ON (n.age)")
        .expect("CREATE INDEX age");
    engine
        .execute_cypher("CREATE INDEX name_idx FOR (n:P) ON (n.name)")
        .expect("CREATE INDEX name");
    (engine, ctx)
}

fn ages(engine: &mut Engine, cypher: &str) -> Vec<i64> {
    let mut v: Vec<i64> = engine
        .execute_cypher(cypher)
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"))
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().expect("integer column"))
        .collect();
    v.sort_unstable();
    v
}

fn ages_with_params(engine: &mut Engine, cypher: &str, params: HashMap<String, Value>) -> Vec<i64> {
    let mut v: Vec<i64> = engine
        .execute_cypher_with_params(cypher, params)
        .unwrap_or_else(|e| panic!("query `{cypher}` failed: {e}"))
        .rows
        .iter()
        .map(|r| r.values[0].as_i64().expect("integer column"))
        .collect();
    v.sort_unstable();
    v
}

// ───────────────────────────── §1 `IN` ─────────────────────────────

#[test]
#[serial_test::serial]
fn in_predicate_on_indexed_property_lifts_to_index_seek() {
    let (mut engine, _ctx) = seeded();
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.age IN [10, 30] RETURN n.age")
        .expect("plan");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexInSeek { .. })),
        "`IN` on an indexed property must lift to a NodeIndexInSeek; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "the seek must replace the label scan, not accompany it; plan = {plan:?}"
    );
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN [10, 30] RETURN n.age"
        ),
        vec![10, 30]
    );
}

#[test]
#[serial_test::serial]
fn in_seek_result_parity_across_list_shapes() {
    let (mut engine, _ctx) = seeded();
    // Value absent from the graph contributes nothing.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN [10, 99] RETURN n.age"
        ),
        vec![10]
    );
    // `x IN []` is false for every row.
    assert_eq!(
        ages(&mut engine, "MATCH (n:P) WHERE n.age IN [] RETURN n.age"),
        Vec::<i64>::new()
    );
    // A NULL element can never make the comparison true — `10` still matches.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN [10, null] RETURN n.age"
        ),
        vec![10]
    );
    // `10.0 = 10` in Cypher, so a float literal must find the int-stored node.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN [10.0] RETURN n.age"
        ),
        vec![10]
    );
    // The lifted seek is one conjunct — the rest still filters.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN [10, 20, 30] AND n.name = 'beta' RETURN n.age"
        ),
        vec![30]
    );
}

#[test]
#[serial_test::serial]
fn in_predicate_without_index_or_with_a_param_list_stays_a_scan() {
    let (mut engine, _ctx) = seeded();
    // `:P(other)` has no index — no seek.
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.other IN [1, 2] RETURN n.age")
        .expect("plan");
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeIndexInSeek { .. })),
        "unindexed property must not seek; plan = {plan:?}"
    );
    // A `$parameter` list has no plan-time elements: lifting it would under-
    // approximate, so the planner keeps the scan (and the result stays right).
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.age IN $ages RETURN n.age")
        .expect("plan");
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeIndexInSeek { .. })),
        "a parameter list must not lift to a seek; plan = {plan:?}"
    );
    let mut params = HashMap::new();
    params.insert("ages".to_string(), json!([10, 40]));
    assert_eq!(
        ages_with_params(
            &mut engine,
            "MATCH (n:P) WHERE n.age IN $ages RETURN n.age",
            params
        ),
        vec![10, 40]
    );
}

#[test]
#[serial_test::serial]
fn in_seek_on_an_indexed_property_emits_no_unindexed_notification() {
    let (mut engine, _ctx) = seeded();
    let result = engine
        .execute_cypher("MATCH (n:P) WHERE n.age IN [10, 30] RETURN n.age")
        .expect("query");
    assert!(
        !result
            .notifications
            .iter()
            .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
        "a seeking predicate must not claim a full scan; notifications = {:?}",
        result.notifications
    );
}

// ───────────────────────── §2 `STARTS WITH` ────────────────────────

#[test]
#[serial_test::serial]
fn starts_with_predicate_on_indexed_property_lifts_to_prefix_seek() {
    let (mut engine, _ctx) = seeded();
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.name STARTS WITH 'alp' RETURN n.age")
        .expect("plan");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexPrefixSeek { .. })),
        "`STARTS WITH` on an indexed property must lift to a NodeIndexPrefixSeek; \
         plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "the seek must replace the label scan, not accompany it; plan = {plan:?}"
    );
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH 'alp' RETURN n.age"
        ),
        vec![10, 20]
    );
}

#[test]
#[serial_test::serial]
fn prefix_seek_result_parity_across_prefix_shapes() {
    let (mut engine, _ctx) = seeded();
    // Exact-value prefix: only the node whose name IS the prefix.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH 'beta' RETURN n.age"
        ),
        vec![30]
    );
    // Every string starts with the empty prefix.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH '' RETURN n.age"
        ),
        vec![10, 20, 30, 40]
    );
    // No match at all.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH 'zzz' RETURN n.age"
        ),
        Vec::<i64>::new()
    );
    // Prefix boundary: `alpha` must not drag in `alpine`.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH 'alpha' RETURN n.age"
        ),
        vec![10]
    );
    // The lifted seek is one conjunct — the rest still filters.
    assert_eq!(
        ages(
            &mut engine,
            "MATCH (n:P) WHERE n.name STARTS WITH 'alp' AND n.age > 15 RETURN n.age"
        ),
        vec![20]
    );
}

#[test]
#[serial_test::serial]
fn contains_predicate_still_scans_and_still_notifies() {
    let (mut engine, _ctx) = seeded();
    // CONTAINS is an unanchored substring match — no ordered index can seek
    // it, so it must keep both the full scan and the notification.
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.name CONTAINS 'lp' RETURN n.age")
        .expect("plan");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "CONTAINS must stay a full scan; plan = {plan:?}"
    );
    let result = engine
        .execute_cypher("MATCH (n:P) WHERE n.name CONTAINS 'lp' RETURN n.age")
        .expect("query");
    assert!(
        result
            .notifications
            .iter()
            .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
        "CONTAINS still full-scans an indexed property — the notification must \
         still fire; notifications = {:?}",
        result.notifications
    );
}

// ───────────────────────── §3 `$parameter` ─────────────────────────

#[test]
#[serial_test::serial]
fn parameter_equality_on_indexed_property_lifts_to_param_seek() {
    let (mut engine, _ctx) = seeded();
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.age = $age RETURN n.age")
        .expect("plan");
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeIndexParamSeek { .. })),
        "`= $param` on an indexed property must lift to a NodeIndexParamSeek; \
         plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "the seek must replace the label scan, not accompany it; plan = {plan:?}"
    );
    // The predicate stays as a residual Filter — that is what makes the
    // operator's list/map/missing-parameter fallback to a scan correct.
    assert!(
        plan.iter().any(|op| matches!(op, Operator::Filter { .. })),
        "the parameter predicate must be retained as a residual Filter; \
         plan = {plan:?}"
    );
    let mut params = HashMap::new();
    params.insert("age".to_string(), json!(30));
    assert_eq!(
        ages_with_params(
            &mut engine,
            "MATCH (n:P) WHERE n.age = $age RETURN n.age",
            params
        ),
        vec![30]
    );
}

#[test]
#[serial_test::serial]
fn param_seek_result_parity_across_bound_values() {
    let (mut engine, _ctx) = seeded();
    let q = "MATCH (n:P) WHERE n.age = $age RETURN n.age";

    // A value no node carries.
    let mut params = HashMap::new();
    params.insert("age".to_string(), json!(99));
    assert_eq!(ages_with_params(&mut engine, q, params), Vec::<i64>::new());

    // `n.prop = null` is null for every node — never a match.
    let mut params = HashMap::new();
    params.insert("age".to_string(), Value::Null);
    assert_eq!(ages_with_params(&mut engine, q, params), Vec::<i64>::new());

    // A list parameter: the index cannot key it, so the operator falls back
    // to a scan and the retained Filter decides. No node has a list age.
    let mut params = HashMap::new();
    params.insert("age".to_string(), json!([10, 20]));
    assert_eq!(ages_with_params(&mut engine, q, params), Vec::<i64>::new());

    // `40 = 40.0` in Cypher — a float parameter must find the int-stored node.
    let mut params = HashMap::new();
    params.insert("age".to_string(), json!(40.0));
    assert_eq!(ages_with_params(&mut engine, q, params), vec![40]);

    // String parameter on the string-indexed property, mirrored operand order.
    let mut params = HashMap::new();
    params.insert("name".to_string(), json!("beta"));
    assert_eq!(
        ages_with_params(
            &mut engine,
            "MATCH (n:P) WHERE $name = n.name RETURN n.age",
            params
        ),
        vec![30]
    );

    // Parameter combined with a literal conjunct: the literal seek wins the
    // lift, the parameter predicate stays a filter — result still exact.
    let mut params = HashMap::new();
    params.insert("age".to_string(), json!(20));
    assert_eq!(
        ages_with_params(
            &mut engine,
            "MATCH (n:P) WHERE n.name = 'alpine' AND n.age = $age RETURN n.age",
            params
        ),
        vec![20]
    );
}

#[test]
#[serial_test::serial]
fn param_equality_without_an_index_stays_a_scan() {
    let (engine, _ctx) = seeded();
    let plan = engine
        .executor
        .parse_and_plan("MATCH (n:P) WHERE n.other = $other RETURN n.age")
        .expect("plan");
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeIndexParamSeek { .. })),
        "unindexed property must not seek; plan = {plan:?}"
    );
    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "unindexed property must keep the label scan; plan = {plan:?}"
    );
}
