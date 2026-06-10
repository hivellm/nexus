//! Tests for transaction correctness and restart durability: UNWIND writes,
//! property index persistence across restart, CALL IN TRANSACTIONS termination,
//! explicit BEGIN/COMMIT index maintenance, relationship index self-heal, and
//! UNWIND+MATCH+MERGE edge upsert.

use super::*;

/// ISSUE #13: a write that ranges over an UNWIND row list must persist every
/// row (MERGE + SET), and `RETURN count(n)` must reflect the rows written.
/// Previously the write path errored/dropped UNWIND and returned count 0.
#[test]
#[serial_test::serial]
fn unwind_write_merge_persists_each_row() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let r = engine
        .execute_cypher(
            "UNWIND [{id:'unw1',nm:'A'},{id:'unw2',nm:'B'}] AS row \
             MERGE (n:ZZUnw {id: row.id}) SET n.name = row.nm RETURN count(n) AS c",
        )
        .expect("UNWIND write must succeed");
    assert_eq!(r.rows.len(), 1, "count query returns one row");
    assert_eq!(
        r.rows[0].values[0].as_i64(),
        Some(2),
        "count(n) must be 2, got {:?}",
        r.rows[0].values[0]
    );

    // Data is actually persisted and readable, and each row's SET applied to
    // ITS OWN node (not the whole accumulated batch).
    let read = engine
        .execute_cypher("MATCH (n:ZZUnw) RETURN n.id, n.name")
        .expect("read must succeed");
    assert_eq!(read.rows.len(), 2, "two :ZZUnw nodes must persist");
    let mut by_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for row in &read.rows {
        by_id.insert(
            row.values[0].as_str().unwrap_or_default().to_string(),
            row.values[1].as_str().unwrap_or_default().to_string(),
        );
    }
    assert_eq!(
        by_id.get("unw1").map(String::as_str),
        Some("A"),
        "per-row SET: unw1.name must be 'A', got {by_id:?}"
    );
    assert_eq!(
        by_id.get("unw2").map(String::as_str),
        Some("B"),
        "per-row SET: unw2.name must be 'B', got {by_id:?}"
    );

    // MERGE is idempotent across UNWIND rows: re-running does not duplicate.
    engine
        .execute_cypher(
            "UNWIND [{id:'unw1',nm:'A2'}] AS row \
             MERGE (n:ZZUnw {id: row.id}) SET n.name = row.nm RETURN count(n) AS c",
        )
        .expect("second UNWIND write must succeed");
    let read2 = engine
        .execute_cypher("MATCH (n:ZZUnw) RETURN n.id")
        .expect("read must succeed");
    assert_eq!(
        read2.rows.len(),
        2,
        "MERGE over UNWIND must stay idempotent (no duplicate for unw1)"
    );
}

/// ISSUE #11: property indexes must survive a restart. After reopening the
/// engine on the same data dir, the typed index is rebuilt + backfilled from
/// the persisted definition, the read seek engages (no UnindexedPropertyAccess),
/// and a duplicate `CREATE INDEX` errors (catalog existence restored).
#[test]
#[serial_test::serial]
fn property_index_survives_restart() {
    let ctx = crate::testing::TestContext::new();
    let path = ctx.path().to_path_buf();

    // First engine: seed data + create index, then flush + drop (= restart).
    {
        let mut engine = Engine::with_data_dir(&path).expect("open engine");
        engine
            .execute_cypher("CREATE (:Restart {id: 'r1'}), (:Restart {id: 'r2'})")
            .expect("seed CREATE");
        engine
            .execute_cypher("CREATE INDEX FOR (n:Restart) ON (n.id)")
            .expect("CREATE INDEX");
        engine.flush().expect("flush");
    }

    // Reopen on the same directory — simulates a server restart.
    let mut engine = Engine::with_data_dir(&path).expect("reopen engine");
    let label_id = engine
        .catalog
        .get_label_id("Restart")
        .expect("label persisted");
    let key_id = engine.catalog.get_key_id("id").expect("key persisted");

    assert!(
        engine.indexes.property_index.has_index(label_id, key_id),
        "property index must be rebuilt after restart"
    );
    let hits = engine
        .indexes
        .property_index
        .find_exact(
            label_id,
            key_id,
            crate::index::PropertyValue::String("r1".into()),
        )
        .expect("find_exact");
    assert_eq!(
        hits.len(),
        1,
        "rebuilt index must be backfilled from storage"
    );

    // Read seek engages — no unindexed-scan notification.
    let res = engine
        .execute_cypher("MATCH (n:Restart {id: 'r1'}) RETURN n.id")
        .expect("read");
    assert_eq!(res.rows.len(), 1, "seek must find r1");
    assert!(
        !res.notifications
            .iter()
            .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
        "restored index must serve the seek (no UnindexedPropertyAccess); notes = {:?}",
        res.notifications
    );

    // Catalog existence restored: duplicate CREATE INDEX errors.
    assert!(
        engine
            .execute_cypher("CREATE INDEX FOR (n:Restart) ON (n.id)")
            .is_err(),
        "duplicate CREATE INDEX must error after restart (definition persisted)"
    );
}

