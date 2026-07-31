//! Regression coverage for standalone `CREATE`'s function-call property
//! values (`crates/nexus-core/src/executor/operators/create/properties.rs`,
//! `Executor::resolve_standalone_create_property`).
//!
//! Before this fix, `CREATE (:V {p: date({...})})` — no preceding
//! `MATCH`/`UNWIND` row — rejected any function-call property value with
//! "Complex expressions not supported in CREATE properties"
//! (`expression_to_json_value` had no `FunctionCall` arm). The row-aware
//! path (`resolve_property_expr_for_create`, used when a row context
//! exists, e.g. `UNWIND ... CREATE`) already evaluated function calls and
//! canonicalized temporals before storage; standalone `CREATE` now routes
//! through the same evaluator + canonicalization, while a bare
//! `Variable`/`PropertyAccess` reference — structurally unresolvable with
//! no row at all in a standalone CREATE — still errors (TCK
//! `Create1[20]`).

use nexus_core::testing::setup_isolated_test_engine;
use nexus_core::{Engine, executor::ResultSet};

fn execute_query(engine: &mut Engine, query: &str) -> ResultSet {
    engine.execute_cypher(query).expect("Query should succeed")
}

fn get_single_value(result: &ResultSet) -> &serde_json::Value {
    assert!(!result.rows.is_empty(), "Result has no rows!");
    assert!(
        !result.rows[0].values.is_empty(),
        "First row has no values!"
    );
    &result.rows[0].values[0]
}

/// The tag key a tagged intermediate temporal value carries before
/// canonicalization — must never appear anywhere in a serialized response
/// or in what actually reaches storage.
const TEMPORAL_TAG_KEY: &str = "_nexus_temporal_type";

fn assert_no_temporal_tag_leak(value: &serde_json::Value) {
    let serialized = serde_json::to_string(value).expect("value must serialize");
    assert!(
        !serialized.contains(TEMPORAL_TAG_KEY),
        "tagged temporal marker leaked into response: {serialized}"
    );
}

#[test]
fn standalone_create_date_function_call_property_persists_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpDateNode {d: date('2015-07-21')})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpDateNode) RETURN v.d AS d");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("2015-07-21"),
        "expected the stored property to read back as the canonical ISO date string, got: {value:?}"
    );
    assert_no_temporal_tag_leak(value);

    // The tag must not survive anywhere in the persisted property map either.
    let result = execute_query(&mut engine, "MATCH (v:CfcpDateNode) RETURN v AS v");
    let node = get_single_value(&result);
    assert_no_temporal_tag_leak(node);
}

#[test]
fn standalone_create_duration_function_call_property_persists_canonical_iso_string() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpDurationNode {d: duration({days: 1, hours: 2})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpDurationNode) RETURN v.d AS d");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_str(),
        Some("P1DT2H"),
        "expected the stored property to read back as the canonical ISO duration string, got: {value:?}"
    );
    assert_no_temporal_tag_leak(value);
}

#[test]
fn standalone_create_non_temporal_function_call_property_resolves() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpUpperNode {name: toUpper('nexus')})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (v:CfcpUpperNode) RETURN v.name AS name");
    let value = get_single_value(&result);
    assert_eq!(value.as_str(), Some("NEXUS"));
}

/// End-to-end regression for the semantic-validation binder-collection
/// fix: a list comprehension's own loop variable inside a standalone
/// CREATE property map must resolve and execute, not be rejected as an
/// undefined-variable reference (`collect_clause_binders`'s `Clause::
/// Create` arm has to walk property-map expressions, not just the
/// pattern's node/relationship variables).
#[test]
fn standalone_create_list_comprehension_property_value_executes() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    execute_query(
        &mut engine,
        "CREATE (:CfcpLcNode {v: [x IN [1, 2, 3] | x * 2]})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:CfcpLcNode) RETURN n.v AS v");
    let value = get_single_value(&result);
    assert_eq!(
        value.as_array().map(|a| a.as_slice()),
        Some(
            [
                serde_json::json!(2),
                serde_json::json!(4),
                serde_json::json!(6)
            ]
            .as_slice()
        ),
        "expected the list comprehension to evaluate and persist as [2, 4, 6], got: {value:?}"
    );
}

