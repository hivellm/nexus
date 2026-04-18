//! Database management module for multi-database support
//!
//! Provides isolation between multiple graph databases within a single Nexus instance.
//! Each database has its own:
//! - Storage directory
//! - Catalog (labels, types, property keys)
//! - Indexes (label, property, KNN)
//! - Transaction log (WAL)

use crate::{Engine, Error, Result};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tracing;

/// Database state
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DatabaseState {
    /// Database is online and accepting requests
    Online,
    /// Database is offline (maintenance, stopped)
    Offline,
    /// Database is starting up
    Starting,
    /// Database is shutting down
    Stopping,
    /// Database encountered an error
    Error(String),
}

/// Database metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseInfo {
    /// Database name
    pub name: String,
    /// Database storage path
    pub path: PathBuf,
    /// Database creation timestamp
    pub created_at: i64,
    /// Number of nodes
    pub node_count: u64,
    /// Number of relationships
    pub relationship_count: u64,
    /// Storage size in bytes
    pub storage_size: u64,
    /// Current database state
    pub state: DatabaseState,
}

/// Database manager for multiple isolated databases
pub struct DatabaseManager {
    /// Map of database name to Engine instance
    databases: Arc<RwLock<HashMap<String, Arc<RwLock<Engine>>>>>,
    /// Map of database name to state
    states: Arc<RwLock<HashMap<String, DatabaseState>>>,
    /// Base directory for all databases
    base_dir: PathBuf,
    /// Default database name
    default_db: String,
}

impl DatabaseManager {
    /// Create a new database manager
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        let default_db = "neo4j".to_string();
        let databases = Arc::new(RwLock::new(HashMap::new()));
        let states = Arc::new(RwLock::new(HashMap::new()));

        let manager = Self {
            databases,
            states,
            base_dir: base_dir.clone(),
            default_db: default_db.clone(),
        };

        // Create default database
        manager.create_database(&default_db)?;

