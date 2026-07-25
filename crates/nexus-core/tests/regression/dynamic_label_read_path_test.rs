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
//! runs, with a non-empty STRING becoming the label and anything else
//! (missing / NULL / empty / non-STRING) collapsing the predicate to "no
//! rows" — never an error. The fix makes the MATCH scan path mirror this:
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

/// Mirrors the `WHERE n:$x` collapse semantics from
/// `crates/nexus-core/src/engine/tests/query.rs`
/// (`where_label_predicate_accepts_static_and_dynamic_label_forms`):
/// missing / null / empty-string / non-string parameter bindings must all
/// collapse the predicate to "no rows" — never an error, never "all
/// nodes".
#[test]
fn dynamic_label_match_missing_or_invalid_param_collapses_to_no_rows() {
    let (mut engine, _ctx) = engine();
    seed_foo_bar(&mut engine);

    // Missing param binding entirely (no `label` key in the params map).
    let r = engine
        .execute_cypher("MATCH (n:$label) RETURN n.n AS n")
        .expect("MATCH (n:$label) with no bound params must parse and execute");
    assert!(
        r.rows.is_empty(),
        "missing $label binding must collapse to no rows, got {:?}",
        r.rows
    );

    // Explicit NULL binding.
    let r = query_label_param(&mut engine, serde_json::Value::Null);
    assert!(
        r.rows.is_empty(),
        "null $label binding must collapse to no rows, got {:?}",
        r.rows
    );

    // Empty string binding.
    let r = query_label_param(&mut engine, serde_json::json!(""));
    assert!(
        r.rows.is_empty(),
        "empty-string $label binding must collapse to no rows, got {:?}",
        r.rows
    );

    // Non-string (integer) binding.
    let r = query_label_param(&mut engine, serde_json::json!(42));
    assert!(
        r.rows.is_empty(),
        "non-string $label binding must collapse to no rows, got {:?}",
        r.rows
    );
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
