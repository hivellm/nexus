//! LMDB storage for API keys and authentication data

use crate::Result;
use heed::types::*;
use heed::{Database, Env, EnvOpenOptions};
use std::path::Path;
use std::sync::Arc;

use super::api_key::ApiKey;

/// Storage for API keys using LMDB
#[derive(Debug, Clone)]
pub struct ApiKeyStorage {
    /// LMDB environment
    env: Arc<Env>,
    /// API key ID → ApiKey mapping
    api_keys_db: Database<Str, SerdeBincode<ApiKey>>,
}

impl ApiKeyStorage {
    /// Create a new API key storage instance
    ///
    /// Opens or creates LMDB environment at specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path for LMDB files
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        use std::sync::OnceLock;

        // In test mode, use a shared directory pool to reduce number of LMDB environments
        // This prevents TlsFull errors when many tests run in parallel
        let is_test = std::env::var("CARGO_PKG_NAME").is_ok()
            || std::env::var("CARGO").is_ok()
            || std::env::args().any(|arg| arg.contains("test") || arg.contains("cargo"));

        // In test mode, use a fixed map_size to avoid BadOpenOptions errors
        let map_size = if is_test {
            100 * 1024 * 1024 // 100MB fixed size for tests
        } else {
            1024 * 1024 * 1024 // 1GB for production
        };