/// ISSUE #12: `CALL { ... } IN TRANSACTIONS OF n ROWS` must terminate. The
/// previous batching loop re-ran the whole subquery every iteration and only
/// stopped when it returned zero rows or fewer than `n`; a subquery returning
/// `>= n` stable rows looped forever, pinning the engine write lock at 100%
/// CPU with no active-query log. This test would hang on the old code.
#[test]
#[serial_test::serial]
fn call_in_transactions_terminates() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();
    engine
        .execute_cypher("CREATE (:Seed {v: 1}), (:Seed {v: 2}), (:Seed {v: 3})")
        .expect("seed CREATE");

    // Subquery returns 3 rows >= batch size 2 — the old loop never terminated.
    let r = engine
        .execute_cypher("CALL { MATCH (s:Seed) RETURN s } IN TRANSACTIONS OF 2 ROWS")
        .expect("CALL IN TRANSACTIONS must terminate");
    assert_eq!(
        r.rows.len(),
        3,
        "subquery runs once and returns all 3 seed rows (no infinite re-execution)"
    );
}

/// ISSUE #22: the legacy CALL IN TRANSACTIONS engine path materializes
/// the whole subquery result inside one wrapper transaction; past the
/// cap it must return the structured ERR_CALL_IN_TX_RESULT_TOO_LARGE
/// error instead of OOMing. The cap check is exercised directly because
/// the legacy path is only reachable for internally dispatched ASTs —
/// top-level client queries route through the executor operator
/// (`run_call_subquery_in_transactions`), which commits per `OF n ROWS`
/// chunk and is covered by `call_in_transactions_terminates` above. The
/// call site aborts the wrapper transaction before surfacing the error,
/// so nothing is committed.
#[test]
#[serial_test::serial]
fn call_in_tx_result_cap_returns_structured_error_past_cap() {
    use crate::engine::ddl::check_call_in_tx_result_cap;

    // Under the default 1M cap.
    assert!(check_call_in_tx_result_cap(0).is_ok());
    assert!(check_call_in_tx_result_cap(1_000_000).is_ok());

    // Past the default cap — structured error, not a panic/OOM.
    let err = check_call_in_tx_result_cap(1_000_001)
        .expect_err("row count above the default cap must error");
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_RESULT_TOO_LARGE"),
        "expected structured cap error, got: {err}"
    );

    // Env knob override (NEXUS_CALL_IN_TX_MAX_ROWS) for constrained
    // deployments lowers the cap.
    // SAFETY: serialized test (no concurrent env access in this process).
    unsafe { std::env::set_var("NEXUS_CALL_IN_TX_MAX_ROWS", "2") };
    assert!(check_call_in_tx_result_cap(2).is_ok(), "at the env cap");
    let err = check_call_in_tx_result_cap(3).expect_err("above the env cap");
    assert!(
        err.to_string().contains("ERR_CALL_IN_TX_RESULT_TOO_LARGE"),
        "expected structured cap error, got: {err}"
    );
    // SAFETY: serialized test (no concurrent env access in this process).
    unsafe { std::env::remove_var("NEXUS_CALL_IN_TX_MAX_ROWS") };
}

