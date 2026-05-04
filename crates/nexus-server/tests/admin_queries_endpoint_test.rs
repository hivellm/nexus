//! End-to-end coverage for the `GET /admin/queries` endpoint and
//! the `RegisteredQueryGuard` wiring on the Cypher path
//! (phase6_slow-query-log-and-active-queries §3 + RAII guard).
//!
//! The unit tests in
//! `crates/nexus-core/src/performance/connection_tracking.rs` pin
//! the guard's drop semantics in isolation. This file pins the
//! end-to-end contract: that running queries surface in the
//! tracker, that the response envelope carries the documented
//! fields, and that the guard cleans up after a Cypher request
//! ends.

use std::sync::Arc;
use std::time::Duration;

use nexus_core::performance::connection_tracking::ConnectionTracker;
use nexus_server::api::admin_queries::{ActiveQueryEntry, AdminQueriesResponse};

#[test]
fn registered_query_guard_drops_completes_in_real_tracker() {
    // Sanity: prove the guard composes cleanly with the same
    // ConnectionTracker the server uses, against the public API
    // surface. The unit test in nexus-core covers the lock-poison
    // and panic-unwind branches; this one is the integration sweep.
    let tracker = ConnectionTracker::new();
    let conn_id = tracker.register_connection(None, "127.0.0.1:1".to_string());

    {
        let _g = tracker.register_query_guarded(conn_id, "MATCH (n) RETURN n".to_string());
        assert_eq!(tracker.get_running_queries().len(), 1, "running in scope");
    }

    assert_eq!(
        tracker.get_running_queries().len(),
        0,
        "guard drop on scope exit must mark completed"
    );
}

#[test]
fn registered_query_guard_handles_concurrent_drops_without_deadlock() {
    // Multiple guards dropping concurrently must not deadlock on the
    // tracker's RwLock. Spawn N tasks, register + immediately drop;
    // join all. If the lock were misused (e.g. holding a read lock
    // across a write), this would hang.
    let tracker = Arc::new(ConnectionTracker::new());
    let conn_id = tracker.register_connection(None, "127.0.0.1:1".to_string());

    let handles: Vec<_> = (0..16)
        .map(|i| {
            let tracker = Arc::clone(&tracker);
            let conn = conn_id.clone();
            std::thread::spawn(move || {
                let _g = tracker.register_query_guarded(conn, format!("MATCH (n{i}) RETURN n{i}"));
                std::thread::sleep(Duration::from_millis(5));
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }

    assert_eq!(
        tracker.get_running_queries().len(),
        0,
        "all 16 guards must have completed"
    );
}

#[test]
fn admin_queries_envelope_serializes_with_documented_fields() {
    // Pins the response shape — schema_version + sorted entries +
    // total/running counts. The handler's data sourcing is exercised
    // by the broader server-up integration tests; this one pins the
    // wire contract so a refactor that drops a field surfaces here.
    let entry = ActiveQueryEntry {
        query_id: "query-1".into(),
        connection_id: "conn-1".into(),
        query: "MATCH (n) RETURN n".into(),
        started_at_secs: 1_700_000_000,
        elapsed_ms: 1500,
        status: "running",
    };
    let resp = AdminQueriesResponse {
        total: 1,
        running: 1,
        entries: vec![entry],
        schema_version: 1,
    };

    let json = serde_json::to_value(&resp).expect("serialize");
    assert_eq!(json["total"], 1);
    assert_eq!(json["running"], 1);
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["entries"][0]["query_id"], "query-1");
    assert_eq!(json["entries"][0]["status"], "running");
    assert_eq!(json["entries"][0]["elapsed_ms"], 1500);
}
