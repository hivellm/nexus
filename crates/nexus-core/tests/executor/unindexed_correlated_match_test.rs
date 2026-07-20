//! phase0_fix-unindexed-correlated-match-drops-rows
//!
//! `UNWIND $rows AS r MATCH (a:P {id: r.s})` over an UNINDEXED `:P(id)` must
//! return every match for every driving row — the same rows the indexed
//! `NodeIndexSeek` path returns (`phase0_fix-correlated-predicate-index-seek`),
//! just via a label scan + residual filter.
//!
//! Root cause of the bug these tests pin: the pre-filter row dedup
//! (`compute_row_dedup_key` in `operators/filter.rs`) keyed every object
//! WITHOUT a `_nexus_id` (i.e. every `UNWIND` row-map like `{s: 10}`) to the
//! constant `"obj:no_id"`, so driving rows sharing the same scanned node `a`
//! collided and all but the first (`r = {s: 10}`) were dropped before the
//! filter ran — leaving only the first driving row's matches.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

/// DISCRIMINATING: nodes {10, 20, 30, 20} (id 20 twice), no index. Driving
/// [10, 20, 99, 30]: 10→1, 20→2, 99→0, 30→1 ⇒ `[10, 20, 20, 30]`. Before the
/// fix this returned only `[10]` (the first driving row's matches).
#[test]
fn unindexed_correlated_match_returns_all_matches_for_every_driving_row() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 10}), (:P {id: 20}), (:P {id: 30}), (:P {id: 20})")
        .expect("seed nodes");
    // Deliberately NO `CREATE INDEX` — this exercises the label-scan + filter
    // path, not the correlated NodeIndexSeek.

    let mut params = HashMap::new();
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 10 }, { "s": 20 }, { "s": 99 }, { "s": 30 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("unindexed correlated match must succeed");

    let mut ids: Vec<i64> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();
    ids.sort_unstable();

    assert_eq!(
        ids,
        vec![10, 20, 20, 30],
        "unindexed correlated match must return every match for every driving \
         row (id 20 twice), omitting only the miss (99); got {ids:?}"
    );
}

/// CHARACTERISATION (§1.3): a driving MISS in the FIRST position must not drop
/// the later hits. Nodes {10, 20}, no index, driving [99, 10] → `[10]`. Before
/// the fix the leading `99` row's dedup keys shadowed every later driving row,
/// and the filter (`a.id = 99`) then matched nothing → `[]`.
#[test]
fn unindexed_correlated_match_survives_a_leading_miss() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (:P {id: 10}), (:P {id: 20})")
        .expect("seed nodes");

    let mut params = HashMap::new();
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 99 }, { "s": 10 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("unindexed correlated match must succeed");

    let ids: Vec<i64> = result
        .rows
        .iter()
        .map(|row| row.values[0].as_i64().expect("a.id must be an integer"))
        .collect();

    assert_eq!(
        ids,
        vec![10],
        "a leading non-matching driving row must not shadow later matching rows; \
         got {ids:?}"
    );
}
