//! Authentication and authorization system for Nexus
//!
//! This module provides API key management, authentication middleware,
//! and role-based access control (RBAC) for the Nexus graph database.

pub mod api_key;
pub mod audit;
pub mod chatroom_permissions;
pub mod database_permissions;
pub mod jwt;
pub mod middleware;
pub mod password;
pub mod permissions;
pub mod queue_permissions;
pub mod rbac;
pub mod storage;

pub use api_key::ApiKey;
pub use audit::{
    AuditConfig, AuditLogEntry, AuditLogger, AuditOperation, AuditResult, WriteOperationParams,
};
pub use chatroom_permissions::{
    ChatroomOperation, can_manage_chatroom, can_read_chatroom, can_send_chatroom,
    check_chatroom_permission,
};
pub use database_permissions::{DatabaseACL, DatabasePermission, check_database_access};
pub use jwt::{JwtConfig, JwtManager, TokenPair};
pub use middleware::{
    AuthContext, AuthError, AuthMiddleware, audit_log_failures_total, record_audit_log_failure,
};
#[cfg(feature = "axum")]
pub use middleware::{extract_auth_context, extract_user_context};
pub use password::{hash_password, needs_rehash, verify_dummy_password, verify_password};
pub use permissions::{Permission, PermissionSet};
pub use queue_permissions::{
    QueueOperation, can_consume_queue, can_manage_queue, can_publish_queue, check_queue_permission,
};
pub use rbac::{Role, RoleBasedAccessControl, User};
pub use storage::ApiKeyStorage;

use anyhow::Result;
use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Whether authentication is required for public binding (0.0.0.0)
    pub required_for_public: bool,
    /// Default permissions for new API keys
    pub default_permissions: Vec<Permission>,
    /// Rate limiting configuration
    pub rate_limits: RateLimits,
}

/// Rate limiting configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    /// Requests per minute
    pub per_minute: u32,
    /// Requests per hour
    pub per_hour: u32,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            required_for_public: true,
            default_permissions: vec![Permission::Read, Permission::Write],
            rate_limits: RateLimits {
                per_minute: 1000,
                per_hour: 10000,
            },
        }
    }
}

/// Authentication manager
#[derive(Debug)]
pub struct AuthManager {
    config: AuthConfig,
    api_keys: Arc<RwLock<HashMap<String, ApiKey>>>,
    storage: Option<Arc<ApiKeyStorage>>,
    argon2: Argon2<'static>,
}

