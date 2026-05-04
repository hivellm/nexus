//! Connection and query tracking for DBMS procedures
//!
//! This module provides:
//! - Connection tracking for dbms.listConnections()
//! - Query tracking for dbms.killQuery()

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    /// Connection ID
    pub connection_id: String,
    /// Username (if authenticated)
    pub username: Option<String>,
    /// Connection timestamp (Unix timestamp)
    pub connected_at: u64,
    /// Client address
    pub client_address: String,
    /// Last activity timestamp
    pub last_activity: u64,
}

/// Query information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryInfo {
    /// Query ID
    pub query_id: String,
    /// Query text
    pub query: String,
    /// Connection ID that started the query
    pub connection_id: String,
    /// Start timestamp (Unix timestamp)
    pub started_at: u64,
    /// Whether query is still running
    pub is_running: bool,
    /// Cancellation token (for future implementation)
    pub cancelled: bool,
}

/// Connection and query tracker
pub struct ConnectionTracker {
    /// Active connections
    connections: Arc<RwLock<HashMap<String, ConnectionInfo>>>,
    /// Active queries
    queries: Arc<RwLock<HashMap<String, QueryInfo>>>,
    /// Connection counter for generating IDs
    connection_counter: Arc<RwLock<u64>>,
    /// Query counter for generating IDs
    query_counter: Arc<RwLock<u64>>,
}

/// RAII guard that marks a registered query completed on drop.
///
/// Without this guard, `mark_query_completed` is called manually at
/// the bottom of every Cypher HTTP handler. Any panic, early return,
/// or path that bypasses the manual call leaks the query in the
/// `is_running=true` state — which then keeps the slow-query log
/// barking forever and inflates `SHOW QUERIES` output until the
/// 10-minute `cleanup_old_queries` sweep evicts it.
///
/// `RegisteredQueryGuard` plugs that hole: callers register via
/// [`ConnectionTracker::register_query_guarded`] and the returned
/// guard's `Drop` impl calls `complete_query`. Drop runs unwinding
/// past `?` returns and during panic, so the contract holds even
/// when the query path errors out abnormally.
pub struct RegisteredQueryGuard {
    tracker: Arc<RwLock<HashMap<String, QueryInfo>>>,
    query_id: String,
    /// Set to `true` by [`Self::take_id`] when a caller wants to opt
    /// out of automatic completion (e.g. they intend to mark the
    /// query cancelled manually). Skips the `complete_query` call in
    /// `Drop`.
    disarmed: bool,
}

impl RegisteredQueryGuard {
    /// The query id that was registered. Useful for the caller to
    /// report back to the client (Neo4j-compatible response shape
    /// includes `query_id` so `TERMINATE QUERY` can target it).
    pub fn query_id(&self) -> &str {
        &self.query_id
    }

    /// Disarm the guard and consume it, returning the query id.
    /// The caller takes responsibility for calling
    /// `complete_query` / `cancel_query` themselves. Used by code
    /// paths that need to mark the query as `cancelled` rather than
    /// `completed` on the way out.
    pub fn take_id(mut self) -> String {
        self.disarmed = true;
        std::mem::take(&mut self.query_id)
    }
}

impl Drop for RegisteredQueryGuard {
    fn drop(&mut self) {
        if self.disarmed || self.query_id.is_empty() {
            return;
        }
        // Best-effort: if the lock is poisoned (a panic happened
        // while another thread held the write lock), the tracker
        // map is already in an inconsistent state. Log and move on
        // — the cleanup tick will eventually evict the orphan.
        match self.tracker.write() {
            Ok(mut queries) => {
                if let Some(q) = queries.get_mut(&self.query_id) {
                    q.is_running = false;
                }
            }
            Err(_) => {
                tracing::warn!(
                    query_id = %self.query_id,
                    "RegisteredQueryGuard: poisoned tracker lock; orphan entry will be \
                     reaped by cleanup_old_queries",
                );
            }
        }
    }
}

impl ConnectionTracker {
    /// Create a new connection tracker
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            queries: Arc::new(RwLock::new(HashMap::new())),
            connection_counter: Arc::new(RwLock::new(0)),
            query_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// Register a new connection
    pub fn register_connection(&self, username: Option<String>, client_address: String) -> String {
        let mut counter = self.connection_counter.write().unwrap();
        *counter += 1;
        let connection_id = format!("conn-{}", *counter);
        drop(counter);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let connection = ConnectionInfo {
            connection_id: connection_id.clone(),
            username,
            connected_at: timestamp,
            client_address,
            last_activity: timestamp,
        };

        self.connections
            .write()
            .unwrap()
            .insert(connection_id.clone(), connection);
        connection_id
    }

