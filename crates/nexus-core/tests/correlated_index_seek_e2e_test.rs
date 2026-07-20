//! End-to-end coverage for the correlated `NodeIndexSeek` execution path
//! (phase0_fix-correlated-predicate-index-seek §3.4 / §4.2).
//!
//! `UNWIND $rows AS r MATCH (a:P {id: r.s})`, with an index on `:P(id)`,
//! now plans a per-driving-row `Operator::NodeIndexSeek` whose
//! `key_expression` is `Some(...)` (`execute_correlated_index_seek`,
//! `crates/nexus-core/src/executor/operators/scan.rs`) instead of scanning
//! the whole label and cross-joining. These tests pin:
//!
//! - correctness of the per-row seek (happy path, no-match, multi-match)
//! - "same results, different plan" versus the constant seek form
//! - the plan shape itself (`NodeIndexSeek`, never `NodeByLabel`), as a
//!   guard that fails if the plan ever degrades back to a label scan

use nexus_core::Engine;
use nexus_core::executor::types::Operator;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

/// CORRECTNESS-1: seeds several `:P {id}` nodes, indexes `:P(id)`, and
/// drives the correlated seek with `$rows` covering a subset of the
/// existing ids. The returned `a.id` values must be exactly the matched
/// nodes' ids, one row per driving row that matched — in driving-row order.
#[test]
fn correlated_seek_returns_matched_node_ids_for_each_driving_row() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 10}), (:P {id: 20}), (:P {id: 30}), (:P {id: 40})")
        .expect("seed nodes");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let mut params = HashMap::new();
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 10 }, { "s": 20 }, { "s": 40 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("correlated seek must succeed");

    eprintln!("DEBUG result = {result:#?}");

    let ids: Vec<i64> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();

    assert_eq!(
        ids,
        vec![10, 20, 40],
        "expected exactly the ids matched by each driving row, in order; got {ids:?}"
    );
}

/// §3.4: a driving row whose key matches NO `:P` node yields no output row
/// for that driving row — the query must not error and must not drop the
/// OTHER driving rows that do match.
#[test]
fn correlated_seek_omits_only_the_nonmatching_driving_row() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 10}), (:P {id: 30})")
        .expect("seed nodes");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let mut params = HashMap::new();
    // 20 and 99 match nothing; 10 and 30 do.
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 10 }, { "s": 20 }, { "s": 30 }, { "s": 99 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("correlated seek with a non-matching driving row must not error");

    let ids: Vec<i64> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();

    assert_eq!(
        ids,
        vec![10, 30],
        "non-matching driving rows must be omitted, matching ones kept; got {ids:?}"
    );
}

/// §3.4: when two `:P` nodes share the same `id` value (no uniqueness
/// constraint), a driving row at that id must be duplicated once per
/// matching node — the join semantics of an index seek, not a lookup
/// capped at one result.
#[test]
fn correlated_seek_duplicates_driving_row_for_each_matching_node() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 5}), (:P {id: 5})")
        .expect("seed two nodes with the same id");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let mut params = HashMap::new();
    params.insert("rows".to_string(), serde_json::json!([{ "s": 5 }]));

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("correlated seek must succeed");

    assert_eq!(
        result.rows.len(),
        2,
        "a driving row matching 2 nodes must produce 2 output rows, one per \
         match; got {} rows",
        result.rows.len()
    );
    for row in &result.rows {
        assert_eq!(
            row.values[0].as_i64(),
            Some(5),
            "both duplicated rows must carry the matched node's id"
        );
    }
}

/// §3.4: the correlated per-row seek must yield the SAME result as the
/// constant seek for the equivalent value — "same results, different
/// plan". Pins that the new execution path does not change semantics.
#[test]
fn correlated_seek_matches_constant_seek_for_equivalent_value() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 7}), (:P {id: 8})")
        .expect("seed nodes");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let correlated = engine
        .execute_cypher("UNWIND [{s: 7}] AS r MATCH (a:P {id: r.s}) RETURN a.id")
        .expect("correlated seek must succeed");
    let constant = engine
        .execute_cypher("MATCH (a:P {id: 7}) RETURN a.id")
        .expect("constant seek must succeed");

    let correlated_ids: Vec<i64> = correlated
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();
    let constant_ids: Vec<i64> = constant
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();

    assert_eq!(
        correlated_ids, constant_ids,
        "correlated seek `r.s` bound to 7 must return the same rows as the \
         constant seek `{{id: 7}}`; correlated={correlated_ids:?}, \
         constant={constant_ids:?}"
    );
}

/// §4.2 / §3.2 PLAN GUARD: with an index on `:P(id)`, the correlated form
/// must plan a `NodeIndexSeek` with a per-row `key_expression`, never a
/// `NodeByLabel` full scan. This is the guard that fails if the plan ever
/// degrades back to a label scan + cross product.
#[test]
fn correlated_match_plans_node_index_seek_with_key_expression_when_indexed() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine.execute_cypher("CREATE (:P {id: 1})").expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let plan = engine
        .executor
        .parse_and_plan("UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id")
        .expect("plan must succeed");

    assert!(
        plan.iter().any(|op| matches!(
            op,
            Operator::NodeIndexSeek {
                key_expression: Some(_),
                ..
            }
        )),
        "indexed correlated selector must plan a NodeIndexSeek with a \
         per-row key_expression; plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "indexed correlated selector must NOT fall back to a NodeByLabel \
         scan; plan = {plan:?}"
    );
}

/// §4.2 mirror guard: WITHOUT an index on `:P(id)`, the same correlated
/// query must NOT plan a `NodeIndexSeek` — it falls back to `NodeByLabel`.
/// Proves the guard above is meaningful (it can actually fail) rather than
/// vacuously true because `NodeIndexSeek` is always planned regardless of
/// index presence.
#[test]
fn correlated_match_plans_node_by_label_without_index() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine.execute_cypher("CREATE (:P {id: 1})").expect("seed");
    // Deliberately no `CREATE INDEX` — :P(id) is unindexed.

    let plan = engine
        .executor
        .parse_and_plan("UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id")
        .expect("plan must succeed");

    assert!(
        plan.iter()
            .any(|op| matches!(op, Operator::NodeByLabel { .. })),
        "unindexed correlated selector must plan a NodeByLabel scan; \
         plan = {plan:?}"
    );
    assert!(
        !plan
            .iter()
            .any(|op| matches!(op, Operator::NodeIndexSeek { .. })),
        "unindexed correlated selector must NOT plan a NodeIndexSeek; \
         plan = {plan:?}"
    );
}