/// ISSUE #15 (contract guard): the typed property index must stay correct
/// across an explicit BEGIN/COMMIT — after COMMIT a `MATCH (n:L {p:v})` finds
/// the committed node and uses the seek (no UnindexedPropertyAccess). Any
/// optimization of the per-commit index maintenance must keep this passing.
#[test]
#[serial_test::serial]
fn explicit_commit_keeps_property_index_seek() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:TxIdx) ON (n.id)")
        .expect("CREATE INDEX");

    engine.execute_cypher("BEGIN TRANSACTION").expect("BEGIN");
    engine
        .execute_cypher("CREATE (:TxIdx {id: 'tx1'}), (:TxIdx {id: 'tx2'})")
        .expect("CREATE in tx");
    engine.execute_cypher("COMMIT TRANSACTION").expect("COMMIT");

    // Node committed in the explicit tx is found, and the seek engages
    // (typed index maintained in-tx, not via a post-commit full rebuild).
    let res = engine
        .execute_cypher("MATCH (n:TxIdx {id: 'tx1'}) RETURN n.id")
        .expect("read");
    assert_eq!(res.rows.len(), 1, "committed node must be found");
    assert!(
        !res.notifications
            .iter()
            .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
        "typed index must still serve the seek after an explicit COMMIT; notes = {:?}",
        res.notifications
    );
}

/// ISSUE #15: after the scoped per-commit index maintenance (which
/// replaced the per-COMMIT full `rebuild_indexes_from_storage()` scan),
/// an explicit BEGIN/COMMIT must leave the label, relationship, and
/// typed property indexes correct — and a subsequent full rebuild must
/// produce the same query results (no net diff between incremental
/// commit-time maintenance and a ground-truth rebuild).
#[test]
#[serial_test::serial]
fn explicit_commit_incremental_indexes_match_full_rebuild() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE INDEX FOR (n:Inc) ON (n.id)")
        .expect("CREATE INDEX");

    // Pre-existing data outside the transaction.
    engine
        .execute_cypher("CREATE (:Inc {id: 'pre'})")
        .expect("pre-tx CREATE");

    engine.execute_cypher("BEGIN TRANSACTION").expect("BEGIN");
    engine
        .execute_cypher("CREATE (:Inc {id: 'in1'})-[:LINKED]->(:Inc {id: 'in2'})")
        .expect("CREATE in tx");
    engine.execute_cypher("COMMIT TRANSACTION").expect("COMMIT");

    let snapshot = |engine: &mut Engine| {
        let label_rows = engine
            .execute_cypher("MATCH (n:Inc) RETURN n.id ORDER BY n.id")
            .expect("label scan")
            .rows;
        let rel_rows = engine
            .execute_cypher("MATCH (:Inc {id: 'in1'})-[:LINKED]->(m:Inc) RETURN m.id")
            .expect("rel traversal")
            .rows;
        let seek = engine
            .execute_cypher("MATCH (n:Inc {id: 'in2'}) RETURN n.id")
            .expect("property seek");
        assert!(
            !seek
                .notifications
                .iter()
                .any(|n| n.code == "Nexus.Performance.UnindexedPropertyAccess"),
            "typed index must serve the seek; notes = {:?}",
            seek.notifications
        );
        (label_rows, rel_rows, seek.rows)
    };

    let incremental = snapshot(&mut engine);
    assert_eq!(
        incremental.0.len(),
        3,
        "all 3 Inc nodes visible (label index)"
    );
    assert_eq!(incremental.1.len(), 1, "committed relationship traversable");
    assert_eq!(
        incremental.2.len(),
        1,
        "committed node found via typed seek"
    );

    // Ground truth: a full rebuild from storage must not change any result.
    engine
        .rebuild_indexes_from_storage()
        .expect("full rebuild (ground truth)");
    engine.refresh_executor().expect("refresh after rebuild");
    let rebuilt = snapshot(&mut engine);
    assert_eq!(
        format!("{incremental:?}"),
        format!("{rebuilt:?}"),
        "incremental commit-time index maintenance must match a full rebuild"
    );
}

