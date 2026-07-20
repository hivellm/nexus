//! End-to-end coverage for the
//! `Nexus.Performance.CorrelatedPropertyPredicate` notification
//! (phase0_fix-correlated-predicate-index-seek §1.3).
//!
//! Mirrors `unindexed_property_notification_e2e_test.rs`'s shape but
//! pins the DISTINCT notification that fires when a `(label, property)`
//! pair IS covered by an index yet the predicate's value is row-local
//! (correlated — evaluated per driving row, e.g. `r.s` from `UNWIND`)
//! rather than a plan-time constant. The constant form seeks the
//! existing index and must emit neither this code nor
//! `Nexus.Performance.UnindexedPropertyAccess`.
//!
//! §3.3 (phase0_fix-correlated-predicate-index-seek): the inline
//! property-map form `MATCH (a:P {id: r.s})` now plans a genuine
//! per-row `NodeIndexSeek`, so it must ALSO stay silent — see
//! `engine_omits_correlated_notification_for_unwind_inline_match_now_that_it_seeks`
//! below. The WHERE `=` form (`WHERE a.id = r.s`) does not yet seek per
//! row and must keep firing.

use nexus_core::Engine;
use nexus_core::testing::TestContext;
use std::collections::HashMap;

const CORRELATED_CODE: &str = "Nexus.Performance.CorrelatedPropertyPredicate";
const UNINDEXED_CODE: &str = "Nexus.Performance.UnindexedPropertyAccess";

#[test]
fn engine_omits_correlated_notification_for_unwind_inline_match_now_that_it_seeks() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    // Seed the catalog so `id` on `:P` is interned, then create the
    // covering index. §3.3: the inline property-map form
    // `MATCH (a:P {id: r.s})` now plans a genuine per-row `NodeIndexSeek`
    // (§2/§3), so it is no different from the constant-value case once
    // an index exists — it must NOT surface CorrelatedPropertyPredicate
    // (nor UnindexedPropertyAccess) any more. That notification remains
    // reserved for the WHERE `=` form below, which is not yet
    // per-row-seek-backed.
    engine.execute_cypher("CREATE (s:P {id: 0})").expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let mut params = HashMap::new();
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 1 }, { "s": 2 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P {id: r.s}) RETURN a.id",
            params,
        )
        .expect("correlated inline match");

    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != CORRELATED_CODE),
        "UNWIND … MATCH (a:P {{id: r.s}}) with an index on :P(id) now plans a \
         per-row NodeIndexSeek — CorrelatedPropertyPredicate must not fire; \
         got {:?}",
        result.notifications
    );
    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != UNINDEXED_CODE),
        "the index exists and is used, so UnindexedPropertyAccess must not \
         fire either: {:?}",
        result.notifications
    );
}

#[test]
fn engine_omits_notification_for_constant_match_when_index_exists() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine.execute_cypher("CREATE (s:P {id: 0})").expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let result = engine
        .execute_cypher("MATCH (a:P {id: 42}) RETURN a.id")
        .expect("constant match");

    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != CORRELATED_CODE && n.code != UNINDEXED_CODE),
        "a constant predicate seeks the existing index — neither \
         CorrelatedPropertyPredicate nor UnindexedPropertyAccess should fire; \
         got {:?}",
        result.notifications
    );
}

#[test]
fn engine_emits_correlated_predicate_notification_for_unwind_where_match_with_index() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine.execute_cypher("CREATE (s:P {id: 0})").expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    let mut params = HashMap::new();
    params.insert(
        "rows".to_string(),
        serde_json::json!([{ "s": 1 }, { "s": 2 }]),
    );

    let result = engine
        .execute_cypher_with_params(
            "UNWIND $rows AS r MATCH (a:P) WHERE a.id = r.s RETURN a.id",
            params,
        )
        .expect("correlated where match");

    let correlated: Vec<_> = result
        .notifications
        .iter()
        .filter(|n| n.code == CORRELATED_CODE)
        .collect();
    assert_eq!(
        correlated.len(),
        1,
        "UNWIND … MATCH (a:P) WHERE a.id = r.s with an index on :P(id) must \
         surface exactly one CorrelatedPropertyPredicate notification; got {:?}",
        result.notifications
    );
    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != UNINDEXED_CODE),
        "the index exists, so UnindexedPropertyAccess must not also fire: {:?}",
        result.notifications
    );
}

#[test]
fn engine_omits_notification_for_join_predicate_between_two_matched_nodes() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).expect("engine init");

    engine.execute_cypher("CREATE (s:P {id: 0})").expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:P) ON (n.id)")
        .expect("create index");

    // `a.id = b.id` compares two matched node variables — a join
    // predicate, not a per-driving-row value. `is_correlated_where_operand`
    // deliberately excludes this shape (unindexed.rs:212-218), so neither
    // notification should fire.
    let result = engine
        .execute_cypher("MATCH (a:P), (b:P) WHERE a.id = b.id RETURN a.id, b.id")
        .expect("join predicate match");

    assert!(
        result
            .notifications
            .iter()
            .all(|n| n.code != CORRELATED_CODE && n.code != UNINDEXED_CODE),
        "a.id = b.id is a join predicate between two matched nodes, not a \
         correlated per-row value — neither notification should fire; got {:?}",
        result.notifications
    );
}