/// A property value referencing a variable with no earlier `MATCH`/`UNWIND`
/// to bind it must still be rejected outright — a standalone `CREATE` has
/// no row scope, so evaluating it against an empty row must never silently
/// degrade to `Null`. Mirrors TCK `Create1[20]`
/// (`CREATE (b {name: missing}) RETURN b`).
#[test]
fn standalone_create_unresolvable_variable_property_still_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();

    let res = engine.execute_cypher("CREATE (:CfcpMissingNode {p: missing})");
    assert!(
        res.is_err(),
        "a bare undefined-variable property value in a standalone CREATE must error, got: {res:?}"
    );
}

// ============================================================================
// Undefined-variable references NESTED inside a larger expression — the
// review-round follow-up to the BLOCKER above. A shape-based carve-out
// (reject only a bare top-level `Variable`/`PropertyAccess`) lets an
// undefined name hiding one level deeper silently resolve to `Null`
// instead of erroring; these assert it still errors regardless of how
// deeply the reference is nested. Covers both the standalone path
// (`semantic_validation::validate` + the resolver-level reference walk)
// and the row-aware path (`MATCH ... CREATE`, TCK `Create2[24]`-shaped).
// ============================================================================

#[test]
fn standalone_create_undefined_variable_nested_in_function_call_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpNestedNode {name: toUpper(missing)})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside a function-call argument must still error, got: {res:?}"
    );
}

#[test]
fn standalone_create_undefined_variable_nested_in_arithmetic_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpNestedNode {name: missing + 1})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside arithmetic must still error, got: {res:?}"
    );
}

#[test]
fn standalone_create_undefined_variable_nested_in_list_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpNestedNode {name: [missing]})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside a list literal must still error, got: {res:?}"
    );
}

#[test]
fn standalone_create_undefined_variable_nested_in_map_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpNestedNode {name: {k: missing}})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside a map literal must still error, got: {res:?}"
    );
}

#[test]
fn standalone_create_undefined_variable_nested_in_case_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine
        .execute_cypher("CREATE (:CfcpNestedNode {name: CASE WHEN true THEN missing ELSE 1 END})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside a CASE branch must still error, got: {res:?}"
    );
}

#[test]
fn standalone_create_undefined_variable_nested_in_coalesce_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpNestedNode {name: coalesce(missing, 1)})");
    assert!(
        res.is_err(),
        "an undefined variable nested inside coalesce(...) must still error, got: {res:?}"
    );
}

#[test]
fn match_create_relationship_property_undefined_variable_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res =
        engine.execute_cypher("MATCH (a) CREATE (a)-[:CfcpKnows {name: missing}]->(a) RETURN a");
    assert!(
        res.is_err(),
        "TCK Create2[24]: a row-aware relationship property referencing a name bound \
         nowhere in the query must error, got: {res:?}"
    );
}

// ============================================================================
// MAJOR1 — unknown function name. A typo'd/nonexistent function name must
// not silently persist `null`.
// ============================================================================

#[test]
fn standalone_create_unknown_function_call_property_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpBogusFnNode {d: bogusFn(1)})");
    assert!(
        res.is_err(),
        "an unrecognized function name in a CREATE property value must error, not silently \
         persist null, got: {res:?}"
    );
}

/// Round-2 follow-up: the strict check must fire on an unknown function
/// name reached through arithmetic, not just a top-level call.
#[test]
fn standalone_create_unknown_function_nested_in_arithmetic_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpBogusFnNode {d: 1 + bogusFn(1)})");
    assert!(
        res.is_err(),
        "an unrecognized function name nested inside arithmetic must error, not silently \
         persist null, got: {res:?}"
    );
}

/// Round-2 follow-up: same, nested inside a list literal.
#[test]
fn standalone_create_unknown_function_nested_in_list_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpBogusFnNode {d: [bogusFn(1)]})");
    assert!(
        res.is_err(),
        "an unrecognized function name nested inside a list literal must error, not silently \
         persist null, got: {res:?}"
    );
}