/// ISSUE #18: when the in-memory relationship index is marked dirty (after a
/// failed incremental update), the next `find_relationship_between` rebuilds it
/// from storage (self-heal) so the edge is still found and the fast path is
/// restored. Driven via engine internals (Cypher edge-MERGE upsert is gated on
/// the separate #14 edge-upsert path).
#[test]
#[serial_test::serial]
fn relationship_index_self_heals_when_dirty() {
    use std::sync::atomic::Ordering;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let a = engine
        .create_node(vec!["RH".to_string()], serde_json::json!({"id": "a"}))
        .unwrap();
    let b = engine
        .create_node(vec!["RH".to_string()], serde_json::json!({"id": "b"}))
        .unwrap();
    engine
        .create_relationship(a, b, "RR".to_string(), serde_json::json!({}))
        .unwrap();

    // Edge is found via the (populated) exact-edge fast path.
    assert!(
        engine
            .find_relationship_between(a, b, "RR")
            .unwrap()
            .is_some(),
        "edge must be found right after creation"
    );

    // Simulate a failed incremental update: wipe the in-memory relationship
    // index and mark it dirty (what crud.rs does on add_relationship error).
    engine.cache.relationship_index().clear().ok();
    engine
        .relationship_index_dirty
        .store(true, Ordering::Release);

    // The next lookup must self-heal (rebuild from storage) and still find the
    // edge, and clear the dirty flag.
    assert!(
        engine
            .find_relationship_between(a, b, "RR")
            .unwrap()
            .is_some(),
        "edge must still be found after a dirty/cleared index (self-heal from storage)"
    );
    assert!(
        !engine.relationship_index_dirty.load(Ordering::Acquire),
        "dirty flag must be cleared by the self-heal (#18)"
    );
}

/// ISSUE #18: a failed relationship-index add (simulated by a wiped index +
/// dirty flag) must NOT let a repeated edge-MERGE create a duplicate edge —
/// the existence check self-heals from storage before deciding to create.
#[test]
#[serial_test::serial]
fn merge_does_not_duplicate_edge_after_failed_index_add() {
    use std::sync::atomic::Ordering;
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    engine
        .execute_cypher("CREATE (:DH {id: 'a'}), (:DH {id: 'b'})")
        .expect("seed nodes");
    engine
        .execute_cypher("MATCH (a:DH {id: 'a'}), (b:DH {id: 'b'}) MERGE (a)-[r:DD]->(b)")
        .expect("first edge MERGE");
    let rels_after_first = engine.storage.relationship_count();

    // Simulate the #18 failure mode: index entry lost, dirty flag set.
    engine.cache.relationship_index().clear().ok();
    engine
        .relationship_index_dirty
        .store(true, Ordering::Release);

    // Re-running the MERGE must find the existing edge (self-heal / chain
    // walk against authoritative storage) and create nothing.
    engine
        .execute_cypher("MATCH (a:DH {id: 'a'}), (b:DH {id: 'b'}) MERGE (a)-[r:DD]->(b)")
        .expect("second edge MERGE");
    assert_eq!(
        engine.storage.relationship_count(),
        rels_after_first,
        "repeated MERGE after a failed index add must not duplicate the edge"
    );
}

