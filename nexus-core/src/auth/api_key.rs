//! API Key management for Nexus authentication

use super::permissions::Permission;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// API Key for authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    /// Unique identifier for the API key
    pub id: String,
    /// Human-readable name for the API key
    pub name: String,
    /// User ID this key belongs to (optional)
    pub user_id: Option<String>,
    /// Permissions granted to this API key
    pub permissions: Vec<Permission>,
    /// Hashed version of the full API key (for storage)
    pub hashed_key: String,
    /// When the API key was created
    pub created_at: DateTime<Utc>,
    /// When the API key expires (None if permanent)
    pub expires_at: Option<DateTime<Utc>>,
    /// When the API key was last used (None if never used)
    pub last_used: Option<DateTime<Utc>>,
    /// Whether the API key is active
    pub is_active: bool,
    /// Whether the API key has been revoked
    pub is_revoked: bool,
    /// Reason for revocation (if revoked)
    pub revocation_reason: Option<String>,
    /// Optional allow-list of MCP / RPC function names this key may
    /// invoke. `None` means "all functions the key's permissions
    /// permit" (the pre-cluster-mode default). `Some(list)` restricts
    /// the key to exactly those names — useful in cluster mode where
    /// a per-tenant key should expose `cypher.execute` but not, say,
    /// `nexus.admin.drop_database`.
    ///
    /// `#[serde(default)]` keeps LMDB-persisted keys from before this
    /// field existed deserialising cleanly as `None`.
    #[serde(default)]
    pub allowed_functions: Option<Vec<String>>,
}

impl ApiKey {
    /// Create a new API key
    pub fn new(id: String, name: String, permissions: Vec<Permission>, hashed_key: String) -> Self {
        Self {
            id,
            name,
            user_id: None,
            permissions,
            hashed_key,
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        }
    }

    /// Create a new API key with user ID
    pub fn with_user_id(
        id: String,
        name: String,
        user_id: String,
        permissions: Vec<Permission>,
        hashed_key: String,
    ) -> Self {
        Self {
            id,
            name,
            user_id: Some(user_id),
            permissions,
            hashed_key,
            created_at: Utc::now(),
            expires_at: None,
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        }
    }