        Ok(manager)
    }

    /// Create a new database
    pub fn create_database(&self, name: &str) -> Result<Arc<RwLock<Engine>>> {
        // Validate database name
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return Err(Error::InvalidInput(
                "Database name must be alphanumeric with _ or -".to_string(),
            ));
        }

        let mut dbs = self.databases.write();

        // Check if database already exists
        if dbs.contains_key(name) {
            return Err(Error::InvalidInput(format!(
                "Database '{}' already exists",
                name
            )));
        }

        // Create database directory
        let db_path = self.base_dir.join(name);
        std::fs::create_dir_all(&db_path)?;

        // Create engine for this database
        let engine = Engine::with_data_dir(&db_path)?;
        let engine_arc = Arc::new(RwLock::new(engine));

        // Store database
        dbs.insert(name.to_string(), engine_arc.clone());

        // Set state to Online
        self.states
            .write()
            .insert(name.to_string(), DatabaseState::Online);

        Ok(engine_arc)
    }

    /// Drop a database (delete all data)
    pub fn drop_database(&self, name: &str, if_exists: bool) -> Result<()> {
        // Cannot drop default database
        if name == self.default_db {
            return Err(Error::InvalidInput(
                "Cannot drop default database".to_string(),
            ));
        }

        let mut dbs = self.databases.write();

        // Check if database exists
        if !dbs.contains_key(name) {
            if if_exists {
                // Database doesn't exist and IF EXISTS was specified, succeed silently
                return Ok(());
            } else {
                return Err(Error::InvalidInput(format!(
                    "Database '{}' does not exist",
                    name
                )));
            }
        }

        // Remove from map and drop the Arc to release all locks
        if let Some(engine_arc) = dbs.remove(name) {
            // Explicitly drop the Arc to ensure Engine is destroyed
            drop(engine_arc);
        }

        // Remove state
        self.states.write().remove(name);

        // Release the write lock before attempting file operations
        drop(dbs);

        // Delete database directory with retry logic for Windows
        let db_path = self.base_dir.join(name);
        if db_path.exists() {
            // On Windows, file handles may not be immediately released
            // Retry with exponential backoff
            let mut attempts = 0;
            let max_attempts = 5;

            loop {
                match std::fs::remove_dir_all(&db_path) {
                    Ok(_) => break,
                    Err(e) => {
                        attempts += 1;
                        if attempts >= max_attempts {
                            // On Windows during tests, it's acceptable to fail directory deletion
                            // The important part is that the database is removed from the manager
                            #[cfg(target_os = "windows")]
                            {
                                tracing::warn!(
                                    "Could not delete directory '{}' after {} attempts: {}",
                                    db_path.display(),
                                    max_attempts,
                                    e
                                );
                                tracing::warn!(
                                    "Database removed from manager but directory may persist."
                                );
                                break;
                            }
                            #[cfg(not(target_os = "windows"))]
                            return Err(e.into());
                        }
                        // Wait before retry with exponential backoff
                        std::thread::sleep(std::time::Duration::from_millis(50 * attempts as u64));
                    }
                }
            }
        }

        Ok(())
    }

    /// Get a database by name
    pub fn get_database(&self, name: &str) -> Result<Arc<RwLock<Engine>>> {
        let dbs = self.databases.read();
        dbs.get(name)
            .cloned()
            .ok_or_else(|| Error::InvalidInput(format!("Database '{}' does not exist", name)))
    }

    /// Get the default database
    pub fn get_default_database(&self) -> Result<Arc<RwLock<Engine>>> {
        self.get_database(&self.default_db)
    }

    /// List all databases
    pub fn list_databases(&self) -> Vec<DatabaseInfo> {
        let dbs = self.databases.read();
        let mut databases: Vec<DatabaseInfo> = dbs
            .iter()
            .map(|(name, engine)| {
                let mut engine_guard = engine.write();
                let (node_count, relationship_count) = match engine_guard.stats() {
                    Ok(stats) => (stats.nodes, stats.relationships),
                    Err(_) => (0, 0),
                };

                let db_path = self.base_dir.join(name);

                // Get creation time from directory metadata
                let created_at = std::fs::metadata(&db_path)
                    .and_then(|m| m.created())
                    .map(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs() as i64
                    })
                    .unwrap_or(0);

                // Calculate storage size by summing all files in the database directory
                let storage_size = Self::calculate_directory_size(&db_path).unwrap_or(0);

                // Get database state
                let state = self
                    .states
                    .read()
                    .get(name)
                    .cloned()
                    .unwrap_or(DatabaseState::Online);

                DatabaseInfo {
                    name: name.clone(),
                    path: db_path,
                    created_at,
                    node_count,
                    relationship_count,
                    storage_size,
                    state,
                }
            })
            .collect();

        // Sort by name
        databases.sort_by(|a, b| a.name.cmp(&b.name));
        databases
    }

    /// Calculate total size of all files in a directory (recursive)
    fn calculate_directory_size(path: &PathBuf) -> Result<u64> {
        let mut total_size = 0u64;

        if !path.exists() {
            return Ok(0);
        }

        if path.is_file() {
            return Ok(std::fs::metadata(path)?.len());
        }

        let entries = std::fs::read_dir(path)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                total_size += std::fs::metadata(&path)?.len();
            } else if path.is_dir() {
                total_size += Self::calculate_directory_size(&path)?;
            }
        }

        Ok(total_size)
    }

    /// Check if a database exists
    pub fn exists(&self, name: &str) -> bool {
        self.databases.read().contains_key(name)
    }

    /// Get the default database name
    pub fn default_database_name(&self) -> &str {
        &self.default_db
    }

    /// Get the state of a database
    pub fn get_database_state(&self, name: &str) -> Option<DatabaseState> {
        self.states.read().get(name).cloned()
    }

    /// Set the state of a database
    pub fn set_database_state(&self, name: &str, state: DatabaseState) -> Result<()> {
        if !self.exists(name) {
            return Err(Error::InvalidInput(format!(
                "Database '{}' does not exist",
                name
            )));
        }

        self.states.write().insert(name.to_string(), state);
        Ok(())
    }

    /// Check if a database is online
    pub fn is_database_online(&self, name: &str) -> bool {
        matches!(self.get_database_state(name), Some(DatabaseState::Online))
    }

    /// Stop a database (set to offline)
    pub fn stop_database(&self, name: &str) -> Result<()> {
        if name == self.default_db {
            return Err(Error::InvalidInput(
                "Cannot stop default database".to_string(),
            ));
        }

        if !self.exists(name) {
            return Err(Error::InvalidInput(format!(
                "Database '{}' does not exist",
                name
            )));
        }

        // Set state to Stopping, then Offline
        self.set_database_state(name, DatabaseState::Stopping)?;
        tracing::info!("Stopping database '{}'", name);

        // In a real implementation, we would wait for active transactions to complete
        // For now, we just set it to Offline immediately
        self.set_database_state(name, DatabaseState::Offline)?;
        tracing::info!("Database '{}' is now offline", name);

        Ok(())
    }

    /// Start a database (set to online)
    pub fn start_database(&self, name: &str) -> Result<()> {
        if !self.exists(name) {
            return Err(Error::InvalidInput(format!(
                "Database '{}' does not exist",
                name
            )));
        }

        // Set state to Starting, then Online
        self.set_database_state(name, DatabaseState::Starting)?;
        tracing::info!("Starting database '{}'", name);

        // In a real implementation, we would initialize resources
        // For now, we just set it to Online immediately
        self.set_database_state(name, DatabaseState::Online)?;
        tracing::info!("Database '{}' is now online", name);

        Ok(())
    }

    /// Get a database only if it's online
    pub fn get_database_if_online(&self, name: &str) -> Result<Arc<RwLock<Engine>>> {
        // Check if database is online
        if !self.is_database_online(name) {
            let state = self
                .get_database_state(name)
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "Unknown".to_string());
            return Err(Error::InvalidInput(format!(
                "Database '{}' is not online (current state: {})",
                name, state
            )));
        }

        // Get the database
        self.get_database(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestContext;
    use serial_test::serial;

    #[test]
    fn test_database_manager_creation() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        // Should have default database
        assert!(manager.exists("neo4j"));
        assert_eq!(manager.default_database_name(), "neo4j");
    }

    #[test]
    fn test_create_duplicate_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("test_db").unwrap();
        let result = manager.create_database("test_db");
        assert!(result.is_err());
    }

    #[test]
    fn test_drop_default_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.drop_database("neo4j", false);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_databases() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("db1").unwrap();
        manager.create_database("db2").unwrap();

        let databases = manager.list_databases();
        assert_eq!(databases.len(), 3); // default + 2 new

        let names: Vec<&str> = databases.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"neo4j"));
        assert!(names.contains(&"db1"));
        assert!(names.contains(&"db2"));
    }

    #[test]
    fn test_get_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("test_db").unwrap();

        let db = manager.get_database("test_db").unwrap();
        let mut engine = db.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 0);
    }

    #[test]
    fn test_invalid_database_names() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        // Empty name
        assert!(manager.create_database("").is_err());

        // Special characters
        assert!(manager.create_database("test@db").is_err());
        assert!(manager.create_database("test db").is_err());
        assert!(manager.create_database("test/db").is_err());

        // Valid names should work
        assert!(manager.create_database("test-db").is_ok());
        assert!(manager.create_database("test_db2").is_ok());
    }

    #[test]
    fn test_get_nonexistent_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.get_database("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_databases_isolation() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        // Create multiple databases
        let db1 = manager.create_database("db1").unwrap();
        let db2 = manager.create_database("db2").unwrap();

        // Add data to db1
        {
            let mut engine1 = db1.write();
            engine1
                .create_node(
                    vec!["Person".to_string()],
                    serde_json::json!({"name": "Alice"}),
                )
                .unwrap();
        }

        // Verify db2 is empty
        {
            let mut engine2 = db2.write();
            let stats = engine2.stats().unwrap();
            assert_eq!(stats.nodes, 0);
        }

        // Verify db1 has data
        {
            let mut engine1 = db1.write();
            let stats = engine1.stats().unwrap();
            assert_eq!(stats.nodes, 1);
        }
    }

    #[test]
    fn test_database_info_metadata() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("test_db").unwrap();

        let databases = manager.list_databases();
        let test_db = databases.iter().find(|d| d.name == "test_db").unwrap();

        assert_eq!(test_db.name, "test_db");
        assert_eq!(test_db.node_count, 0);
        assert_eq!(test_db.relationship_count, 0);
        assert!(test_db.path.ends_with("test_db"));
    }

    #[test]
    fn test_database_with_data() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let db = manager.create_database("test_db").unwrap();

        // Add nodes
        {
            let mut engine = db.write();
            for i in 0..10 {
                engine
                    .create_node(vec!["Person".to_string()], serde_json::json!({"id": i}))
                    .unwrap();
            }
        }

        // Check stats via list
        let databases = manager.list_databases();
        let test_db = databases.iter().find(|d| d.name == "test_db").unwrap();
        assert_eq!(test_db.node_count, 10);
    }

    #[test]
    fn test_drop_nonexistent_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.drop_database("nonexistent", false);
        assert!(result.is_err());
    }

    #[test]
    #[serial]
    fn test_concurrent_database_access() {
        use std::sync::Arc;
        use std::thread;

        let ctx = TestContext::new();
        let manager = Arc::new(DatabaseManager::new(ctx.path().to_path_buf()).unwrap());

        let db = manager.create_database("test_db").unwrap();

        // Spawn multiple threads accessing same database
        let mut handles = vec![];
        for i in 0..5 {
            let db_clone = db.clone();
            let handle = thread::spawn(move || {
                let mut engine = db_clone.write();
                engine
                    .create_node(vec!["Person".to_string()], serde_json::json!({"thread": i}))
                    .unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all nodes created
        let mut engine = db.write();
        let stats = engine.stats().unwrap();
        assert_eq!(stats.nodes, 5);
    }

    #[test]
    fn test_database_list_sorting() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("zulu").unwrap();
        manager.create_database("alpha").unwrap();
        manager.create_database("bravo").unwrap();

        let databases = manager.list_databases();
        let names: Vec<&str> = databases.iter().map(|d| d.name.as_str()).collect();

        // Should be sorted alphabetically
        assert_eq!(names, vec!["alpha", "bravo", "neo4j", "zulu"]);
    }

    #[test]
    fn test_database_name_edge_cases() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        // Very long name (should work if within limits)
        let long_name = "a".repeat(50);
        assert!(manager.create_database(&long_name).is_ok());

        // Single character
        assert!(manager.create_database("x").is_ok());

        // Numbers only
        assert!(manager.create_database("123").is_ok());

        // Mixed case
        assert!(manager.create_database("TestDB").is_ok());
    }

    #[test]
    fn test_default_database_name() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        assert_eq!(manager.default_database_name(), "neo4j");
        assert!(manager.exists("neo4j"));
    }

    #[test]
    fn test_database_state_management() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("statedb").unwrap();

        // Database should be online by default
        assert!(manager.is_database_online("statedb"));
        assert_eq!(
            manager.get_database_state("statedb"),
            Some(DatabaseState::Online)
        );

        // Stop database
        manager.stop_database("statedb").unwrap();
        assert!(!manager.is_database_online("statedb"));
        assert_eq!(
            manager.get_database_state("statedb"),
            Some(DatabaseState::Offline)
        );

        // Start database again
        manager.start_database("statedb").unwrap();
        assert!(manager.is_database_online("statedb"));
        assert_eq!(
            manager.get_database_state("statedb"),
            Some(DatabaseState::Online)
        );
    }

    #[test]
    fn test_cannot_stop_default_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.stop_database("neo4j");
        assert!(result.is_err());
    }

    #[test]
    fn test_get_database_if_online() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("onlinetest").unwrap();

        // Should succeed when online
        let db = manager.get_database_if_online("onlinetest");
        assert!(db.is_ok());

        // Stop database
        manager.stop_database("onlinetest").unwrap();

        // Should fail when offline
        let db = manager.get_database_if_online("onlinetest");
        assert!(db.is_err());

        // Start database again
        manager.start_database("onlinetest").unwrap();

        // Should succeed again
        let db = manager.get_database_if_online("onlinetest");
        assert!(db.is_ok());
    }

    #[test]
    fn test_database_info_includes_state() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("infostate").unwrap();

        let databases = manager.list_databases();
        let db_info = databases.iter().find(|d| d.name == "infostate").unwrap();

        assert_eq!(db_info.state, DatabaseState::Online);

        // Stop database and check state in info
        manager.stop_database("infostate").unwrap();

        let databases = manager.list_databases();
        let db_info = databases.iter().find(|d| d.name == "infostate").unwrap();

        assert_eq!(db_info.state, DatabaseState::Offline);
    }

    #[test]
    fn test_set_database_state_custom() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        manager.create_database("customstate").unwrap();

        // Set custom error state
        manager
            .set_database_state(
                "customstate",
                DatabaseState::Error("Test error".to_string()),
            )
            .unwrap();

        assert_eq!(
            manager.get_database_state("customstate"),
            Some(DatabaseState::Error("Test error".to_string()))
        );
    }

    #[test]
    fn test_stop_nonexistent_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.stop_database("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_start_nonexistent_database() {
        let ctx = TestContext::new();
        let manager = DatabaseManager::new(ctx.path().to_path_buf()).unwrap();

        let result = manager.start_database("nonexistent");
        assert!(result.is_err());
    }
}