    /// Update connection activity
    pub fn update_connection_activity(&self, connection_id: &str) {
        let mut connections = self.connections.write().unwrap();
        if let Some(conn) = connections.get_mut(connection_id) {
            conn.last_activity = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
        }
    }

    /// Unregister a connection
    pub fn unregister_connection(&self, connection_id: &str) {
        self.connections.write().unwrap().remove(connection_id);
        // Also remove any queries associated with this connection
        let mut queries = self.queries.write().unwrap();
        queries.retain(|_, q| q.connection_id != connection_id);
    }

    /// Register a new query
    pub fn register_query(&self, connection_id: String, query: String) -> String {
        let mut counter = self.query_counter.write().unwrap();
        *counter += 1;
        let query_id = format!("query-{}", *counter);
        drop(counter);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let query_info = QueryInfo {
            query_id: query_id.clone(),
            query,
            connection_id,
            started_at: timestamp,
            is_running: true,
            cancelled: false,
        };

        self.queries
            .write()
            .unwrap()
            .insert(query_id.clone(), query_info);
        query_id
    }

    /// Register a query and return an RAII guard whose `Drop` impl
    /// calls `complete_query`. Use this from every code path that
    /// can early-return or panic — the guard ensures the
    /// `is_running` flag flips back to `false` even when the
    /// surrounding handler bails out abnormally.
    ///
    /// The guard exposes the query id via [`RegisteredQueryGuard::query_id`]
    /// so callers that need to surface it to the client (e.g. the
    /// Cypher response envelope) can read it without a separate
    /// `register_query` call.
    pub fn register_query_guarded(
        &self,
        connection_id: String,
        query: String,
    ) -> RegisteredQueryGuard {
        let query_id = self.register_query(connection_id, query);
        RegisteredQueryGuard {
            tracker: Arc::clone(&self.queries),
            query_id,
            disarmed: false,
        }
    }

    /// Mark query as completed
    pub fn complete_query(&self, query_id: &str) {
        let mut queries = self.queries.write().unwrap();
        if let Some(query) = queries.get_mut(query_id) {
            query.is_running = false;
        }
    }

    /// Cancel a query
    pub fn cancel_query(&self, query_id: &str) -> bool {
        let mut queries = self.queries.write().unwrap();
        if let Some(query) = queries.get_mut(query_id) {
            if query.is_running {
                query.cancelled = true;
                query.is_running = false;
                return true;
            }
        }
        false
    }

    /// Get all active connections
    pub fn get_connections(&self) -> Vec<ConnectionInfo> {
        self.connections.read().unwrap().values().cloned().collect()
    }

    /// Get all active queries
    pub fn get_queries(&self) -> Vec<QueryInfo> {
        self.queries.read().unwrap().values().cloned().collect()
    }

    /// Get running queries
    pub fn get_running_queries(&self) -> Vec<QueryInfo> {
        self.queries
            .read()
            .unwrap()
            .values()
            .filter(|q| q.is_running)
            .cloned()
            .collect()
    }

    /// Clean up old completed queries (older than max_age_seconds)
    pub fn cleanup_old_queries(&self, max_age_seconds: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut queries = self.queries.write().unwrap();
        queries.retain(|_, q| {
            if !q.is_running {
                (now - q.started_at) < max_age_seconds
            } else {
                true
            }
        });
    }

    /// Evict connections whose `last_activity` is older than `max_idle_seconds`.
    ///
    /// Counterpart to `cleanup_old_queries`: if a client disconnects without
    /// calling `unregister_connection`, its entry would otherwise live in the
    /// map forever. Queries associated with the evicted connections are also
    /// dropped to keep both maps in sync.
    pub fn cleanup_stale_connections(&self, max_idle_seconds: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let stale_ids: Vec<String> = {
            let connections = self.connections.read().unwrap();
            connections
                .iter()
                .filter_map(|(id, conn)| {
                    now.checked_sub(conn.last_activity)
                        .filter(|idle| *idle > max_idle_seconds)
                        .map(|_| id.clone())
                })
                .collect()
        };

        if stale_ids.is_empty() {
            return;
        }

        let mut connections = self.connections.write().unwrap();
        for id in &stale_ids {
            connections.remove(id);
        }
        drop(connections);

        let mut queries = self.queries.write().unwrap();
        queries.retain(|_, q| !stale_ids.contains(&q.connection_id));
    }
}

