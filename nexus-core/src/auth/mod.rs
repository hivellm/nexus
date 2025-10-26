//! Authentication and authorization system for Nexus
//!
//! This module provides API key management, authentication middleware,
//! and role-based access control (RBAC) for the Nexus graph database.

pub mod api_key;
pub mod middleware;
pub mod permissions;
pub mod rbac;

pub use api_key::ApiKey;
pub use middleware::{AuthMiddleware, AuthContext, AuthError};
pub use permissions::{Permission, PermissionSet};
pub use rbac::{RoleBasedAccessControl, Role, User};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use chrono::Utc;

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
    ) -> Result<ApiKey> {
        let key_id = uuid::Uuid::new_v4().to_string();
        let key_secret = self.generate_secret();
        let full_key = format!("nexus_sk_{}_{}", key_id, key_secret);
        
        // Hash the full key for storage
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = self.argon2
            .hash_password(full_key.as_bytes(), &salt)
            .map_err(|e| anyhow::anyhow!("Failed to hash API key: {}", e))?;

        let api_key = ApiKey {
            id: key_id,
            name,
            permissions,
            hashed_key: password_hash.to_string(),
            created_at: Utc::now(),
            last_used: None,
            is_active: true,
        };

        // Store the API key
        {
            let mut keys = self.api_keys.write();
            keys.insert(api_key.id.clone(), api_key.clone());
        }

        Ok(api_key)
    }

    /// Verify an API key
    pub fn verify_api_key(&self, key: &str) -> Result<Option<ApiKey>> {
        if !self.config.enabled {
            return Ok(None);
        }

        // Extract key ID from the full key
        let key_id = if let Some(stripped) = key.strip_prefix("nexus_sk_") {
            if let Some(underscore_pos) = stripped.find('_') {
                &stripped[..underscore_pos]
            } else {
                return Ok(None);
            }
        } else {
            return Ok(None);
        };

        // Look up the API key
        let api_key = {
            let keys = self.api_keys.read();
            keys.get(key_id).cloned()
        };

        if let Some(api_key) = api_key {
            if !api_key.is_active {
                return Ok(None);
            }

            // Verify the key
            let password_hash = PasswordHash::new(&api_key.hashed_key)
                .map_err(|e| anyhow::anyhow!("Invalid password hash: {}", e))?;

            if self.argon2.verify_password(key.as_bytes(), &password_hash).is_ok() {
                // Update last used timestamp
                {
                    let mut keys = self.api_keys.write();
                    if let Some(key) = keys.get_mut(key_id) {
                        key.last_used = Some(Utc::now());
                    }
                }
                Ok(Some(api_key))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
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
        
        let api_key = manager.generate_api_key(
            "test-key".to_string(),
            vec![Permission::Read, Permission::Write],
        ).unwrap();
        
        assert_eq!(api_key.name, "test-key");
        assert!(api_key.permissions.contains(&Permission::Read));
        assert!(api_key.permissions.contains(&Permission::Write));
        assert!(api_key.is_active);
    }

    #[test]
    fn test_permission_checking() {
        let config = AuthConfig::default();
        let manager = AuthManager::new(config);
        
        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            permissions: vec![Permission::Read],
            hashed_key: "test".to_string(),
            created_at: Utc::now(),
            last_used: None,
            is_active: true,
        };
        
        assert!(manager.has_permission(&api_key, Permission::Read));
        assert!(!manager.has_permission(&api_key, Permission::Write));
    }
}