    /// Create a new API key with expiration
    pub fn with_expiration(
        id: String,
        name: String,
        permissions: Vec<Permission>,
        hashed_key: String,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            name,
            user_id: None,
            permissions,
            hashed_key,
            created_at: Utc::now(),
            expires_at: Some(expires_at),
            last_used: None,
            is_active: true,
            is_revoked: false,
            revocation_reason: None,
            allowed_functions: None,
        }
    }

    /// Replace the function allow-list and return `self` for
    /// chaining. Pass `None` for unrestricted access or `Some(vec)`
    /// to restrict the key to exactly those function names. Used by
    /// cluster-mode key provisioning to lock down per-tenant keys.
    pub fn with_allowed_functions(mut self, allowed: Option<Vec<String>>) -> Self {
        self.allowed_functions = allowed;
        self
    }

    /// Whether this key is permitted to invoke the function named
    /// `name`. Unrestricted keys (`allowed_functions = None`) accept
    /// everything; restricted keys require an exact, case-sensitive
    /// match against the allow-list. Function-name canonicalisation
    /// (casing, prefixing) is the caller's responsibility — this
    /// method does NOT massage the input.
    pub fn may_call_function(&self, name: &str) -> bool {
        match &self.allowed_functions {
            None => true,
            Some(list) => list.iter().any(|f| f == name),
        }
    }

    /// Check if the API key has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now() > expires_at
        } else {
            false
        }
    }

    /// Check if the API key has expired (legacy method for backward compatibility)
    pub fn is_expired_legacy(&self, max_age_days: Option<u32>) -> bool {
        if let Some(max_age) = max_age_days {
            let max_age_duration = chrono::Duration::days(max_age as i64);
            Utc::now() - self.created_at > max_age_duration
        } else {
            self.is_expired()
        }
    }

    /// Check if the API key has been inactive for too long
    pub fn is_inactive(&self, max_inactive_days: Option<u32>) -> bool {
        if let Some(max_inactive) = max_inactive_days {
            if let Some(last_used) = self.last_used {
                let max_inactive_duration = chrono::Duration::days(max_inactive as i64);
                Utc::now() - last_used > max_inactive_duration
            } else {
                // Never used, check creation date
                let max_inactive_duration = chrono::Duration::days(max_inactive as i64);
                Utc::now() - self.created_at > max_inactive_duration
            }
        } else {
            false
        }
    }

    /// Deactivate the API key
    pub fn deactivate(&mut self) {
        self.is_active = false;
    }

    /// Activate the API key
    pub fn activate(&mut self) {
        self.is_active = true;
    }

    /// Update the last used timestamp
    pub fn mark_used(&mut self) {
        self.last_used = Some(Utc::now());
    }

    /// Revoke the API key
    pub fn revoke(&mut self, reason: Option<String>) {
        self.is_revoked = true;
        self.is_active = false;
        self.revocation_reason = reason;
    }

    /// Check if the API key is valid (active, not revoked, not expired)
    pub fn is_valid(&self) -> bool {
        self.is_active && !self.is_revoked && !self.is_expired()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_creation() {
        let api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        assert_eq!(api_key.id, "test-id");
        assert_eq!(api_key.name, "test-key");
        assert!(api_key.permissions.contains(&Permission::Read));
        assert!(api_key.is_active);
        assert!(!api_key.is_revoked);
        assert!(api_key.user_id.is_none());
        assert!(api_key.expires_at.is_none());
        assert!(api_key.last_used.is_none());
    }

    #[test]
    fn test_api_key_expiration() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        // Test with expires_at
        api_key.expires_at = Some(Utc::now() - chrono::Duration::days(1));
        assert!(api_key.is_expired());

        api_key.expires_at = Some(Utc::now() + chrono::Duration::days(1));
        assert!(!api_key.is_expired());

        api_key.expires_at = None;
        assert!(!api_key.is_expired());

        // Test legacy method
        api_key.created_at = Utc::now() - chrono::Duration::days(10);
        assert!(api_key.is_expired_legacy(Some(7))); // Expired after 7 days
        assert!(!api_key.is_expired_legacy(Some(15))); // Not expired after 15 days
        assert!(!api_key.is_expired_legacy(None)); // No expiration
    }

    #[test]
    fn test_api_key_inactivity() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        // Set last_used to 10 days ago
        api_key.last_used = Some(Utc::now() - chrono::Duration::days(10));

        assert!(api_key.is_inactive(Some(7))); // Inactive after 7 days
        assert!(!api_key.is_inactive(Some(15))); // Not inactive after 15 days
        assert!(!api_key.is_inactive(None)); // No inactivity limit
    }

    #[test]
    fn test_api_key_activation() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        assert!(api_key.is_active);

        api_key.deactivate();
        assert!(!api_key.is_active);

        api_key.activate();
        assert!(api_key.is_active);
    }

    #[test]
    fn test_api_key_usage_tracking() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        assert!(api_key.last_used.is_none());

        api_key.mark_used();
        assert!(api_key.last_used.is_some());
    }

    #[test]
    fn test_api_key_revocation() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        assert!(api_key.is_active);
        assert!(!api_key.is_revoked);

        api_key.revoke(Some("Test revocation".to_string()));
        assert!(!api_key.is_active);
        assert!(api_key.is_revoked);
        assert_eq!(
            api_key.revocation_reason,
            Some("Test revocation".to_string())
        );
    }

    #[test]
    fn test_api_key_validity() {
        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );

        assert!(api_key.is_valid());

        // Expired key
        api_key.expires_at = Some(Utc::now() - chrono::Duration::days(1));
        assert!(!api_key.is_valid());

        // Reset and test revoked
        api_key.expires_at = None;
        api_key.revoke(None);
        assert!(!api_key.is_valid());

        // Reset and test inactive
        api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![Permission::Read],
            "hashed_key".to_string(),
        );
        api_key.deactivate();
        assert!(!api_key.is_valid());
    }

    #[test]
    fn api_key_default_allows_all_functions() {
        // Pre-cluster-mode behaviour: a key with no explicit
        // allow-list lets every function through. Locked in as a
        // regression guard — flipping the default silently would
        // break every existing key in storage.
        let key = ApiKey::new(
            "id".into(),
            "name".into(),
            vec![Permission::Read],
            "hash".into(),
        );
        assert!(key.allowed_functions.is_none());
        assert!(key.may_call_function("anything"));
        assert!(key.may_call_function("nexus.admin.drop_database"));
    }

    #[test]
    fn api_key_restricted_function_list() {
        let key = ApiKey::new(
            "id".into(),
            "name".into(),
            vec![Permission::Read],
            "hash".into(),
        )
        .with_allowed_functions(Some(vec!["cypher.execute".into(), "kv.get".into()]));

        assert!(key.may_call_function("cypher.execute"));
        assert!(key.may_call_function("kv.get"));
        assert!(!key.may_call_function("nexus.admin.drop_database"));
        // Case-sensitive matching — canonicalisation is the caller's
        // job, otherwise `"KV.GET"` would silently bypass the filter.
        assert!(!key.may_call_function("Cypher.Execute"));
    }

    #[test]
    fn api_key_empty_allow_list_denies_everything() {
        // Empty Some(vec![]) is deliberately distinct from None:
        // "explicitly zero functions" for health-probe-only keys.
        let key = ApiKey::new(
            "id".into(),
            "name".into(),
            vec![Permission::Read],
            "hash".into(),
        )
        .with_allowed_functions(Some(vec![]));
        assert!(!key.may_call_function("cypher.execute"));
    }

    #[test]
    fn api_key_allowed_functions_round_trip_serde() {
        // Legacy keys in LMDB predate this field — `#[serde(default)]`
        // must let them deserialise as `None` without migration.
        let legacy = r#"{
            "id": "legacy",
            "name": "old-key",
            "user_id": null,
            "permissions": ["Read"],
            "hashed_key": "h",
            "created_at": "2020-01-01T00:00:00Z",
            "expires_at": null,
            "last_used": null,
            "is_active": true,
            "is_revoked": false,
            "revocation_reason": null
        }"#;
        let key: ApiKey = serde_json::from_str(legacy).expect("legacy key must parse");
        assert!(key.allowed_functions.is_none());
        assert!(key.may_call_function("anything"));
    }
}
