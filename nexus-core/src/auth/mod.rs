//! Authentication and authorization system for Nexus
//!
//! This module provides API key management, authentication middleware,
//! and role-based access control (RBAC) for the Nexus graph database.

pub mod api_key;
pub mod middleware;
pub mod permissions;
pub mod rbac;

pub use api_key::ApiKey;
pub use middleware::{AuthContext, AuthError, AuthMiddleware};
pub use permissions::{Permission, PermissionSet};
pub use rbac::{Role, RoleBasedAccessControl, User};

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
    argon2: Argon2<'static>,
}

impl AuthManager {
    /// Create a new authentication manager
    pub fn new(config: AuthConfig) -> Self {
        Self {
            config,
            api_keys: Arc::new(RwLock::new(HashMap::new())),
            argon2: Argon2::default(),
        }
    }

    /// Generate a new API key
    pub fn generate_api_key(
        &self,
        name: String,
        permissions: Vec<Permission>,
    ) -> Result<(ApiKey, String)> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        let full_key = format!("nx_{}", key_secret);

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
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
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
        let full_key = format!("nx_{}", key_secret);

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
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
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
        let full_key = format!("nx_{}", key_secret);

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
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        Ok((api_key, full_key))
    }

    /// Verify an API key
    pub fn verify_api_key(&self, key: &str) -> Result<Option<ApiKey>> {
        if !self.config.enabled {
            return Ok(None);
        }

        // Check if key starts with nx_
        if !key.starts_with("nx_") {
            return Ok(None);
        }

        // Try to find the key by verifying against all stored keys
        // This is less efficient but necessary since we hash the full key
        let keys = {
            let keys_guard = self.api_keys.read();
            keys_guard.values().cloned().collect::<Vec<_>>()
        };

        for mut api_key in keys {
            // Check if key is valid (active, not revoked, not expired)
            if !api_key.is_valid() {
                continue;
            }

            // Verify the key
            let password_hash = PasswordHash::new(&api_key.hashed_key)
                .map_err(|e| anyhow::anyhow!("Invalid password hash: {}", e))?;

            if self
                .argon2
                .verify_password(key.as_bytes(), &password_hash)
                .is_ok()
            {
                // Update last used timestamp
                {
                    let mut keys = self.api_keys.write();
                    if let Some(key) = keys.get_mut(&api_key.id) {
                        key.last_used = Some(Utc::now());
                    }
                }
                api_key.last_used = Some(Utc::now());
                return Ok(Some(api_key));
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
        let keys = self.api_keys.read();
        keys.values().cloned().collect()
    }

    /// Delete an API key
    pub fn delete_api_key(&self, key_id: &str) -> bool {
        let mut keys = self.api_keys.write();
        keys.remove(key_id).is_some()
    }

    /// Revoke an API key
    pub fn revoke_api_key(&self, key_id: &str, reason: Option<String>) -> Result<()> {
        let mut keys = self.api_keys.write();
        if let Some(api_key) = keys.get_mut(key_id) {
            api_key.revoke(reason);
            Ok(())
        } else {
            Err(anyhow::anyhow!("API key not found"))
        }
    }

    /// Get API keys for a specific user
    pub fn get_api_keys_for_user(&self, user_id: &str) -> Vec<ApiKey> {
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
            Ok(())
        } else {
            Err(anyhow::anyhow!("API key not found"))
        }
    }

    /// Generate a random secret for API keys
    fn generate_secret(&self) -> String {
        use rand::Rng;
        const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
        let mut rng = rand::thread_rng();

        (0..32)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect()
    }

    /// Get authentication configuration
    pub fn config(&self) -> &AuthConfig {
        &self.config
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
        assert_eq!(full_key.len(), 35); // nx_ + 32 chars
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
        };

        assert!(manager.has_permission(&api_key, Permission::Read));
        assert!(!manager.has_permission(&api_key, Permission::Write));
    }
}