/// The unknown-function pre-pass must not false-positive on a nested but
/// genuinely KNOWN function call.
#[test]
fn standalone_create_known_function_nested_in_arithmetic_resolves() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(&mut engine, "CREATE (:CfcpAbsNode {d: 1 + abs(-1)})");
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:CfcpAbsNode) RETURN n.d AS d");
    let value = get_single_value(&result);
    // `abs()` returns a float, so `1 + abs(-1)` is `2.0`, not the integer `2`.
    assert_eq!(value.as_f64(), Some(2.0));
}

// ============================================================================
// MAJOR2 — aggregate functions have no grouping context inside a CREATE
// property map and must be rejected, not silently evaluate to `null`.
// ============================================================================

#[test]
fn standalone_create_aggregate_in_property_map_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpAggNode {c: count(*)})");
    assert!(
        res.is_err(),
        "an aggregate function in a CREATE property map must error, got: {res:?}"
    );
}

// ============================================================================
// MAJOR3 — a resolved property value must be a primitive scalar (or a flat
// array of primitives), never a whole node/relationship object or a
// literal map. `{peer: b}` (a bound node used as a property value) is a
// marker-collision hazard if persisted as-is: the node's own
// `_nexus_id`/`_nexus_labels` keys would land inside another entity's
// property store.
// ============================================================================

#[test]
fn match_create_relationship_property_storing_a_bound_node_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher("CREATE (:CfcpSrc)").unwrap();
    engine.refresh_executor().unwrap();

    let res = engine
        .execute_cypher("MATCH (a:CfcpSrc) CREATE (a)-[:CfcpRel {peer: a}]->(:CfcpDst) RETURN a");
    assert!(
        res.is_err(),
        "storing a whole bound node as a relationship property value must error \
         (TypeError: property values must be primitive), got: {res:?}"
    );
}

#[test]
fn match_create_node_property_storing_a_bound_node_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    engine.execute_cypher("CREATE (:CfcpSrc)").unwrap();
    engine.refresh_executor().unwrap();

    let res = engine.execute_cypher("MATCH (a:CfcpSrc) CREATE (:CfcpDst {peer: a}) RETURN a");
    assert!(
        res.is_err(),
        "storing a whole bound node as a node property value must error \
         (TypeError: property values must be primitive), got: {res:?}"
    );
}

#[test]
fn standalone_create_literal_map_property_value_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpMapNode {p: {a: 1}})");
    assert!(
        res.is_err(),
        "a literal map is not a legal property value (TypeError: property values must be \
         primitive or array thereof), got: {res:?}"
    );
}

/// MINOR1 (round-2 review): a map that merely carries a `crs` key must
/// not be mistaken for a real Point value and slip the primitive-value
/// guard — a bare `contains_key("crs")` discriminator let
/// `{crs: 'x', secret: 'leak'}` through and persist verbatim.
#[test]
fn standalone_create_map_forging_a_crs_key_still_errors() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    let res = engine.execute_cypher("CREATE (:CfcpFakePointNode {p: {crs: 'x', secret: 'leak'}})");
    assert!(
        res.is_err(),
        "a map carrying only a `crs` key (no numeric x/y, not a known CRS value) is not a \
         real Point and must not be accepted as a property value, got: {res:?}"
    );
}

/// A genuine Point value — the real shape `geospatial::Point::to_json_value`
/// produces — must still be accepted as a property value.
#[test]
fn standalone_create_real_point_property_value_resolves() {
    let (mut engine, _ctx) = setup_isolated_test_engine().unwrap();
    execute_query(
        &mut engine,
        "CREATE (:CfcpPointNode {p: point({x: 1.0, y: 2.0})})",
    );
    engine.refresh_executor().unwrap();

    let result = execute_query(&mut engine, "MATCH (n:CfcpPointNode) RETURN n.p AS p");
    let value = get_single_value(&result);
    assert_eq!(
        value.get("x").and_then(|v| v.as_f64()),
        Some(1.0),
        "expected the stored property to read back as a Point object, got: {value:?}"
    );
}
