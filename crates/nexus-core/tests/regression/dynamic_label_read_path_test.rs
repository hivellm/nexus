//! `MATCH (n:$label)` — a dynamic node label supplied via a query parameter
//! — silently returned ZERO rows instead of the nodes carrying the
//! resolved label.
//!
//! Root cause: the node pattern parser stores `$label` verbatim as a
//! sentinel string in the node pattern's `labels` list, but the planner's
//! `plan_execution_strategy` (`crates/nexus-core/src/executor/planner/
//! queries/strategy.rs`) resolved the FIRST label by calling
//! `self.catalog.get_or_create_label("$label")` at PLAN time — before any
//! query parameters are available (they live on the execution context, not
//! on the `QueryPlanner`). That call matched (and even CREATED) a literal
//! label named `$label`, which almost never has any nodes, so the scan
//! came back empty.
//!
//! The WHERE-clause form `WHERE n:$x` already resolved this correctly at
//! EXECUTION time (`executor/operators/filter.rs`): a `$name` label-check
//! predicate is resolved against `context.params` when the Filter operator
//! runs. A non-empty STRING becomes the label, a LIST<STRING> a label
//! intersection (§4.4), and NULL/missing/empty/non-STRING raises a typed
//! `ERR_INVALID_LABEL` (§4.4 uniformised labels with relationship types —
//! see `dynamic_rel_type_read_path_test`). The fix makes the MATCH scan
//! path mirror this:
//! when the first label starts with `$`, the planner emits an
//! `AllNodesScan` + a label-check `Filter("{var}:{first_label}")` instead
//! of resolving via the catalog, deferring resolution to `filter.rs` at
//! execution time.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

fn engine() -> (Engine, TestContext) {
    let ctx = TestContext::new();
    let engine = Engine::with_isolated_catalog(ctx.path()).expect("engine");
    (engine, ctx)
}

fn seed_foo_bar(engine: &mut Engine) {
    engine
        .execute_cypher("CREATE (:Foo {n: 1}), (:Bar {n: 2})")
        .expect("seed Foo/Bar");
}

fn query_label_param(
    engine: &mut Engine,
    label: serde_json::Value,
) -> nexus_core::executor::ResultSet {
    let mut params: HashMap<String, serde_json::Value> = HashMap::new();
    params.insert("label".to_string(), label);
    engine
        .execute_cypher_with_params("MATCH (n:$label) RETURN n.n AS n", params)
        .expect("MATCH (n:$label) must parse and execute, not error")
}

/// Positive discriminator (the core regression): `MATCH (n:$label)` with
/// `label = 'Foo'` must return exactly the Foo node, `[1]`. PRE-FIX this
/// query returns EMPTY because the scan resolves against a literal
/// `$label` label (via `get_or_create_label("$label")`) instead of the
/// bound parameter — that emptiness is exactly what makes this test
/// discriminating between the buggy and fixed planner.
#[test]
fn dynamic_label_match_resolves_param_to_matching_nodes() {
    let (mut engine, _ctx) = engine();
    seed_foo_bar(&mut engine);

    let r = query_label_param(&mut engine, serde_json::json!("Foo"));
    assert_eq!(
        r.rows.len(),
        1,
        "MATCH (n:$label) with label='Foo' must return exactly 1 row (the Foo node), got {:?}",
        r.rows
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!(1));

    let r = query_label_param(&mut engine, serde_json::json!("Bar"));
    assert_eq!(
        r.rows.len(),
        1,
        "MATCH (n:$label) with label='Bar' must return exactly 1 row (the Bar node), got {:?}",
        r.rows
    );
    assert_eq!(r.rows[0].values[0], serde_json::json!(2));
}

/// A label value that does not exist in the catalog must yield zero rows,
/// not an error and not a spuriously-created label.
#[test]
fn dynamic_label_match_nonexistent_label_returns_no_rows() {
    let (mut engine, _ctx) = engine();
    seed_foo_bar(&mut engine);

    let r = query_label_param(&mut engine, serde_json::json!("Nope"));
    assert!(
        r.rows.is_empty(),
        "MATCH (n:$label) with label='Nope' must return no rows, got {:?}",
        r.rows
    );
}

