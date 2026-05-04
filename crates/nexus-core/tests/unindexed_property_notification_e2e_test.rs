//! End-to-end coverage for the
//! `Nexus.Performance.UnindexedPropertyAccess` notification feature
//! (phase6_merge-unindexed-property-warning).
//!
//! The planner unit tests in
//! `crates/nexus-core/src/executor/planner/tests.rs` already pin the
//! emission contract at the planner boundary. This file drives the
//! same contract through the full `Engine::execute_cypher` path so a
//! regression in the thread-local notification sink, the
//! `Executor::execute` wrapper, or the `ResultSet.notifications`
//! plumbing surfaces here even when the planner-level tests still
//! pass.

use nexus_core::Engine;
use nexus_core::testing::TestContext;

const UNINDEXED_CODE: &str = "Nexus.Performance.UnindexedPropertyAccess";

#[test]
fn engine_surfaces_unindexed_notification_for_merge_through_executor_drain() {
    // Fresh data dir so no stale index/catalog state leaks across runs.
    let ctx = TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).expect("engine init");

    // Seed the catalog so the diagnostic pre-pass can resolve the
    // `(label_id, key_id)` pair. The MERGE itself does this implicitly
    // on first run, but doing it up-front via a no-op CREATE keeps the
    // assertion focused on the notification, not on schema bootstrap.
    engine
        .execute_cypher("CREATE (s:Artifact { natural_key: 'seed' })")
        .expect("seed");

    let result = engine
        .execute_cypher("MERGE (n:Artifact { natural_key: 'sha256:abc' }) RETURN count(n) AS c")
        .expect("merge");

    let unindexed: Vec<_> = result
        .notifications
        .iter()
        .filter(|n| n.code == UNINDEXED_CODE)
        .collect();
    assert_eq!(
        unindexed.len(),
        1,
        "MERGE on unindexed (Artifact, natural_key) must surface exactly one \
         UnindexedPropertyAccess notification through the engine envelope; \
         got {:?}",
        result.notifications
    );

    let n = unindexed[0];
    assert!(
        n.title.contains("Artifact") && n.title.contains("natural_key"),
        "title must name the (label, property) pair: {}",
        n.title
    );
    assert!(
        n.description
            .contains("CREATE INDEX FOR (n:Artifact) ON (n.natural_key)"),
        "description must include the suggested DDL verbatim: {}",
        n.description
    );
}

#[test]
fn engine_omits_notification_after_index_is_created() {
    let ctx = TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).expect("engine init");

    // Bootstrap label + key, then create the property index so the
    // planner's `has_index` check returns true on the next plan.
    engine
        .execute_cypher("CREATE (s:Artifact { natural_key: 'seed' })")
        .expect("seed");
    engine
        .execute_cypher("CREATE INDEX FOR (n:Artifact) ON (n.natural_key)")
        .expect("create index");

    let result = engine
        .execute_cypher("MERGE (n:Artifact { natural_key: 'sha256:def' }) RETURN count(n) AS c")
        .expect("merge");

    let unindexed: Vec<_> = result
        .notifications
        .iter()
        .filter(|n| n.code == UNINDEXED_CODE)
        .collect();
    assert!(
        unindexed.is_empty(),
        "with index registered, no UnindexedPropertyAccess notification \
         should appear in the response envelope; got {:?}",
        result.notifications
    );
}

#[test]
fn engine_does_not_leak_notifications_across_consecutive_queries() {
    // The thread-local sink is shared across calls on the same thread.
    // `Executor::execute` clears stale entries before planning. This
    // test pins that contract: a query that emits a notification must
    // not pollute a follow-up query that should produce none (because
    // the second query targets an already-indexed pair).
    let ctx = TestContext::new();
    let mut engine = Engine::with_data_dir(ctx.path()).expect("engine init");

    engine
        .execute_cypher("CREATE (s:Artifact { natural_key: 'seed' })")
        .expect("seed");

    // Call 1 — unindexed (Artifact, natural_key) → expects notification.
    let dirty = engine
        .execute_cypher("MERGE (n:Artifact { natural_key: 'x' }) RETURN n")
        .expect("first merge");
    assert!(
        dirty.notifications.iter().any(|n| n.code == UNINDEXED_CODE),
        "first MERGE should emit; got {:?}",
        dirty.notifications
    );

    // Now register the index — the next plan must not emit.
    engine
        .execute_cypher("CREATE INDEX FOR (n:Artifact) ON (n.natural_key)")
        .expect("create index");

    // Call 2 — indexed pair, expects empty notifications. If the
    // thread-local leaks, the prior `dirty.notifications` entry would
    // re-appear here.
    let clean = engine
        .execute_cypher("MERGE (n:Artifact { natural_key: 'y' }) RETURN n")
        .expect("second merge");
    assert!(
        clean.notifications.iter().all(|n| n.code != UNINDEXED_CODE),
        "second MERGE must not surface UnindexedPropertyAccess after the \
         index was created; got {:?}",
        clean.notifications
    );
}
