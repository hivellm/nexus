//! DBMS procedures for system management
//!
//! This module provides DBMS procedures similar to Neo4j:
//! - dbms.showCurrentUser()
//! - dbms.listConfig()
//! - dbms.listConnections()
//! - dbms.killQuery()
//! - dbms.clearQueryCaches()

use super::connection_tracking::ConnectionTracker;
use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// DBMS procedure result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbmsProcedureResult {
    /// Column names
    pub columns: Vec<String>,
    /// Rows of data
    pub rows: Vec<Vec<serde_json::Value>>,
}

/// DBMS procedures handler
pub struct DbmsProcedures {
    /// Current user (if available)
    current_user: Option<String>,
    /// Configuration map
    config: HashMap<String, serde_json::Value>,
    /// Connection tracker
    connection_tracker: Arc<ConnectionTracker>,
}

impl DbmsProcedures {
    /// Create a new DBMS procedures handler
    pub fn new() -> Self {
        let mut config = HashMap::new();

        // Add default configuration
        config.insert(
            "dbms.memory.heap.max_size".to_string(),
            serde_json::json!("512m"),
        );
        config.insert(
            "dbms.transaction.timeout".to_string(),
            serde_json::json!(60000),
        );
        config.insert("dbms.query_cache_size".to_string(), serde_json::json!(100));
        config.insert(
            "dbms.slow_query_threshold_ms".to_string(),
            serde_json::json!(1000),
        );

        Self {
            current_user: None,
            config,
            connection_tracker: Arc::new(ConnectionTracker::new()),
        }
    }

    /// Set current user
    pub fn set_current_user(&mut self, username: Option<String>) {
        self.current_user = username;
    }

    /// Execute dbms.showCurrentUser()
    pub fn show_current_user(&self) -> DbmsProcedureResult {
        let username = self.current_user.as_deref().unwrap_or("anonymous");

        DbmsProcedureResult {
            columns: vec!["username".to_string()],
            rows: vec![vec![serde_json::Value::String(username.to_string())]],
        }
    }

    /// Execute dbms.listConfig()
    pub fn list_config(&self) -> DbmsProcedureResult {
        let mut rows = Vec::new();

        for (key, value) in &self.config {
            rows.push(vec![
                serde_json::Value::String(key.clone()),
                value.clone(),
                serde_json::Value::String("dynamic".to_string()), // Default to dynamic
            ]);
        }

        DbmsProcedureResult {
            columns: vec![
                "name".to_string(),
                "value".to_string(),
                "description".to_string(),
            ],
            rows,
        }
    }

    /// Execute dbms.listConnections()
    pub fn list_connections(&self) -> DbmsProcedureResult {
        let connections = self.connection_tracker.get_connections();
        let mut rows = Vec::new();

        for conn in connections {
            rows.push(vec![
                serde_json::Value::String(conn.connection_id),
                serde_json::Value::String(conn.username.unwrap_or_else(|| "anonymous".to_string())),
                serde_json::Value::String(conn.connected_at.to_string()),
                serde_json::Value::String(conn.client_address),
            ]);
        }

        DbmsProcedureResult {
            columns: vec![
                "connectionId".to_string(),
                "username".to_string(),
                "connectedAt".to_string(),
                "clientAddress".to_string(),
            ],
            rows,
        }
    }

    /// Get connection tracker (for external use)
    pub fn get_connection_tracker(&self) -> Arc<ConnectionTracker> {
        self.connection_tracker.clone()
    }

    /// Execute dbms.killQuery(queryId)
    pub fn kill_query(&self, query_id: &str) -> Result<DbmsProcedureResult> {
        let cancelled = self.connection_tracker.cancel_query(query_id);
        let status = if cancelled {
            "killed".to_string()
        } else {
            "not_found_or_already_completed".to_string()
        };

        Ok(DbmsProcedureResult {
            columns: vec!["queryId".to_string(), "status".to_string()],
            rows: vec![vec![
                serde_json::Value::String(query_id.to_string()),
                serde_json::Value::String(status),
            ]],
        })
    }

    /// Execute dbms.clearQueryCaches()
    pub fn clear_query_caches(&self) -> DbmsProcedureResult {
        // This would clear the plan cache
        // For now, just return success
        DbmsProcedureResult {
            columns: vec!["action".to_string(), "status".to_string()],
            rows: vec![vec![
                serde_json::Value::String("clearQueryCaches".to_string()),
                serde_json::Value::String("success".to_string()),
            ]],
        }
    }
}

impl Default for DbmsProcedures {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_show_current_user() {
        let mut procedures = DbmsProcedures::new();
        procedures.set_current_user(Some("admin".to_string()));

        let result = procedures.show_current_user();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0][0],
            serde_json::Value::String("admin".to_string())
        );
    }

    #[test]
    fn test_list_config() {
        let procedures = DbmsProcedures::new();
        let result = procedures.list_config();

        assert_eq!(result.columns.len(), 3);
        assert!(!result.rows.is_empty());
    }

    #[test]
    fn test_clear_query_caches() {
        let procedures = DbmsProcedures::new();
        let result = procedures.clear_query_caches();

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows.len(), 1);
    }

    #[test]
    fn test_show_current_user_anonymous() {
        let procedures = DbmsProcedures::new(); // No user set

        let result = procedures.show_current_user();
        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0][0],
            serde_json::Value::String("anonymous".to_string())
        );
    }

    #[test]
    fn test_list_config_content() {
        let procedures = DbmsProcedures::new();
        let result = procedures.list_config();

        assert_eq!(result.columns.len(), 3);
        assert!(!result.rows.is_empty());

        // Check that config entries have correct structure
        for row in &result.rows {
            assert_eq!(row.len(), 3); // name, value, description
        }
    }

    #[test]
    fn test_list_connections() {
        let procedures = DbmsProcedures::new();
        let result = procedures.list_connections();

        assert_eq!(result.columns.len(), 4);
        assert_eq!(result.columns[0], "connectionId");
        assert_eq!(result.columns[1], "username");
        assert_eq!(result.columns[2], "connectedAt");
        assert_eq!(result.columns[3], "clientAddress");
        // Empty for now (no connection tracking yet)
        assert_eq!(result.rows.len(), 0);
    }

    #[test]
    fn test_kill_query() {
        let procedures = DbmsProcedures::new();
        let tracker = procedures.get_connection_tracker();

        // First register a connection and query
        let conn_id = tracker
            .register_connection(Some("testuser".to_string()), "127.0.0.1:12345".to_string());
        let query_id = tracker.register_query(conn_id, "MATCH (n) RETURN n".to_string());

        // Now kill the query
        let result = procedures.kill_query(&query_id).unwrap();

        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(
            result.rows[0][0],
            serde_json::Value::String(query_id.clone())
        );
        assert_eq!(
            result.rows[0][1],
            serde_json::Value::String("killed".to_string())
        );

        // Test killing non-existent query
        let result2 = procedures.kill_query("nonexistent").unwrap();
        assert_eq!(result2.rows.len(), 1);
        assert_eq!(
            result2.rows[0][1],
            serde_json::Value::String("not_found_or_already_completed".to_string())
        );
    }

    #[test]
    fn test_set_current_user() {
        let mut procedures = DbmsProcedures::new();

        procedures.set_current_user(Some("testuser".to_string()));
        let result = procedures.show_current_user();
        assert_eq!(
            result.rows[0][0],
            serde_json::Value::String("testuser".to_string())
        );

        procedures.set_current_user(None);
        let result2 = procedures.show_current_user();
        assert_eq!(
            result2.rows[0][0],
            serde_json::Value::String("anonymous".to_string())
        );
    }
}
