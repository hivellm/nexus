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
