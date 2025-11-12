//! Queue Permission Helpers
//!
//! Helper functions for checking queue-related permissions.
//! These functions can be used when queue operations are implemented.

use super::{ApiKey, Permission};

/// Queue operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueOperation {
    /// Consume messages from a queue
    Consume,
    /// Publish messages to a queue
    Publish,
    /// Manage queues (create, delete, configure)
    Manage,
}

impl QueueOperation {
    /// Get the required permission for this operation
    pub fn required_permission(&self) -> Permission {
        match self {
            QueueOperation::Consume => Permission::Queue, // QUEUE:READ equivalent
            QueueOperation::Publish => Permission::Queue, // QUEUE:WRITE equivalent
            QueueOperation::Manage => Permission::Admin,  // QUEUE:ADMIN equivalent
        }
    }
}

/// Check if an API key has permission for a queue operation
pub fn check_queue_permission(api_key: &ApiKey, operation: QueueOperation) -> bool {
    // Admin and Super have all permissions
    if api_key.permissions.contains(&Permission::Admin)
        || api_key.permissions.contains(&Permission::Super)
    {
        return true;
    }

    // Check for specific queue permission
    match operation {
        QueueOperation::Consume | QueueOperation::Publish => {
            api_key.permissions.contains(&Permission::Queue)
        }
        QueueOperation::Manage => {
            // Management requires Admin permission
            api_key.permissions.contains(&Permission::Admin)
        }
    }
}

/// Check if an API key can consume from queues
pub fn can_consume_queue(api_key: &ApiKey) -> bool {
    check_queue_permission(api_key, QueueOperation::Consume)
}

/// Check if an API key can publish to queues
pub fn can_publish_queue(api_key: &ApiKey) -> bool {
    check_queue_permission(api_key, QueueOperation::Publish)
}

/// Check if an API key can manage queues
pub fn can_manage_queue(api_key: &ApiKey) -> bool {
    check_queue_permission(api_key, QueueOperation::Manage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_operation_permissions() {
        assert_eq!(
            QueueOperation::Consume.required_permission(),
            Permission::Queue
        );
        assert_eq!(
            QueueOperation::Publish.required_permission(),
            Permission::Queue
        );
        assert_eq!(
            QueueOperation::Manage.required_permission(),
            Permission::Admin
        );
    }

    #[test]
    fn test_check_queue_permission_with_queue_permission() {
        use super::super::api_key::ApiKey;
        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Queue],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(check_queue_permission(&api_key, QueueOperation::Consume));
        assert!(check_queue_permission(&api_key, QueueOperation::Publish));
        assert!(!check_queue_permission(&api_key, QueueOperation::Manage));
    }

    #[test]
    fn test_check_queue_permission_with_admin() {
        use super::super::api_key::ApiKey;
        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Admin],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(check_queue_permission(&api_key, QueueOperation::Consume));
        assert!(check_queue_permission(&api_key, QueueOperation::Publish));
        assert!(check_queue_permission(&api_key, QueueOperation::Manage));
    }

    #[test]
    fn test_check_queue_permission_without_permission() {
        use super::super::api_key::ApiKey;
        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Read],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(!check_queue_permission(&api_key, QueueOperation::Consume));
        assert!(!check_queue_permission(&api_key, QueueOperation::Publish));
        assert!(!check_queue_permission(&api_key, QueueOperation::Manage));
    }

    #[test]
    fn test_can_consume_queue() {
        use super::super::api_key::ApiKey;
        let api_key_with_queue = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Queue],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        let api_key_without_queue = ApiKey {
            id: "test2".to_string(),
            name: "test2".to_string(),
            user_id: Some("user2".to_string()),
            hashed_key: "hash2".to_string(),
            permissions: vec![Permission::Read],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(can_consume_queue(&api_key_with_queue));
        assert!(!can_consume_queue(&api_key_without_queue));
    }

    #[test]
    fn test_can_publish_queue() {
        use super::super::api_key::ApiKey;
        let api_key_with_queue = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Queue],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        let api_key_without_queue = ApiKey {
            id: "test2".to_string(),
            name: "test2".to_string(),
            user_id: Some("user2".to_string()),
            hashed_key: "hash2".to_string(),
            permissions: vec![Permission::Read],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(can_publish_queue(&api_key_with_queue));
        assert!(!can_publish_queue(&api_key_without_queue));
    }

    #[test]
    fn test_can_manage_queue() {
        use super::super::api_key::ApiKey;
        let api_key_with_admin = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Admin],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        let api_key_with_queue = ApiKey {
            id: "test2".to_string(),
            name: "test2".to_string(),
            user_id: Some("user2".to_string()),
            hashed_key: "hash2".to_string(),
            permissions: vec![Permission::Queue],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(can_manage_queue(&api_key_with_admin));
        assert!(!can_manage_queue(&api_key_with_queue));
    }
}