/// §4.4 uniformised the degenerate-parameter policy for labels with the
/// one already chosen for relationship types: missing / null / empty-string
/// / non-string bindings raise a typed `ERR_INVALID_LABEL` rather than
/// silently collapsing to "no rows". (A valid-but-unregistered label name
/// like `'Nope'` is NOT degenerate — it stays a plain empty result, covered
/// by `dynamic_label_match_nonexistent_label_returns_no_rows`.)
#[test]
fn dynamic_label_match_invalid_param_raises_typed_error() {
    let (mut engine, _ctx) = engine();
    seed_foo_bar(&mut engine);

    // Missing param binding entirely (no `label` key in the params map).
    let res = engine.execute_cypher("MATCH (n:$label) RETURN n.n AS n");
    assert!(
        res.is_err_and(|e| e.to_string().contains("ERR_INVALID_LABEL")),
        "missing $label binding must raise ERR_INVALID_LABEL"
    );

    for bad in [
        serde_json::Value::Null,
        serde_json::json!(""),
        serde_json::json!(42),
        serde_json::json!([]),
        serde_json::json!(["Foo", 42]),
    ] {
        let mut params: HashMap<String, serde_json::Value> = HashMap::new();
        params.insert("label".to_string(), bad.clone());
        let res = engine.execute_cypher_with_params("MATCH (n:$label) RETURN n.n AS n", params);
        match res {
            Err(e) => assert!(
                e.to_string().contains("ERR_INVALID_LABEL"),
                "expected ERR_INVALID_LABEL for {bad:?}, got {e}"
            ),
            Ok(rs) => panic!(
                "expected an error for invalid $label {bad:?}, got {} rows",
                rs.rows.len()
            ),
        }
    }
}

/// §4.4 — a LIST parameter is a label INTERSECTION: `MATCH (n:$labels)` with
/// `labels = ['A', 'B']` matches only nodes carrying BOTH labels (like
/// `(n:A:B)`), while `['A']` matches every `A` node.
#[test]
fn dynamic_label_match_list_param_is_a_label_intersection() {
    let (mut engine, _ctx) = engine();
    engine
        .execute_cypher("CREATE (:A:B {n: 10}), (:A {n: 20})")
        .expect("seed multi-label graph");

    let query = |engine: &mut Engine, labels: serde_json::Value| -> Vec<i64> {
        let mut params: HashMap<String, serde_json::Value> = HashMap::new();
        params.insert("labels".to_string(), labels);
        let rs = engine
            .execute_cypher_with_params("MATCH (n:$labels) RETURN n.n AS n", params)
            .expect("query");
        let mut ids: Vec<i64> = rs
            .rows
            .iter()
            .filter_map(|r| r.values.first().and_then(serde_json::Value::as_i64))
            .collect();
        ids.sort_unstable();
        ids
    };

    // Both labels required → only the (:A:B) node (n = 10).
    assert_eq!(query(&mut engine, serde_json::json!(["A", "B"])), vec![10]);
    // Single label → every A node (n = 10 and n = 20).
    assert_eq!(query(&mut engine, serde_json::json!(["A"])), vec![10, 20]);
}

/// Confusion guard: the `$label` sentinel must never be treated as (or
/// resolved against) a literal label named `$label`. With `label='Foo'`
/// bound, the result must be exactly the Foo node — not every node, and
/// not a node that happens to carry a literal `$label` label.
///
/// There is no test-accessible way to construct a node whose LITERAL
/// label is the string `$label`: `Engine` exposes no direct catalog
/// accessor, and the Cypher write path's dynamic-label handling
/// (`resolve_dynamic_labels` in `crates/nexus-core/src/engine/
/// match_exec.rs`) resolves `CREATE (:$label)` against the bound
/// parameter too, so it can never produce a literally-`$label`-labelled
/// node either. Cases 1-3 above (exact-match, non-existent label, and the
/// null/empty/non-string collapse) are therefore the discriminator for
/// this confusion: pre-fix, `get_or_create_label("$label")` is called
/// directly on the planner's catalog handle, which would `assert!` here
/// as "returns everything with the literal `$label` label" (in practice,
/// nothing, since no such label is ever created another way) rather than
/// resolving the bound parameter.
#[test]
fn dynamic_label_sentinel_is_never_treated_as_a_literal_label() {
    let (mut engine, _ctx) = engine();
    seed_foo_bar(&mut engine);

    let r = query_label_param(&mut engine, serde_json::json!("Foo"));
    assert_eq!(
        r.rows.len(),
        1,
        "MATCH (n:$label) with label='Foo' must return exactly the Foo node, not every node, got {:?}",
        r.rows
    );
    assert_eq!(
        r.rows[0].values[0],
        serde_json::json!(1),
        "the single returned row must be the Foo node (n.n == 1), not the Bar node"
    );
}
