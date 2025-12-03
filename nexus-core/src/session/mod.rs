//! Session management for transaction context
//!
//! Manages active transactions per session, allowing BEGIN/COMMIT/ROLLBACK
//! to work across multiple queries in the same session.

use crate::{Error, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::transaction::{Transaction, TransactionManager};

/// Session ID type
pub type SessionId = String;

/// Session state
#[derive(Clone)]
pub struct Session {
    /// Session ID
    pub id: SessionId,
    /// Active transaction (if any)
    pub active_transaction: Option<Transaction>,
    /// Transaction manager reference
    pub transaction_manager: Arc<RwLock<TransactionManager>>,
    /// Nodes created during this transaction (for rollback)
    pub created_nodes: Vec<u64>,
    /// Relationships created during this transaction (for rollback)
    pub created_relationships: Vec<u64>,
    /// Pending index updates to be applied on commit
    pub pending_index_updates: crate::index::pending_updates::PendingIndexUpdates,
    /// Last activity timestamp
    pub last_activity: Instant,
    /// Session timeout (default: 30 minutes)
    pub timeout: Duration,
    /// Current database name (for multi-database support)
    pub current_database: String,
}

impl Session {
    /// Create a new session
    pub fn new(id: SessionId, transaction_manager: Arc<RwLock<TransactionManager>>) -> Self {
        Self::new_with_database(id, transaction_manager, "neo4j".to_string())
    }

    /// Create a new session with a specific database
    pub fn new_with_database(
        id: SessionId,
        transaction_manager: Arc<RwLock<TransactionManager>>,
        database: String,
    ) -> Self {
        Self {
            id,
            active_transaction: None,
            transaction_manager,
            created_nodes: Vec::new(),
            created_relationships: Vec::new(),
            pending_index_updates: crate::index::pending_updates::PendingIndexUpdates::new(),
            last_activity: Instant::now(),
            timeout: Duration::from_secs(30 * 60), // 30 minutes
            current_database: database,
        }
    }

    /// Check if session has an active transaction
    pub fn has_active_transaction(&self) -> bool {
        self.active_transaction
            .as_ref()
            .map(|tx| tx.is_active())
            .unwrap_or(false)
    }

    /// Begin a transaction for this session
    pub fn begin_transaction(&mut self) -> Result<()> {
        if self.has_active_transaction() {
            return Err(Error::transaction(format!(
                "Session {} already has an active transaction",
                self.id
            )));
        }

        let mut tx_mgr = self.transaction_manager.write();
        let tx = tx_mgr.begin_write()?;
        self.active_transaction = Some(tx);
        // Clear tracking for new transaction
        self.created_nodes.clear();
        self.created_relationships.clear();
        self.pending_index_updates.clear();
        self.last_activity = Instant::now();

        Ok(())
    }

    /// Commit the active transaction
    pub fn commit_transaction(&mut self) -> Result<()> {
        if let Some(mut tx) = self.active_transaction.take() {
            let mut tx_mgr = self.transaction_manager.write();
            tx_mgr.commit(&mut tx)?;
            self.last_activity = Instant::now();
            Ok(())
        } else {
            Err(Error::transaction(format!(
                "Session {} has no active transaction to commit",
                self.id
            )))
        }
    }

    /// Rollback the active transaction
    pub fn rollback_transaction(&mut self) -> Result<()> {
        if let Some(mut tx) = self.active_transaction.take() {
            let mut tx_mgr = self.transaction_manager.write();
            tx_mgr.abort(&mut tx)?;
            self.last_activity = Instant::now();
            Ok(())
        } else {
            Err(Error::transaction(format!(
                "Session {} has no active transaction to rollback",
                self.id
            )))
        }
    }

    /// Check if session has expired
    pub fn is_expired(&self) -> bool {
        self.last_activity.elapsed() > self.timeout
    }

    /// Update last activity timestamp
    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Get the current database for this session
    pub fn get_current_database(&self) -> &str {
        &self.current_database
    }

    /// Switch to a different database
    /// Note: This will fail if there's an active transaction
    pub fn switch_database(&mut self, database: String) -> Result<()> {
        if self.has_active_transaction() {
            return Err(Error::transaction(format!(
                "Cannot switch database while transaction is active in session {}",
                self.id
            )));
        }

        self.current_database = database;
        self.last_activity = Instant::now();
        Ok(())
    }

    /// Set the current database (internal use)
    pub(crate) fn set_current_database(&mut self, database: String) {
        self.current_database = database;
    }
}

/// Session manager for tracking active sessions and their transactions
pub struct SessionManager {
    /// Active sessions
    sessions: Arc<RwLock<HashMap<SessionId, Session>>>,
    /// Transaction manager (shared with Engine)
    transaction_manager: Arc<RwLock<TransactionManager>>,
    /// Session timeout (default: 30 minutes)
    timeout: Duration,
    /// Default database name
    default_database: String,
}

impl SessionManager {
    /// Create a new session manager
    pub fn new(transaction_manager: Arc<RwLock<TransactionManager>>) -> Self {
        Self::new_with_default_database(transaction_manager, "neo4j".to_string())
    }

    /// Create a new session manager with a specific default database
    pub fn new_with_default_database(
        transaction_manager: Arc<RwLock<TransactionManager>>,
        default_database: String,
    ) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            transaction_manager,
            timeout: Duration::from_secs(30 * 60), // 30 minutes
            default_database,
        }
    }

    /// Get or create a session
    pub fn get_or_create_session(&self, session_id: SessionId) -> Session {
        let mut sessions = self.sessions.write();

        // Check if session exists and is not expired
        if let Some(session) = sessions.get(&session_id) {
            if !session.is_expired() {
                // Touch session to update last activity
                let mut session = session.clone();
                session.touch();
                sessions.insert(session_id.clone(), session.clone());
                return session;
            } else {
                // Session expired, remove it
                sessions.remove(&session_id);
            }
        }

        // Create new session with default database
        let session = Session::new_with_database(
            session_id.clone(),
            self.transaction_manager.clone(),
            self.default_database.clone(),
        );
        sessions.insert(session_id.clone(), session.clone());
        session
    }

    /// Get or create a session with a specific database
    pub fn get_or_create_session_with_database(
        &self,
        session_id: SessionId,
        database: Option<String>,
    ) -> Session {
        let mut sessions = self.sessions.write();

        // Check if session exists and is not expired
        if let Some(session) = sessions.get(&session_id) {
            if !session.is_expired() {
                // Touch session to update last activity
                let mut session = session.clone();
                session.touch();

                // If database is specified and different, switch to it
                if let Some(db) = database {
                    if session.current_database != db && !session.has_active_transaction() {
                        session.set_current_database(db);
                    }
                }

                sessions.insert(session_id.clone(), session.clone());
                return session;
            } else {
                // Session expired, remove it
                sessions.remove(&session_id);
            }
        }

        // Create new session with specified or default database
        let db_name = database.unwrap_or_else(|| self.default_database.clone());
        let session = Session::new_with_database(
            session_id.clone(),
            self.transaction_manager.clone(),
            db_name,
        );
        sessions.insert(session_id.clone(), session.clone());
        session
    }

    /// Get a session (returns None if not found or expired)
    pub fn get_session(&self, session_id: &SessionId) -> Option<Session> {
        let mut sessions = self.sessions.write();

        if let Some(session) = sessions.get(session_id) {
            if session.is_expired() {
                sessions.remove(session_id);
                return None;
            }
            let session = Session {
                id: session.id.clone(),
                active_transaction: session.active_transaction.clone(),
                transaction_manager: session.transaction_manager.clone(),
                created_nodes: session.created_nodes.clone(),
                created_relationships: session.created_relationships.clone(),
                pending_index_updates: session.pending_index_updates.clone(),
                last_activity: Instant::now(),
                timeout: session.timeout,
                current_database: session.current_database.clone(),
            };
            sessions.insert(session_id.clone(), session.clone());
            Some(session)
        } else {
            None
        }
    }

    /// Update a session
    pub fn update_session(&self, session: Session) {
        let mut sessions = self.sessions.write();
        sessions.insert(session.id.clone(), session);
    }

    /// Remove a session
    pub fn remove_session(&self, session_id: &SessionId) {
        let mut sessions = self.sessions.write();
        sessions.remove(session_id);
    }

    /// Clean up expired sessions
    pub fn cleanup_expired_sessions(&self) {
        let mut sessions = self.sessions.write();
        sessions.retain(|_, session| !session.is_expired());
    }

    /// Get all active session IDs
    pub fn get_active_session_ids(&self) -> Vec<SessionId> {
        let sessions = self.sessions.read();
        sessions
            .keys()
            .filter(|id| sessions.get(*id).map(|s| !s.is_expired()).unwrap_or(false))
            .cloned()
            .collect()
    }

    /// Get the default database name
    pub fn default_database(&self) -> &str {
        &self.default_database
    }

    /// Switch database for a session
    /// Returns an error if the session has an active transaction
    pub fn switch_session_database(&self, session_id: &SessionId, database: String) -> Result<()> {
        let mut sessions = self.sessions.write();

        if let Some(session) = sessions.get_mut(session_id) {
            if session.is_expired() {
                return Err(Error::transaction(format!(
                    "Session '{}' has expired",
                    session_id
                )));
            }

            if session.has_active_transaction() {
                return Err(Error::transaction(format!(
                    "Cannot switch database while transaction is active in session '{}'",
                    session_id
                )));
            }

            session.set_current_database(database);
            session.touch();
            Ok(())
        } else {
            Err(Error::transaction(format!(
                "Session '{}' not found",
                session_id
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_begin_commit() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let mut session = Session::new("test-session".to_string(), tx_mgr.clone());

        // Begin transaction
        session.begin_transaction().unwrap();
        assert!(session.has_active_transaction());

        // Commit transaction
        session.commit_transaction().unwrap();
        assert!(!session.has_active_transaction());
    }

    #[test]
    fn test_session_begin_rollback() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let mut session = Session::new("test-session".to_string(), tx_mgr.clone());

        // Begin transaction
        session.begin_transaction().unwrap();
        assert!(session.has_active_transaction());

        // Rollback transaction
        session.rollback_transaction().unwrap();
        assert!(!session.has_active_transaction());
    }

    #[test]
    fn test_session_manager() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let session_mgr = SessionManager::new(tx_mgr);

        // Get or create session
        let session = session_mgr.get_or_create_session("test-session".to_string());
        assert_eq!(session.id, "test-session");

        // Get same session
        let session2 = session_mgr.get_or_create_session("test-session".to_string());
        assert_eq!(session2.id, "test-session");
    }

    #[test]
    fn test_session_database_binding() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let session_mgr = SessionManager::new(tx_mgr);

        // Create session - should use default database
        let session = session_mgr.get_or_create_session("test-session".to_string());
        assert_eq!(session.get_current_database(), "neo4j");
        assert_eq!(session_mgr.default_database(), "neo4j");
    }

    #[test]
    fn test_session_switch_database() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let mut session = Session::new("test-session".to_string(), tx_mgr);

        // Default database
        assert_eq!(session.get_current_database(), "neo4j");

        // Switch to another database
        session.switch_database("testdb".to_string()).unwrap();
        assert_eq!(session.get_current_database(), "testdb");

        // Switch back
        session.switch_database("neo4j".to_string()).unwrap();
        assert_eq!(session.get_current_database(), "neo4j");
    }

    #[test]
    fn test_session_switch_database_with_transaction_fails() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let mut session = Session::new("test-session".to_string(), tx_mgr);

        // Begin transaction
        session.begin_transaction().unwrap();

        // Try to switch database - should fail
        let result = session.switch_database("testdb".to_string());
        assert!(result.is_err());

        // Commit transaction
        session.commit_transaction().unwrap();

        // Now switch should work
        let result = session.switch_database("testdb".to_string());
        assert!(result.is_ok());
        assert_eq!(session.get_current_database(), "testdb");
    }

    #[test]
    fn test_session_manager_switch_database() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let session_mgr = SessionManager::new(tx_mgr);

        // Create session
        let session = session_mgr.get_or_create_session("test-session".to_string());
        assert_eq!(session.get_current_database(), "neo4j");

        // Switch database via manager
        session_mgr
            .switch_session_database(&"test-session".to_string(), "testdb".to_string())
            .unwrap();

        // Get session again - should have new database
        let session = session_mgr
            .get_session(&"test-session".to_string())
            .unwrap();
        assert_eq!(session.get_current_database(), "testdb");
    }

    #[test]
    fn test_session_manager_with_database() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let session_mgr = SessionManager::new(tx_mgr);

        // Create session with specific database
        let session = session_mgr.get_or_create_session_with_database(
            "test-session".to_string(),
            Some("customdb".to_string()),
        );
        assert_eq!(session.get_current_database(), "customdb");

        // Get or create with None should use existing session
        let session2 =
            session_mgr.get_or_create_session_with_database("test-session".to_string(), None);
        assert_eq!(session2.get_current_database(), "customdb");
    }

    #[test]
    fn test_session_manager_default_database() {
        let tx_mgr = Arc::new(RwLock::new(TransactionManager::new().unwrap()));
        let session_mgr =
            SessionManager::new_with_default_database(tx_mgr, "mydefault".to_string());

        assert_eq!(session_mgr.default_database(), "mydefault");

        // New sessions should use the custom default
        let session = session_mgr.get_or_create_session("test-session".to_string());
        assert_eq!(session.get_current_database(), "mydefault");
    }
}
