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
    /// Permissions granted to this API key
    pub permissions: Vec<Permission>,
    /// Hashed version of the full API key (for storage)
    pub hashed_key: String,
    /// When the API key was created
    pub created_at: DateTime<Utc>,
    /// When the API key was last used (None if never used)
    pub last_used: Option<DateTime<Utc>>,
    /// Whether the API key is active
    pub is_active: bool,
}

impl ApiKey {
    /// Create a new API key
    pub fn new(id: String, name: String, permissions: Vec<Permission>, hashed_key: String) -> Self {
        Self {
            id,
            name,
            permissions,
            hashed_key,
            created_at: Utc::now(),
            last_used: None,
            is_active: true,
        }
    }

    /// Check if the API key has expired
    pub fn is_expired(&self, max_age_days: Option<u32>) -> bool {
        if let Some(max_age) = max_age_days {
            let max_age_duration = chrono::Duration::days(max_age as i64);
            Utc::now() - self.created_at > max_age_duration
        } else {
            false
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

        // Set created_at to 10 days ago
        api_key.created_at = Utc::now() - chrono::Duration::days(10);

        assert!(api_key.is_expired(Some(7))); // Expired after 7 days
        assert!(!api_key.is_expired(Some(15))); // Not expired after 15 days
        assert!(!api_key.is_expired(None)); // No expiration
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
}