        let actual_path = if is_test {
            // Use a SINGLE shared test directory for ALL auth storage in tests
            // This prevents TlsFull errors on Windows by limiting to just 1 LMDB environment
            static AUTH_STORAGE_DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

            let shared_dir = AUTH_STORAGE_DIR.get_or_init(|| {
                let base = std::env::temp_dir().join("nexus_test_auth_storage_shared");
                // Clean up old data on first init
                let _ = std::fs::remove_dir_all(&base);
                std::fs::create_dir_all(&base).ok();
                base
            });

            shared_dir.clone()
        } else {
            path.as_ref().to_path_buf()
        };

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&actual_path)?;

        // Open LMDB environment
        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(map_size)
                .max_dbs(2)
                .open(&actual_path)?
        };
        let env = Arc::new(env);

        // Open/create database
        let mut wtxn = env.write_txn()?;
        let api_keys_db = env.create_database(&mut wtxn, Some("api_keys"))?;
        wtxn.commit()?;

        Ok(Self { env, api_keys_db })
    }

    /// Create storage with an isolated path (bypasses test sharing)
    ///
    /// WARNING: Use sparingly! Each call creates a new LMDB environment.
    /// Only use for tests that absolutely require data isolation.
    #[cfg(test)]
    pub fn with_isolated_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        std::fs::create_dir_all(path.as_ref())?;

        let env = unsafe {
            EnvOpenOptions::new()
                .map_size(100 * 1024 * 1024)
                .max_dbs(2)
                .open(path.as_ref())?
        };
        let env = Arc::new(env);

        let mut wtxn = env.write_txn()?;
        let api_keys_db = env.create_database(&mut wtxn, Some("api_keys"))?;
        wtxn.commit()?;

        Ok(Self { env, api_keys_db })
    }

    /// Store an API key
    pub fn store_api_key(&self, api_key: &ApiKey) -> Result<()> {
        let mut wtxn = self.env.write_txn()?;
        self.api_keys_db.put(&mut wtxn, &api_key.id, api_key)?;
        wtxn.commit()?;
        Ok(())
    }

    /// Get an API key by ID
    pub fn get_api_key(&self, key_id: &str) -> Result<Option<ApiKey>> {
        let rtxn = self.env.read_txn()?;
        let result = self.api_keys_db.get(&rtxn, key_id)?;
        Ok(result)
    }

    /// Delete an API key
    pub fn delete_api_key(&self, key_id: &str) -> Result<bool> {
        let mut wtxn = self.env.write_txn()?;
        let existed = self.api_keys_db.delete(&mut wtxn, key_id)?;
        wtxn.commit()?;
        Ok(existed)
    }

    /// List all API keys
    pub fn list_api_keys(&self) -> Result<Vec<ApiKey>> {
        let rtxn = self.env.read_txn()?;
        let mut keys = Vec::new();
        for result in self.api_keys_db.iter(&rtxn)? {
            let (_, api_key) = result?;
            keys.push(api_key);
        }
        Ok(keys)
    }

    /// Get API keys for a specific user
    pub fn get_api_keys_for_user(&self, user_id: &str) -> Result<Vec<ApiKey>> {
        let rtxn = self.env.read_txn()?;
        let mut keys = Vec::new();
        for result in self.api_keys_db.iter(&rtxn)? {
            let (_, api_key) = result?;
            if api_key
                .user_id
                .as_ref()
                .map(|id| id == user_id)
                .unwrap_or(false)
            {
                keys.push(api_key);
            }
        }
        Ok(keys)
    }

    /// Update an API key
    pub fn update_api_key(&self, api_key: &ApiKey) -> Result<()> {
        self.store_api_key(api_key)
    }

    /// Clean up expired keys
    pub fn cleanup_expired_keys(&self) -> Result<usize> {
        let mut wtxn = self.env.write_txn()?;
        let mut count = 0;
        let mut keys_to_delete = Vec::new();

        // Collect expired keys
        for result in self.api_keys_db.iter(&wtxn)? {
            let (key_id, api_key) = result?;
            if api_key.is_expired() {
                keys_to_delete.push(key_id.to_string());
            }
        }

        // Delete expired keys
        for key_id in keys_to_delete {
            if self.api_keys_db.delete(&mut wtxn, &key_id)? {
                count += 1;
            }
        }

        wtxn.commit()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::super::permissions::Permission;
    use super::*;
    use chrono::{Duration, Utc};
    use tempfile::TempDir;

    fn create_test_api_key(id: &str, name: &str) -> ApiKey {
        ApiKey {
            id: id.to_string(),
            name: name.to_string(),
            user_id: None,
            permissions: vec![Permission::Read],
            hashed_key: "test_hash".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
        }
    }

    #[test]
    fn test_api_key_storage_creation() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();
        assert!(storage.list_api_keys().unwrap().is_empty());
    }

    #[test]
    fn test_store_and_retrieve_api_key() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        let api_key = create_test_api_key("test-id", "test-key");
        storage.store_api_key(&api_key).unwrap();

        let retrieved = storage.get_api_key("test-id").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "test-id");
        assert_eq!(retrieved.name, "test-key");
    }

    #[test]
    fn test_delete_api_key() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        let api_key = create_test_api_key("test-id", "test-key");
        storage.store_api_key(&api_key).unwrap();

        assert!(storage.get_api_key("test-id").unwrap().is_some());
        assert!(storage.delete_api_key("test-id").unwrap());
        assert!(storage.get_api_key("test-id").unwrap().is_none());
        assert!(!storage.delete_api_key("test-id").unwrap());
    }

    #[test]
    fn test_list_api_keys() {
        let temp_dir = TempDir::new().unwrap();
        // Use isolated path for tests that count items
        let storage = ApiKeyStorage::with_isolated_path(temp_dir.path()).unwrap();

        let key1 = create_test_api_key("list-id1", "list-key1");
        let key2 = create_test_api_key("list-id2", "list-key2");
        storage.store_api_key(&key1).unwrap();
        storage.store_api_key(&key2).unwrap();

        let keys = storage.list_api_keys().unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn test_get_api_keys_for_user() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        let mut key1 = create_test_api_key("id1", "key1");
        key1.user_id = Some("user1".to_string());
        let mut key2 = create_test_api_key("id2", "key2");
        key2.user_id = Some("user2".to_string());
        let key3 = create_test_api_key("id3", "key3");

        storage.store_api_key(&key1).unwrap();
        storage.store_api_key(&key2).unwrap();
        storage.store_api_key(&key3).unwrap();

        let user1_keys = storage.get_api_keys_for_user("user1").unwrap();
        assert_eq!(user1_keys.len(), 1);
        assert_eq!(user1_keys[0].id, "id1");

        let user2_keys = storage.get_api_keys_for_user("user2").unwrap();
        assert_eq!(user2_keys.len(), 1);
        assert_eq!(user2_keys[0].id, "id2");
    }

    #[test]
    fn test_cleanup_expired_keys() {
        let temp_dir = TempDir::new().unwrap();
        // Use isolated path for cleanup tests
        let storage = ApiKeyStorage::with_isolated_path(temp_dir.path()).unwrap();

        let mut expired_key = create_test_api_key("cleanup-expired", "cleanup-expired-key");
        expired_key.expires_at = Some(Utc::now() - Duration::days(1));

        let mut valid_key = create_test_api_key("cleanup-valid", "cleanup-valid-key");
        valid_key.expires_at = Some(Utc::now() + Duration::days(1));

        storage.store_api_key(&expired_key).unwrap();
        storage.store_api_key(&valid_key).unwrap();

        let count = storage.cleanup_expired_keys().unwrap();
        assert_eq!(count, 1);

        assert!(storage.get_api_key("cleanup-expired").unwrap().is_none());
        assert!(storage.get_api_key("cleanup-valid").unwrap().is_some());
    }

    #[test]
    fn test_update_api_key() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        let mut api_key = create_test_api_key("test-id", "test-key");
        storage.store_api_key(&api_key).unwrap();

        api_key.name = "updated-key".to_string();
        storage.update_api_key(&api_key).unwrap();

        let retrieved = storage.get_api_key("test-id").unwrap().unwrap();
        assert_eq!(retrieved.name, "updated-key");
    }

    #[test]
    fn test_cleanup_expired_keys_edge_cases() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        // Test with no expired keys
        let mut valid_key = create_test_api_key("valid", "valid-key");
        valid_key.expires_at = Some(Utc::now() + Duration::days(1));
        storage.store_api_key(&valid_key).unwrap();

        let count = storage.cleanup_expired_keys().unwrap();
        assert_eq!(count, 0);
        assert!(storage.get_api_key("valid").unwrap().is_some());

        // Test with multiple expired keys
        for i in 0..5 {
            let mut expired_key =
                create_test_api_key(&format!("expired-{}", i), &format!("expired-key-{}", i));
            expired_key.expires_at = Some(Utc::now() - Duration::days(i + 1));
            storage.store_api_key(&expired_key).unwrap();
        }

        let count = storage.cleanup_expired_keys().unwrap();
        assert_eq!(count, 5);

        // All expired keys should be gone
        for i in 0..5 {
            assert!(
                storage
                    .get_api_key(&format!("expired-{}", i))
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn test_cleanup_expired_keys_with_none_expiration() {
        let temp_dir = TempDir::new().unwrap();
        let storage = ApiKeyStorage::new(temp_dir.path()).unwrap();

        // Keys with None expiration should not be cleaned up
        let mut no_expiration_key = create_test_api_key("no-exp", "no-exp-key");
        no_expiration_key.expires_at = None;
        storage.store_api_key(&no_expiration_key).unwrap();

        let count = storage.cleanup_expired_keys().unwrap();
        assert_eq!(count, 0);
        assert!(storage.get_api_key("no-exp").unwrap().is_some());
    }

    #[test]
    fn test_cleanup_expired_keys_boundary_time() {
        let temp_dir = TempDir::new().unwrap();
        // Use isolated path for cleanup tests
        let storage = ApiKeyStorage::with_isolated_path(temp_dir.path()).unwrap();

        // Key expiring exactly now (boundary case)
        let mut boundary_key = create_test_api_key("boundary-test", "boundary-test-key");
        boundary_key.expires_at = Some(Utc::now());
        storage.store_api_key(&boundary_key).unwrap();

        // Small delay to ensure it's expired
        std::thread::sleep(std::time::Duration::from_millis(100));

        let count = storage.cleanup_expired_keys().unwrap();
        assert_eq!(count, 1);
        assert!(storage.get_api_key("boundary-test").unwrap().is_none());
    }
}
