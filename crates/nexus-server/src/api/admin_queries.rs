//! `/admin/queries` — read-only operator surface for triaging a
//! wedged server (phase6_slow-query-log-and-active-queries §3).
//!
//! When the writer thread is saturated (the 2026-05-04 cortex-nexus
//! 100 % CPU incident), `/cypher` and `/stats` time out and the
//! Cypher `SHOW QUERIES` introspection path is unreachable for the
//! same reason. This endpoint reads from the same in-memory tracker
//! (`ConnectionTracker::get_queries`) but lives on a dedicated route
//! that only touches the lock guarding the active-query map — never
//! the executor — so it stays responsive while the writer is
//! blocked.
//!
//! The shape mirrors `SHOW QUERIES` (Neo4j-compatible columns) but
//! delivered as JSON for fast `curl` triage.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::NexusServer;

/// Per-query entry in the `/admin/queries` JSON envelope.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ActiveQueryEntry {
    /// Tracker-assigned id (`query-N`). Stable for the lifetime of
    /// the query; matches the `queryId` column on `SHOW QUERIES`
    /// and the argument to `TERMINATE QUERY '...'`.
    pub query_id: String,
    /// Owning connection id (`conn-N`). Joinable against
    /// `ConnectionInfo` if the operator wants the client address.
    pub connection_id: String,
    /// Cypher text. Truncated to 8 KiB at the wire boundary so a
    /// runaway parameter dump doesn't blow up the response payload;
    /// the original is intact in the tracker.
    pub query: String,
    /// Unix start time, seconds since epoch.
    pub started_at_secs: u64,
    /// Wall-clock elapsed time at the moment the request was
    /// served. Computed against `SystemTime::now()` — a single
    /// snapshot for all entries in the response so they all read
    /// against the same `now` reference.
    pub elapsed_ms: u64,
    /// Lifecycle state — one of `running`, `cancelled`, `completed`.
    /// Completed queries linger up to `QUERY_MAX_AGE_SECS` before
    /// the cleanup tick reaps them; surfacing them here lets
    /// operators see what just finished.
    pub status: &'static str,
}

/// Top-level shape of the `/admin/queries` response. Stable across
/// releases; new fields are additive.
#[derive(Debug, Clone, Serialize)]
pub struct AdminQueriesResponse {
    /// Total entries in the tracker — running + recently completed.
    /// Useful to distinguish "no queries" from "tracker is empty
    /// because cleanup just ran".
    pub total: usize,
    /// Subset of `entries` where `status == "running"` — duplicated
    /// at the top level so a `curl | jq '.running'` triage script
    /// doesn't need to filter.
    pub running: usize,
    /// All entries, sorted by `elapsed_ms` descending so the
    /// longest-running query is first. Operators looking for the
    /// wedged query don't have to sort the response.
    pub entries: Vec<ActiveQueryEntry>,
    /// Wire schema version — bumped on breaking shape changes.
    pub schema_version: u32,
}

/// `GET /admin/queries` handler.
pub async fn list_queries(State(server): State<Arc<NexusServer>>) -> Json<AdminQueriesResponse> {
    let tracker = server.dbms_procedures.get_connection_tracker();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut entries: Vec<ActiveQueryEntry> = tracker
        .get_queries()
        .into_iter()
        .map(|q| {
            let elapsed_ms = now.saturating_sub(q.started_at) * 1000;
            let status = if q.cancelled {
                "cancelled"
            } else if q.is_running {
                "running"
            } else {
                "completed"
            };
            let query = if q.query.len() > 8192 {
                let mut t: String = q.query.chars().take(8192).collect();
                t.push_str("…<<truncated>>");
                t
            } else {
                q.query
            };
            ActiveQueryEntry {
                query_id: q.query_id,
                connection_id: q.connection_id,
                query,
                started_at_secs: q.started_at,
                elapsed_ms,
                status,
            }
        })
        .collect();

    entries.sort_by_key(|e| std::cmp::Reverse(e.elapsed_ms));
    let total = entries.len();
    let running = entries.iter().filter(|e| e.status == "running").count();

    Json(AdminQueriesResponse {
        total,
        running,
        entries,
        schema_version: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Building blocks for response sorting / status mapping.
    /// The HTTP shell (server bootstrapping, auth) is exercised by
    /// the broader integration suite.
    #[test]
    fn entries_serialize_with_documented_field_names() {
        let entry = ActiveQueryEntry {
            query_id: "query-7".into(),
            connection_id: "conn-3".into(),
            query: "MATCH (n) RETURN n".into(),
            started_at_secs: 1_700_000_000,
            elapsed_ms: 1234,
            status: "running",
        };
        let v = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(v["query_id"], "query-7");
        assert_eq!(v["connection_id"], "conn-3");
        assert_eq!(v["elapsed_ms"], 1234);
        assert_eq!(v["status"], "running");
    }

    #[test]
    fn response_envelope_carries_total_running_and_schema_version() {
        let resp = AdminQueriesResponse {
            total: 3,
            running: 2,
            entries: Vec::new(),
            schema_version: 1,
        };
        let v = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(v["total"], 3);
        assert_eq!(v["running"], 2);
        assert_eq!(v["schema_version"], 1);
        assert!(v["entries"].is_array());
    }
}