/// ISSUE #14: `UNWIND rows AS row MATCH (a {row.fk}),(b {row.tk}) MERGE
/// (a)-[r:T]->(b) ON CREATE/ON MATCH SET r.w = row.w` upserts the edge for
/// every row (per-row MATCH after UNWIND + ON CREATE/ON MATCH SET on the edge),
/// instead of being rejected with "Unsupported clause after UNWIND".
#[test]
#[serial_test::serial]
fn unwind_match_merge_edge_upsert() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let za = engine
        .create_node(vec!["ZT".to_string()], serde_json::json!({"id": "za"}))
        .unwrap();
    let zb = engine
        .create_node(vec!["ZT".to_string()], serde_json::json!({"id": "zb"}))
        .unwrap();

    // First upsert: creates the edge and ON CREATE SET r.w = 5.
    engine
        .execute_cypher(
            "UNWIND [{fk:'za',tk:'zb',w:5}] AS row \
             MATCH (a:ZT {id: row.fk}), (b:ZT {id: row.tk}) \
             MERGE (a)-[r:ZREL]->(b) ON CREATE SET r.w = row.w ON MATCH SET r.w = row.w \
             RETURN count(r) AS c",
        )
        .expect("UNWIND+MATCH+MERGE edge upsert must succeed (not be rejected)");

    let rid = engine
        .find_relationship_between(za, zb, "ZREL")
        .unwrap()
        .expect("edge must be created by the per-row MATCH+MERGE");
    let props = engine
        .storage
        .load_relationship_properties(rid)
        .unwrap()
        .expect("edge must have properties");
    assert_eq!(
        props["w"],
        serde_json::json!(5),
        "ON CREATE SET r.w = row.w"
    );

    // Second upsert with w=7: MERGE is idempotent (still one edge) and
    // ON MATCH SET updates the weight.
    engine
        .execute_cypher(
            "UNWIND [{fk:'za',tk:'zb',w:7}] AS row \
             MATCH (a:ZT {id: row.fk}), (b:ZT {id: row.tk}) \
             MERGE (a)-[r:ZREL]->(b) ON CREATE SET r.w = row.w ON MATCH SET r.w = row.w \
             RETURN count(r) AS c",
        )
        .expect("second upsert must succeed");
    let rid2 = engine
        .find_relationship_between(za, zb, "ZREL")
        .unwrap()
        .expect("still exactly one edge (idempotent MERGE)");
    let props2 = engine
        .storage
        .load_relationship_properties(rid2)
        .unwrap()
        .expect("edge props");
    assert_eq!(
        props2["w"],
        serde_json::json!(7),
        "ON MATCH SET r.w = row.w"
    );
}

/// ISSUE #14 (multi-row): one UNWIND batch upserts an edge for EVERY row —
/// `count(r)` reflects all rows and each edge carries its own row's
/// property value.
#[test]
#[serial_test::serial]
fn unwind_match_merge_edge_upsert_every_row() {
    let ctx = crate::testing::TestContext::new();
    let mut engine = Engine::with_isolated_catalog(ctx.path()).unwrap();

    let hub = engine
        .create_node(vec!["ZM".to_string()], serde_json::json!({"id": "hub"}))
        .unwrap();
    let s1 = engine
        .create_node(vec!["ZM".to_string()], serde_json::json!({"id": "s1"}))
        .unwrap();
    let s2 = engine
        .create_node(vec!["ZM".to_string()], serde_json::json!({"id": "s2"}))
        .unwrap();

    let r = engine
        .execute_cypher(
            "UNWIND [{tk:'s1',w:1},{tk:'s2',w:2}] AS row \
             MATCH (a:ZM {id: 'hub'}), (b:ZM {id: row.tk}) \
             MERGE (a)-[r:ZMREL]->(b) ON CREATE SET r.w = row.w ON MATCH SET r.w = row.w \
             RETURN count(r) AS c",
        )
        .expect("multi-row UNWIND edge upsert must succeed");
    assert_eq!(
        r.rows[0].values[0].as_i64(),
        Some(2),
        "count(r) must reflect both rows, got {:?}",
        r.rows[0].values[0]
    );

    for (dst, w) in [(s1, 1), (s2, 2)] {
        let rid = engine
            .find_relationship_between(hub, dst, "ZMREL")
            .unwrap()
            .expect("edge must exist for every row");
        let props = engine
            .storage
            .load_relationship_properties(rid)
            .unwrap()
            .expect("edge props");
        assert_eq!(
            props["w"],
            serde_json::json!(w),
            "each edge carries its own row's property value"
        );
    }
}