impl AuthManager {
    /// Create a new authentication manager (in-memory only)
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            storage: None,
            argon2: Argon2::default(),
        }
    }

    /// Create a new authentication manager with LMDB storage
    pub fn with_storage<P: AsRef<std::path::Path>>(
        config: AuthConfig,
        storage_path: P,
    ) -> Result<Self> {
        let storage = ApiKeyStorage::new(storage_path)?;

        // Load existing keys from storage
        let existing_keys = storage.list_api_keys()?;
        let mut api_keys = HashMap::new();
        for key in existing_keys {
            api_keys.insert(key.id.clone(), key);
        }

        Ok(Self {
            config,
            api_keys: Arc::new(RwLock::new(api_keys)),
            storage: Some(Arc::new(storage)),
            argon2: Argon2::default(),
        })
    }

    /// Generate a new API key
    pub fn generate_api_key(
        &self,
        name: String,
        permissions: Vec<Permission>,
    ) -> Result<(ApiKey, String)> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        // The key ID is embedded right after the `nx_` prefix so
        // `verify_api_key` can look up the single candidate key in O(1)
        // instead of running Argon2 against every stored key (see
        // `extract_embedded_key_id`).
        let full_key = format!("nx_{key_id}_{key_secret}");

        // Hash the full key for storage
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .argon2
            .hash_password(full_key.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash API key: {}", e))?;

        let api_key = ApiKey {
            id: key_id,
            name,
            user_id: None,
            permissions,
            hashed_key: password_hash.to_string(),
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        // Persist to LMDB if storage is available
        if let Some(storage) = &self.storage {
            storage.store_api_key(&api_key)?;
        }

        Ok((api_key, full_key))
    }

    /// Generate a new API key for a user
    pub fn generate_api_key_for_user(
        &self,
        name: String,
        user_id: String,
        permissions: Vec<Permission>,
    ) -> Result<(ApiKey, String)> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        // The key ID is embedded right after the `nx_` prefix so
        // `verify_api_key` can look up the single candidate key in O(1)
        // instead of running Argon2 against every stored key (see
        // `extract_embedded_key_id`).
        let full_key = format!("nx_{key_id}_{key_secret}");

        // Hash the full key for storage
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .argon2
            .hash_password(full_key.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash API key: {}", e))?;

        let api_key = ApiKey {
            id: key_id,
            name,
            user_id: Some(user_id),
            permissions,
            hashed_key: password_hash.to_string(),
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        // Persist to LMDB if storage is available
        if let Some(storage) = &self.storage {
            storage.store_api_key(&api_key)?;
        }

        Ok((api_key, full_key))
    }

    /// Generate a new temporary API key with expiration
    pub fn generate_temporary_api_key(
        &self,
        name: String,
        permissions: Vec<Permission>,
        expires_at: DateTime<Utc>,
    ) -> Result<(ApiKey, String)> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        // The key ID is embedded right after the `nx_` prefix so
        // `verify_api_key` can look up the single candidate key in O(1)
        // instead of running Argon2 against every stored key (see
        // `extract_embedded_key_id`).
        let full_key = format!("nx_{key_id}_{key_secret}");

        // Hash the full key for storage
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .argon2
            .hash_password(full_key.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash API key: {}", e))?;

        let api_key = ApiKey {
            id: key_id,
            name,
            user_id: None,
            permissions,
            hashed_key: password_hash.to_string(),
            created_at: Utc::now(),
            expires_at: Some(expires_at),
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        // Persist to LMDB if storage is available
        if let Some(storage) = &self.storage {
            storage.store_api_key(&api_key)?;
        }

        Ok((api_key, full_key))
    }

    /// Generate a new API key for a user with expiration
    pub fn generate_api_key_for_user_with_expiration(
        &self,
        name: String,
        user_id: String,
        permissions: Vec<Permission>,
        expires_at: DateTime<Utc>,
    ) -> Result<(ApiKey, String)> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        // The key ID is embedded right after the `nx_` prefix so
        // `verify_api_key` can look up the single candidate key in O(1)
        // instead of running Argon2 against every stored key (see
        // `extract_embedded_key_id`).
        let full_key = format!("nx_{key_id}_{key_secret}");

        // Hash the full key for storage
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self
            .argon2
            .hash_password(full_key.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash API key: {}", e))?;

        let api_key = ApiKey {
            id: key_id,
            name,
            user_id: Some(user_id),
            permissions,
            hashed_key: password_hash.to_string(),
            created_at: Utc::now(),
            expires_at: Some(expires_at),
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        // Persist to LMDB if storage is available
        if let Some(storage) = &self.storage {
            storage.store_api_key(&api_key)?;
        }

        Ok((api_key, full_key))
    }

    /// Verify an API key.
    ///
    /// Current-format keys (`nx_{key_id}_{secret}`, see [`Self::generate_api_key`])
    /// embed their own key ID, so the matching stored key can be looked up
    /// in O(1) and exactly one Argon2 verify is performed — cost no longer
    /// scales with the number of stored keys. Legacy keys minted before
    /// this format existed (`nx_{secret}`, no embedded ID) fall back to the
    /// original linear scan over valid keys so already-issued keys keep
    /// working.
    pub fn verify_api_key(&self, key: &str) -> Result<Option<ApiKey>> {
        if !self.config.enabled {
            return Ok(None);
        }

        if !key.starts_with("nx_") {
            return Ok(None);
        }

        match Self::extract_embedded_key_id(key) {
            Some(key_id) => {
                let candidate = {
                    let keys_guard = self.api_keys.read();
                    keys_guard.get(&key_id).filter(|k| k.is_valid()).cloned()
                };
                match candidate {
                    Some(api_key) => self.verify_candidate(key, api_key),
                    // A well-formed-but-unknown embedded ID is a forged or
                    // stale token, not a legacy key — deliberately do NOT
                    // fall back to the linear scan here, or a forged token
                    // with a syntactically valid ID would still cost O(N).
                    None => Ok(None),
                }
            }
            None => self.verify_by_linear_scan(key),
        }
    }

    /// Extract the key ID embedded in a current-format token
    /// (`nx_{uuid}_{secret}`), if `key` matches that shape. Returns `None`
    /// for legacy tokens (`nx_{secret}`, no `_` separator after the prefix)
    /// so [`Self::verify_api_key`] knows to fall back to the O(N) scan
    /// instead of treating an unrelated or forged prefix as a lookup miss.
    fn extract_embedded_key_id(key: &str) -> Option<String> {
        let rest = key.strip_prefix("nx_")?;
        let (id_part, secret_part) = rest.split_once('_')?;
        if secret_part.is_empty() {
            return None;
        }
        uuid::Uuid::parse_str(id_part).ok()?;
        Some(id_part.to_string())
    }

    /// Verify `key` against a single already-selected candidate, updating
    /// `last_used` (in memory and, if configured, in LMDB) on success.
    fn verify_candidate(&self, key: &str, mut api_key: ApiKey) -> Result<Option<ApiKey>> {
        let password_hash = PasswordHash::new(&api_key.hashed_key)
            .map_err(|e| anyhow::anyhow!("Invalid password hash: {}", e))?;

        if self
            .argon2
            .verify_password(key.as_bytes(), &password_hash)
            .is_err()
        {
            return Ok(None);
        }

        api_key.last_used = Some(Utc::now());
        {
            let mut keys = self.api_keys.write();
            if let Some(stored) = keys.get_mut(&api_key.id) {
                stored.last_used = Some(Utc::now());
            }
        }

        if let Some(storage) = &self.storage {
            storage.update_api_key(&api_key)?;
        }

        Ok(Some(api_key))
    }

    /// Legacy path: verify `key` against every stored valid key, exactly as
    /// `verify_api_key` did before the O(1) embedded-ID lookup existed.
    /// Only reachable for tokens with no embedded key ID.
    fn verify_by_linear_scan(&self, key: &str) -> Result<Option<ApiKey>> {
        let keys: Vec<_> = {
            let keys_guard = self.api_keys.read();
            keys_guard
                .values()
                .filter(|k| k.is_valid())
                .cloned()
                .collect()
        };

        for api_key in keys {
            if let Some(verified) = self.verify_candidate(key, api_key)? {
                return Ok(Some(verified));
            }
        }

        Ok(None)
    }

    /// Check if a user has a specific permission
    pub fn has_permission(&self, api_key: &ApiKey, permission: Permission) -> bool {
        api_key.permissions.contains(&permission)
    }

    /// Get all API keys
    pub fn list_api_keys(&self) -> Vec<ApiKey> {
        // If storage is available, use it as source of truth
        if let Some(storage) = &self.storage {
            if let Ok(keys) = storage.list_api_keys() {
                // Update in-memory cache
                {
                    let mut cache = self.api_keys.write();
                    cache.clear();
                    for key in &keys {
                        cache.insert(key.id.clone(), key.clone());
                    }
                }
                return keys;
            }
        }

        // Fallback to in-memory cache
        let keys = self.api_keys.read();
        keys.values().cloned().collect()
    }

    /// Delete an API key
    pub fn delete_api_key(&self, key_id: &str) -> bool {
        // Delete from storage first if available
        if let Some(storage) = &self.storage {
            if storage.delete_api_key(key_id).unwrap_or(false) {
                // Remove from cache
                let mut keys = self.api_keys.write();
                keys.remove(key_id);
                return true;
            }
            return false;
        }

        // Fallback to in-memory only
        let mut keys = self.api_keys.write();
        keys.remove(key_id).is_some()
    }

    /// Revoke an API key
    pub fn revoke_api_key(&self, key_id: &str, reason: Option<String>) -> Result<()> {
        let mut keys = self.api_keys.write();
        if let Some(api_key) = keys.get_mut(key_id) {
            api_key.revoke(reason.clone());

            // Persist to LMDB if storage is available
            if let Some(storage) = &self.storage {
                storage.update_api_key(api_key)?;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("API key not found"))
        }
    }

    /// Get API keys for a specific user
    pub fn get_api_keys_for_user(&self, user_id: &str) -> Vec<ApiKey> {
        // If storage is available, use it
        if let Some(storage) = &self.storage {
            if let Ok(keys) = storage.get_api_keys_for_user(user_id) {
                return keys;
            }
        }

        // Fallback to in-memory cache
        let keys = self.api_keys.read();
        keys.values()
            .filter(|key| {
                key.user_id
                    .as_ref()
                    .map(|id| id == user_id)
                    .unwrap_or(false)
            })
            .cloned()
            .collect()
    }

    /// Get a specific API key by ID
    pub fn get_api_key(&self, key_id: &str) -> Option<ApiKey> {
        // If storage is available, use it
        if let Some(storage) = &self.storage {
            if let Ok(Some(key)) = storage.get_api_key(key_id) {
                // Update cache
                {
                    let mut cache = self.api_keys.write();
                    cache.insert(key.id.clone(), key.clone());
                }
                return Some(key);
            }
        }

        // Fallback to in-memory cache
        let keys = self.api_keys.read();
        keys.get(key_id).cloned()
    }

    /// Update API key permissions
    pub fn update_api_key_permissions(
        &self,
        key_id: &str,
        permissions: Vec<Permission>,
    ) -> Result<()> {
        let mut keys = self.api_keys.write();
        if let Some(api_key) = keys.get_mut(key_id) {
            api_key.permissions = permissions;

            // Persist to LMDB if storage is available
            if let Some(storage) = &self.storage {
                storage.update_api_key(api_key)?;
            }

            Ok(())
        } else {
            Err(anyhow::anyhow!("API key not found"))
        }
    }

    /// Cleanup expired API keys
    pub fn cleanup_expired_keys(&self) -> Result<usize> {
        if let Some(storage) = &self.storage {
            let count = storage.cleanup_expired_keys()?;
            // Reload keys from storage
            let keys = storage.list_api_keys()?;
            {
                let mut cache = self.api_keys.write();
                cache.clear();
                for key in keys {
                    cache.insert(key.id.clone(), key);
                }
            }
            Ok(count)
        } else {
            // Cleanup from in-memory cache
            let mut keys = self.api_keys.write();
            let expired_ids: Vec<String> = keys
                .values()
                .filter(|key| key.is_expired())
                .map(|key| key.id.clone())
                .collect();
            let count = expired_ids.len();
            for id in expired_ids {
                keys.remove(&id);
            }
            Ok(count)
        }
    }

    /// Generate a random secret for API keys
    /// Uses cryptographically secure random number generator (OsRng)
    fn generate_secret(&self) -> String {
        use argon2::password_hash::rand_core::OsRng;
        use argon2::password_hash::rand_core::RngCore;
        let mut rng = OsRng;
        let mut bytes = [0u8; 16];
        rng.fill_bytes(&mut bytes);
        hex::encode(bytes)
    }

    /// Get authentication configuration
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }

    /// Check if storage is available
    pub fn has_storage(&self) -> bool {
        self.storage.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_manager_creation() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config);
        assert!(!manager.config().enabled);
    }

    #[test]
    fn test_cleanup_expired_api_keys() {
        let config = AuthConfig::default();
        let ctx = crate::testing::TestContext::new();
        let auth_manager = AuthManager::with_storage(config, ctx.path()).unwrap();

        // Create expired key
        let (expired_key, _) = auth_manager
            .generate_api_key_for_user_with_expiration(
                "expired-key".to_string(),
                "user-123".to_string(),
                vec![Permission::Read],
                Utc::now() - chrono::Duration::days(1),
            )
            .unwrap();

        // Create valid key
        let (valid_key, _) = auth_manager
            .generate_api_key_for_user_with_expiration(
                "valid-key".to_string(),
                "user-123".to_string(),
                vec![Permission::Read],
                Utc::now() + chrono::Duration::days(1),
            )
            .unwrap();

        // Verify both exist
        assert!(auth_manager.get_api_key(&expired_key.id).is_some());
        assert!(auth_manager.get_api_key(&valid_key.id).is_some());

        // Cleanup expired keys
        let count = auth_manager.cleanup_expired_keys().unwrap();
        // At least the one we created should be cleaned up
        // (could be more if tests run in parallel with shared storage)
        assert!(
            count >= 1,
            "Expected at least 1 expired key to be cleaned up, got {}",
            count
        );

        // Expired key should be gone, valid key should remain
        assert!(auth_manager.get_api_key(&expired_key.id).is_none());
        assert!(auth_manager.get_api_key(&valid_key.id).is_some());
    }

    #[test]
    fn test_cleanup_expired_api_keys_no_storage() {
        let config = AuthConfig::default();
        let auth_manager = AuthManager::new(config);

        // Create expired key in memory
        let (expired_key, _) = auth_manager
            .generate_api_key_for_user_with_expiration(
                "expired-key".to_string(),
                "user-123".to_string(),
                vec![Permission::Read],
                Utc::now() - chrono::Duration::days(1),
            )
            .unwrap();

        // Verify key exists
        assert!(auth_manager.get_api_key(&expired_key.id).is_some());

        // Cleanup should work even without storage (cleans in-memory cache)
        let count = auth_manager.cleanup_expired_keys().unwrap();
        // Cleanup removes expired keys from in-memory cache
        assert_eq!(count, 1);

        // Expired key should be gone
        assert!(auth_manager.get_api_key(&expired_key.id).is_none());
    }

    #[test]
    fn test_api_key_generation() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, full_key) = manager
            .generate_api_key(
                "test-key".to_string(),
                vec![Permission::Read, Permission::Write],
            )
            .unwrap();

        assert_eq!(api_key.name, "test-key");
        assert!(api_key.permissions.contains(&Permission::Read));
        assert!(api_key.permissions.contains(&Permission::Write));
        assert!(api_key.is_active);
        assert!(full_key.starts_with("nx_"));
        // nx_ + 36-char UUID (key ID, for the M1 O(1) lookup) + _ + 32-char
        // hex secret.
        assert_eq!(full_key.len(), 3 + 36 + 1 + 32);
        assert!(full_key.contains(&api_key.id));
    }

    #[test]
    fn test_permission_checking() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config);

        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: None,
            permissions: vec![Permission::Read],
            hashed_key: "test".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };

        assert!(manager.has_permission(&api_key, Permission::Read));
        assert!(!manager.has_permission(&api_key, Permission::Write));
    }

    // 2.4.4 - Unit tests for revocation logic
    #[test]
    fn test_revoke_api_key() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, full_key) = manager
            .generate_api_key(
                "test-key".to_string(),
                vec![Permission::Read, Permission::Write],
            )
            .unwrap();

        // Verify key is valid before revocation
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_some());
        assert!(!verified.unwrap().is_revoked);

        // Revoke the key
        let result = manager.revoke_api_key(&api_key.id, Some("Test revocation".to_string()));
        assert!(result.is_ok());

        // Verify key is revoked
        let revoked_key = manager.get_api_key(&api_key.id);
        assert!(revoked_key.is_some());
        let revoked_key = revoked_key.unwrap();
        assert!(revoked_key.is_revoked);
        assert!(!revoked_key.is_active);
        assert_eq!(
            revoked_key.revocation_reason,
            Some("Test revocation".to_string())
        );

        // Verify revoked key cannot be used
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_none()); // Revoked keys should not verify
    }

    #[test]
    fn test_revoke_api_key_without_reason() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, _full_key) = manager
            .generate_api_key("test-key".to_string(), vec![Permission::Read])
            .unwrap();

        // Revoke without reason
        let result = manager.revoke_api_key(&api_key.id, None);
        assert!(result.is_ok());

        let revoked_key = manager.get_api_key(&api_key.id).unwrap();
        assert!(revoked_key.is_revoked);
        assert_eq!(revoked_key.revocation_reason, None);
    }

    #[test]
    fn test_revoke_nonexistent_api_key() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let result = manager.revoke_api_key("nonexistent", Some("Test".to_string()));
        assert!(result.is_err());
    }

    #[test]
    fn test_revoke_already_revoked_key() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, _) = manager
            .generate_api_key("test-key".to_string(), vec![Permission::Read])
            .unwrap();

        // Revoke first time
        let result1 = manager.revoke_api_key(&api_key.id, Some("First revocation".to_string()));
        assert!(result1.is_ok());

        // Try to revoke again
        let result2 = manager.revoke_api_key(&api_key.id, Some("Second revocation".to_string()));
        assert!(result2.is_ok()); // Should succeed, but update reason

        let revoked_key = manager.get_api_key(&api_key.id).unwrap();
        assert!(revoked_key.is_revoked);
        assert_eq!(
            revoked_key.revocation_reason,
            Some("Second revocation".to_string())
        );
    }

    #[test]
    fn test_verify_revoked_api_key() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, full_key) = manager
            .generate_api_key("test-key".to_string(), vec![Permission::Read])
            .unwrap();

        // Verify key works initially
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_some());
        assert!(verified.unwrap().is_valid());

        // Revoke the key
        manager
            .revoke_api_key(&api_key.id, Some("Revoked".to_string()))
            .unwrap();

        // Verify revoked key cannot be used
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_none()); // Revoked keys should not verify
    }

    #[test]
    fn test_list_revoked_api_keys() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (key1, _) = manager
            .generate_api_key("key1".to_string(), vec![Permission::Read])
            .unwrap();
        let (key2, _) = manager
            .generate_api_key("key2".to_string(), vec![Permission::Write])
            .unwrap();
        let (key3, _) = manager
            .generate_api_key("key3".to_string(), vec![Permission::Admin])
            .unwrap();

        // Revoke key2
        manager
            .revoke_api_key(&key2.id, Some("Revoked".to_string()))
            .unwrap();

        // List all keys
        let all_keys = manager.list_api_keys();
        assert_eq!(all_keys.len(), 3);

        // Check that key2 is revoked
        let revoked_key = all_keys.iter().find(|k| k.id == key2.id).unwrap();
        assert!(revoked_key.is_revoked);
        assert!(!revoked_key.is_active);

        // Check that other keys are not revoked
        let active_key1 = all_keys.iter().find(|k| k.id == key1.id).unwrap();
        let active_key3 = all_keys.iter().find(|k| k.id == key3.id).unwrap();
        assert!(!active_key1.is_revoked);
        assert!(!active_key3.is_revoked);
        assert!(active_key1.is_active);
        assert!(active_key3.is_active);
    }

    #[test]
    fn test_revocation_reason_persistence() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (api_key, _) = manager
            .generate_api_key("test-key".to_string(), vec![Permission::Read])
            .unwrap();

        let reason = "Security breach detected".to_string();
        manager
            .revoke_api_key(&api_key.id, Some(reason.clone()))
            .unwrap();

        let revoked_key = manager.get_api_key(&api_key.id).unwrap();
        assert_eq!(revoked_key.revocation_reason, Some(reason));
    }

    #[test]
    fn test_generate_api_key_for_user_with_expiration() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let user_id = "user123".to_string();
        let expires_at = Utc::now() + chrono::Duration::days(30);

        let (api_key, full_key) = manager
            .generate_api_key_for_user_with_expiration(
                "test-key".to_string(),
                user_id.clone(),
                vec![Permission::Read, Permission::Write],
                expires_at,
            )
            .unwrap();

        // Verify user_id is set
        assert_eq!(api_key.user_id, Some(user_id));

        // Verify expiration is set
        assert!(api_key.expires_at.is_some());

        // Verify key can be used
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_some());
        assert!(verified.unwrap().is_valid());

        // Verify permissions
        assert!(manager.has_permission(&api_key, Permission::Read));
        assert!(manager.has_permission(&api_key, Permission::Write));
    }

    #[test]
    fn test_generate_api_key_for_user_with_expiration_expired() {
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let user_id = "user123".to_string();
        let expires_at = Utc::now() - chrono::Duration::days(1); // Already expired

        let (api_key, full_key) = manager
            .generate_api_key_for_user_with_expiration(
                "expired-key".to_string(),
                user_id,
                vec![Permission::Read],
                expires_at,
            )
            .unwrap();

        // Verify expiration is set
        assert!(api_key.expires_at.is_some());
        assert!(api_key.is_expired());

        // Verify expired key cannot be used
        let verified = manager.verify_api_key(&full_key).unwrap();
        assert!(verified.is_none()); // Expired keys should not verify
    }

    // -- M1: O(1) API key verification ------------------------------------

    #[test]
    fn test_extract_embedded_key_id() {
        let id = uuid::Uuid::new_v4().to_string();
        let key = format!("nx_{id}_somesecrethex");
        assert_eq!(AuthManager::extract_embedded_key_id(&key), Some(id));

        // Legacy shape: no `_` separator after the prefix at all.
        assert_eq!(
            AuthManager::extract_embedded_key_id("nx_deadbeefdeadbeef"),
            None
        );

        // Forged/garbage shape: has a separator, but the id segment is not
        // a valid UUID — must NOT be treated as a lookup miss that falls
        // back to the expensive linear scan.
        assert_eq!(
            AuthManager::extract_embedded_key_id("nx_notauuid_secret"),
            None
        );
    }

    #[test]
    fn test_verify_api_key_legacy_format_without_embedded_id_still_verifies() {
        // Simulates a key minted before the O(1) embedded-ID lookup
        // existed: `nx_{secret}` with no key-ID segment. Must still verify
        // via the linear-scan fallback so already-issued keys keep working.
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let legacy_full_key = "nx_deadbeefdeadbeefdeadbeefdeadbeef".to_string();
        let salt = SaltString::generate(&mut OsRng);
        let hashed_key = manager
            .argon2
            .hash_password(legacy_full_key.as_bytes(), &salt)
            .unwrap()
            .to_string();

        let legacy_key = ApiKey {
            id: uuid::Uuid::new_v4().to_string(),
            name: "legacy".to_string(),
            user_id: None,
            permissions: vec![Permission::Read],
            hashed_key,
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        };
        manager
            .api_keys
            .write()
            .insert(legacy_key.id.clone(), legacy_key.clone());

        let verified = manager.verify_api_key(&legacy_full_key).unwrap();
        assert!(verified.is_some());
        assert_eq!(verified.unwrap().id, legacy_key.id);

        // A wrong secret against the same legacy shape must still fail.
        assert!(
            manager
                .verify_api_key("nx_wrongwrongwrongwrongwrongwrongwrong")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn test_verify_api_key_forged_embedded_id_does_not_fall_back_to_scan() {
        // A token shaped like the current format (`nx_{uuid}_{secret}`)
        // but whose UUID does not match any stored key must fail outright
        // — it must NOT fall back to the O(N) linear scan, or a forged
        // token would still cost as much as the pre-fix bug.
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);
        let (_key, _full_key) = manager
            .generate_api_key("real-key".to_string(), vec![Permission::Read])
            .unwrap();

        let forged = format!("nx_{}_forgedsecret", uuid::Uuid::new_v4());
        assert!(manager.verify_api_key(&forged).unwrap().is_none());
    }

    #[test]
    fn test_verify_api_key_cost_is_constant_in_key_count() {
        // M1 regression: verifying a real key must cost about the same
        // whether it is the only stored key or one of hundreds — not O(N)
        // Argon2 verifications.
        let config = AuthConfig {
            enabled: true,
            ..Default::default()
        };
        let manager = AuthManager::new(config);

        let (_baseline_key, baseline_full_key) = manager
            .generate_api_key("baseline".to_string(), vec![Permission::Read])
            .unwrap();
        let start = std::time::Instant::now();
        assert!(
            manager
                .verify_api_key(&baseline_full_key)
                .unwrap()
                .is_some()
        );
        let baseline = start.elapsed();

        for i in 0..200 {
            manager
                .generate_api_key(format!("decoy-{i}"), vec![Permission::Read])
                .unwrap();
        }
        // Created after the decoys so it can't benefit from being first in
        // any incidental map iteration order.
        let (_target_key, target_full_key) = manager
            .generate_api_key("target".to_string(), vec![Permission::Read])
            .unwrap();

        let start = std::time::Instant::now();
        let verified = manager.verify_api_key(&target_full_key).unwrap();
        let with_many_keys = start.elapsed();

        assert!(verified.is_some());
        // Generous tolerance (10x the single-key baseline, floor 200ms) to
        // avoid CI flakiness while still failing hard against genuine O(N)
        // behavior, which would cost roughly 200x the baseline here.
        let tolerance = std::cmp::max(baseline * 10, std::time::Duration::from_millis(200));
        assert!(
            with_many_keys <= tolerance,
            "verify_api_key took {with_many_keys:?} with 200+ stored keys vs {baseline:?} \
             baseline (tolerance {tolerance:?}) — looks O(N), not O(1)"
        );
    }
}
