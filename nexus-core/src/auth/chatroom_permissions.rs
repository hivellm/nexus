//! Chatroom Permission Helpers
//!
//! Helper functions for checking chatroom-related permissions.
//! These functions can be used when chatroom operations are implemented.

use super::{ApiKey, Permission};

/// Chatroom operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatroomOperation {
    /// Read messages from a chatroom
    Read,
    /// Send messages to a chatroom
    Send,
    /// Manage chatrooms (create, delete, configure)
    Manage,
}

impl ChatroomOperation {
    /// Get the required permission for this operation
    pub fn required_permission(&self) -> Permission {
        match self {
            ChatroomOperation::Read => Permission::Chatroom, // CHATROOM:READ equivalent
            ChatroomOperation::Send => Permission::Chatroom, // CHATROOM:WRITE equivalent
            ChatroomOperation::Manage => Permission::Admin,  // CHATROOM:ADMIN equivalent
        }
    }
}

/// Check if an API key has permission for a chatroom operation
pub fn check_chatroom_permission(api_key: &ApiKey, operation: ChatroomOperation) -> bool {
    // Admin and Super have all permissions
    if api_key.permissions.contains(&Permission::Admin)
        || api_key.permissions.contains(&Permission::Super)
    {
        return true;
    }

    // Check for specific chatroom permission
    match operation {
        ChatroomOperation::Read | ChatroomOperation::Send => {
            api_key.permissions.contains(&Permission::Chatroom)
        }
        ChatroomOperation::Manage => {
            // Management requires Admin permission
            api_key.permissions.contains(&Permission::Admin)
        }
    }
}

/// Check if an API key can read from chatrooms
pub fn can_read_chatroom(api_key: &ApiKey) -> bool {
    check_chatroom_permission(api_key, ChatroomOperation::Read)
}

/// Check if an API key can send messages to chatrooms
pub fn can_send_chatroom(api_key: &ApiKey) -> bool {
    check_chatroom_permission(api_key, ChatroomOperation::Send)
}

/// Check if an API key can manage chatrooms
pub fn can_manage_chatroom(api_key: &ApiKey) -> bool {
    check_chatroom_permission(api_key, ChatroomOperation::Manage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chatroom_operation_permissions() {
        assert_eq!(
            ChatroomOperation::Read.required_permission(),
            Permission::Chatroom
        );
        assert_eq!(
            ChatroomOperation::Send.required_permission(),
            Permission::Chatroom
        );
        assert_eq!(
            ChatroomOperation::Manage.required_permission(),
            Permission::Admin
        );
    }

    #[test]
    fn test_check_chatroom_permission_with_chatroom_permission() {
        use super::super::api_key::ApiKey;
        let api_key = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Chatroom],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(check_chatroom_permission(&api_key, ChatroomOperation::Read));
        assert!(check_chatroom_permission(&api_key, ChatroomOperation::Send));
        assert!(!check_chatroom_permission(
            &api_key,
            ChatroomOperation::Manage
        ));
    }

    #[test]
    fn test_check_chatroom_permission_with_admin() {
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

        assert!(check_chatroom_permission(&api_key, ChatroomOperation::Read));
        assert!(check_chatroom_permission(&api_key, ChatroomOperation::Send));
        assert!(check_chatroom_permission(
            &api_key,
            ChatroomOperation::Manage
        ));
    }

    #[test]
    fn test_check_chatroom_permission_without_permission() {
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

        assert!(!check_chatroom_permission(
            &api_key,
            ChatroomOperation::Read
        ));
        assert!(!check_chatroom_permission(
            &api_key,
            ChatroomOperation::Send
        ));
        assert!(!check_chatroom_permission(
            &api_key,
            ChatroomOperation::Manage
        ));
    }

    #[test]
    fn test_can_read_chatroom() {
        use super::super::api_key::ApiKey;
        let api_key_with_chatroom = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Chatroom],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        let api_key_without_chatroom = ApiKey {
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

        assert!(can_read_chatroom(&api_key_with_chatroom));
        assert!(!can_read_chatroom(&api_key_without_chatroom));
    }

    #[test]
    fn test_can_send_chatroom() {
        use super::super::api_key::ApiKey;
        let api_key_with_chatroom = ApiKey {
            id: "test".to_string(),
            name: "test".to_string(),
            user_id: Some("user1".to_string()),
            hashed_key: "hash".to_string(),
            permissions: vec![Permission::Chatroom],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        let api_key_without_chatroom = ApiKey {
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

        assert!(can_send_chatroom(&api_key_with_chatroom));
        assert!(!can_send_chatroom(&api_key_without_chatroom));
    }

    #[test]
    fn test_can_manage_chatroom() {
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

        let api_key_with_chatroom = ApiKey {
            id: "test2".to_string(),
            name: "test2".to_string(),
            user_id: Some("user2".to_string()),
            hashed_key: "hash2".to_string(),
            permissions: vec![Permission::Chatroom],
            created_at: chrono::Utc::now(),
            last_used: None,
            expires_at: None,
            is_revoked: false,
            revocation_reason: None,
            is_active: true,
        };

        assert!(can_manage_chatroom(&api_key_with_admin));
        assert!(!can_manage_chatroom(&api_key_with_chatroom));
    }
}