impl Default for ConnectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_connection() {
        let tracker = ConnectionTracker::new();
        let conn_id =
            tracker.register_connection(Some("user1".to_string()), "127.0.0.1:12345".to_string());

        assert!(conn_id.starts_with("conn-"));
        let connections = tracker.get_connections();
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].username, Some("user1".to_string()));
    }

    #[test]
    fn test_register_query() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:12345".to_string());
        let query_id = tracker.register_query(conn_id.clone(), "MATCH (n) RETURN n".to_string());

        assert!(query_id.starts_with("query-"));
        let queries = tracker.get_running_queries();
        assert_eq!(queries.len(), 1);
        assert_eq!(queries[0].query, "MATCH (n) RETURN n");
    }

    #[test]
    fn test_cancel_query() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:12345".to_string());
        let query_id = tracker.register_query(conn_id, "MATCH (n) RETURN n".to_string());

        assert!(tracker.cancel_query(&query_id));
        let queries = tracker.get_running_queries();
        assert_eq!(queries.len(), 0);
    }

    #[test]
    fn test_unregister_connection() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:12345".to_string());
        tracker.register_query(conn_id.clone(), "MATCH (n) RETURN n".to_string());

        tracker.unregister_connection(&conn_id);
        let connections = tracker.get_connections();
        assert_eq!(connections.len(), 0);
        let queries = tracker.get_running_queries();
        assert_eq!(queries.len(), 0); // Queries should be removed too
    }

    #[test]
    fn registered_query_guard_marks_completed_on_drop() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:1".to_string());
        {
            let _guard =
                tracker.register_query_guarded(conn_id.clone(), "MATCH (n) RETURN n".to_string());
            assert_eq!(
                tracker.get_running_queries().len(),
                1,
                "running while in scope"
            );
        } // guard drops here
        assert_eq!(
            tracker.get_running_queries().len(),
            0,
            "guard drop must mark the query completed"
        );
    }

    #[test]
    fn registered_query_guard_runs_on_panic_unwind() {
        // The guard's `Drop` runs during stack unwinding, so a
        // panic inside the registered scope still flips
        // `is_running` to `false`. This is the load-bearing
        // contract — without it, a panic in the executor would
        // leave the query "running" forever in `SHOW QUERIES`.
        let tracker = Arc::new(ConnectionTracker::new());
        let conn_id = tracker.register_connection(None, "127.0.0.1:1".to_string());

        let tracker_clone = Arc::clone(&tracker);
        let conn_clone = conn_id.clone();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard =
                tracker_clone.register_query_guarded(conn_clone, "MATCH (n) RETURN n".to_string());
            panic!("simulated executor panic");
        }));
        assert!(panicked.is_err(), "panic should propagate");
        assert_eq!(
            tracker.get_running_queries().len(),
            0,
            "guard must mark completed even on panic unwind"
        );
    }

    #[test]
    fn registered_query_guard_take_id_disarms_drop() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:1".to_string());
        let guard = tracker.register_query_guarded(conn_id, "MATCH (n) RETURN n".to_string());
        let qid = guard.take_id();

        // Disarmed: the query is still running until manually
        // completed/cancelled.
        assert_eq!(tracker.get_running_queries().len(), 1);
        assert!(!qid.is_empty());

        // Caller takes responsibility for completion.
        tracker.complete_query(&qid);
        assert_eq!(tracker.get_running_queries().len(), 0);
    }

    #[test]
    fn test_cleanup_stale_connections() {
        let tracker = ConnectionTracker::new();
        let conn_id = tracker.register_connection(None, "127.0.0.1:12345".to_string());
        tracker.register_query(conn_id.clone(), "MATCH (n) RETURN n".to_string());

        // Fast-forward the recorded last_activity into the distant past so the
        // connection is considered idle past any reasonable threshold.
        {
            let mut conns = tracker.connections.write().unwrap();
            let entry = conns.get_mut(&conn_id).unwrap();
            entry.last_activity = 0;
        }

        // max_idle = 10s; the doctored connection has idle > now, so it goes.
        tracker.cleanup_stale_connections(10);

        assert_eq!(tracker.get_connections().len(), 0);
        assert_eq!(tracker.get_running_queries().len(), 0);
    }
}
