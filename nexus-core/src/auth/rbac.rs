//! Role-Based Access Control (RBAC) for Nexus

use super::permissions::{Permission, PermissionSet};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A role in the RBAC system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Role {
    /// Unique identifier for the role
    pub id: String,
    /// Human-readable name for the role
    pub name: String,
    /// Description of the role
    pub description: Option<String>,
    /// Permissions granted to this role
    pub permissions: PermissionSet,
}

impl Role {
    /// Create a new role
    pub fn new(id: String, name: String, permissions: PermissionSet) -> Self {
        Self {
            id,
            name,
            description: None,
            permissions,
        }
    }

    /// Create a new role with description
    pub fn with_description(
        id: String,
        name: String,
        description: String,
        permissions: PermissionSet,
    ) -> Self {
        Self {
            id,
            name,
            description: Some(description),
            permissions,
        }
    }

    /// Check if the role has a specific permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions.has_permission(permission)
    }

    /// Add a permission to the role
    pub fn add_permission(&mut self, permission: Permission) {
        self.permissions.add(permission);
    }

    /// Remove a permission from the role
    pub fn remove_permission(&mut self, permission: &Permission) {
        self.permissions.remove(permission);
    }
}

/// A user in the RBAC system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    /// Unique identifier for the user
    pub id: String,
    /// Username
    pub username: String,
    /// Email address
    pub email: Option<String>,
    /// Password hash (Argon2)
    pub password_hash: Option<String>,
    /// Roles assigned to this user
    pub roles: Vec<String>,
    /// Additional permissions (beyond roles)
    pub additional_permissions: PermissionSet,
    /// Whether the user is active
    pub is_active: bool,
    /// Whether this is the root user (cannot be deleted, only disabled)
    pub is_root: bool,
}

impl User {
    /// Create a new user
    pub fn new(id: String, username: String) -> Self {
        Self {
            id,
            username,
            email: None,
            password_hash: None,
            roles: Vec::new(),
            additional_permissions: PermissionSet::new(),
            is_active: true,
            is_root: false,
        }
    }

    /// Create a new user with email
    pub fn with_email(id: String, username: String, email: String) -> Self {
        Self {
            id,
            username,
            email: Some(email),
            password_hash: None,
            roles: Vec::new(),
            additional_permissions: PermissionSet::new(),
            is_active: true,
            is_root: false,
        }
    }

    /// Create a new user with password hash
    pub fn with_password_hash(id: String, username: String, password_hash: String) -> Self {
        Self {
            id,
            username,
            email: None,
            password_hash: Some(password_hash),
            roles: Vec::new(),
            additional_permissions: PermissionSet::new(),
            is_active: true,
            is_root: false,
        }
    }

    /// Create a root user with password hash
    pub fn root_user(id: String, username: String, password_hash: String) -> Self {
        let mut user = Self::with_password_hash(id, username, password_hash);
        user.is_root = true;
        // Root user gets Super permission
        user.additional_permissions.add(Permission::Super);
        user
    }

    /// Add a role to the user
    pub fn add_role(&mut self, role_id: String) {
        if !self.roles.contains(&role_id) {
            self.roles.push(role_id);
        }
    }

    /// Remove a role from the user
    pub fn remove_role(&mut self, role_id: &str) {
        self.roles.retain(|r| r != role_id);
    }

    /// Add an additional permission to the user
    pub fn add_permission(&mut self, permission: Permission) {
        self.additional_permissions.add(permission);
    }

    /// Remove an additional permission from the user
    pub fn remove_permission(&mut self, permission: &Permission) {
        self.additional_permissions.remove(permission);
    }

    /// Check if the user has a specific permission
    pub fn has_permission(&self, permission: &Permission, rbac: &RoleBasedAccessControl) -> bool {
        // Check additional permissions first
        if self.additional_permissions.has_permission(permission) {
            return true;
        }

        // Check role permissions
        for role_id in &self.roles {
            if let Some(role) = rbac.get_role(role_id) {
                if role.has_permission(permission) {
                    return true;
                }
            }
        }

        false
    }

    /// Get all effective permissions for the user
    pub fn effective_permissions(&self, rbac: &RoleBasedAccessControl) -> PermissionSet {
        let mut permissions = self.additional_permissions.clone();

        for role_id in &self.roles {
            if let Some(role) = rbac.get_role(role_id) {
                permissions.merge(&role.permissions);
            }
        }

        permissions
    }
}

/// Role-Based Access Control system
#[derive(Debug, Clone)]
pub struct RoleBasedAccessControl {
    roles: HashMap<String, Role>,
    users: HashMap<String, User>,
}

impl RoleBasedAccessControl {
    /// Create a new RBAC system
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
            users: HashMap::new(),
        }
    }

    /// Add a role to the system
    pub fn add_role(&mut self, role: Role) {
        self.roles.insert(role.id.clone(), role);
    }

    /// Get a role by ID
    pub fn get_role(&self, role_id: &str) -> Option<&Role> {
        self.roles.get(role_id)
    }

    /// Get a mutable reference to a role by ID
    pub fn get_role_mut(&mut self, role_id: &str) -> Option<&mut Role> {
        self.roles.get_mut(role_id)
    }

    /// Remove a role from the system
    pub fn remove_role(&mut self, role_id: &str) -> Option<Role> {
        self.roles.remove(role_id)
    }

    /// List all roles
    pub fn list_roles(&self) -> Vec<&Role> {
        self.roles.values().collect()
    }

    /// Add a user to the system
    pub fn add_user(&mut self, user: User) {
        self.users.insert(user.id.clone(), user);
    }

    /// Get a user by ID
    pub fn get_user(&self, user_id: &str) -> Option<&User> {
        self.users.get(user_id)
    }

    /// Get a mutable reference to a user by ID
    pub fn get_user_mut(&mut self, user_id: &str) -> Option<&mut User> {
        self.users.get_mut(user_id)
    }

    /// Remove a user from the system
    pub fn remove_user(&mut self, user_id: &str) -> Option<User> {
        // Prevent deletion of root user
        if let Some(user) = self.users.get(user_id) {
            if user.is_root {
                return None;
            }
        }
        self.users.remove(user_id)
    }

    /// Create root user with password hash
    pub fn create_root_user(
        &mut self,
        username: String,
        password_hash: String,
    ) -> Result<(), String> {
        let root_id = "root".to_string();

        // Check if root user already exists
        if self.users.contains_key(&root_id) {
            return Err("Root user already exists".to_string());
        }

        let root_user = User::root_user(root_id.clone(), username, password_hash);
        self.users.insert(root_id, root_user);
        Ok(())
    }

    /// Disable root user
    pub fn disable_root_user(&mut self) -> Result<(), String> {
        if let Some(user) = self.users.get_mut("root") {
            if user.is_root {
                user.is_active = false;
                Ok(())
            } else {
                Err("User is not root".to_string())
            }
        } else {
            Err("Root user not found".to_string())
        }
    }

    /// Check if root user exists and is enabled
    pub fn is_root_enabled(&self) -> bool {
        if let Some(user) = self.users.get("root") {
            user.is_root && user.is_active
        } else {
            false
        }
    }

    /// List all users
    pub fn list_users(&self) -> Vec<&User> {
        self.users.values().collect()
    }

    /// Check if a user has a specific permission
    pub fn user_has_permission(&self, user_id: &str, permission: &Permission) -> bool {
        if let Some(user) = self.get_user(user_id) {
            user.has_permission(permission, self)
        } else {
            false
        }
    }

    /// Get effective permissions for a user
    pub fn user_permissions(&self, user_id: &str) -> Option<PermissionSet> {
        self.get_user(user_id)
            .map(|user| user.effective_permissions(self))
    }

    /// Create default roles
    pub fn create_default_roles(&mut self) {
        // Read-only role
        let read_only = Role::with_description(
            "read_only".to_string(),
            "Read Only".to_string(),
            "Can only read data from the database".to_string(),
            PermissionSet::from_vec(vec![Permission::Read]),
        );
        self.add_role(read_only);

        // Read-write role
        let read_write = Role::with_description(
            "read_write".to_string(),
            "Read Write".to_string(),
            "Can read and write data to the database".to_string(),
            PermissionSet::from_vec(vec![Permission::Read, Permission::Write]),
        );
        self.add_role(read_write);

        // Admin role
        let admin = Role::with_description(
            "admin".to_string(),
            "Administrator".to_string(),
            "Can manage the database schema and settings".to_string(),
            PermissionSet::from_vec(vec![Permission::Read, Permission::Write, Permission::Admin]),
        );
        self.add_role(admin);

        // Super user role
        let super_user = Role::with_description(
            "super".to_string(),
            "Super User".to_string(),
            "Full access to all database operations including replication".to_string(),
            PermissionSet::from_vec(vec![
                Permission::Read,
                Permission::Write,
                Permission::Admin,
                Permission::Super,
            ]),
        );
        self.add_role(super_user);
    }
}

impl Default for RoleBasedAccessControl {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_creation() {
        let role = Role::new(
            "test_role".to_string(),
            "Test Role".to_string(),
            PermissionSet::from_vec(vec![Permission::Read]),
        );

        assert_eq!(role.id, "test_role");
        assert_eq!(role.name, "Test Role");
        assert!(role.has_permission(&Permission::Read));
        assert!(!role.has_permission(&Permission::Write));
    }

    #[test]
    fn test_user_creation() {
        let user = User::new("user1".to_string(), "testuser".to_string());

        assert_eq!(user.id, "user1");
        assert_eq!(user.username, "testuser");
        assert!(user.is_active);
        assert!(user.roles.is_empty());
    }

    #[test]
    fn test_user_with_email() {
        let user = User::with_email(
            "user1".to_string(),
            "testuser".to_string(),
            "test@example.com".to_string(),
        );

        assert_eq!(user.email, Some("test@example.com".to_string()));
    }

    #[test]
    fn test_user_role_management() {
        let mut user = User::new("user1".to_string(), "testuser".to_string());

        user.add_role("admin".to_string());
        assert!(user.roles.contains(&"admin".to_string()));

        user.remove_role("admin");
        assert!(!user.roles.contains(&"admin".to_string()));
    }

    #[test]
    fn test_user_permission_management() {
        let mut user = User::new("user1".to_string(), "testuser".to_string());

        user.add_permission(Permission::Read);
        assert!(
            user.additional_permissions
                .has_permission(&Permission::Read)
        );

        user.remove_permission(&Permission::Read);
        assert!(
            !user
                .additional_permissions
                .has_permission(&Permission::Read)
        );
    }

    #[test]
    fn test_rbac_creation() {
        let rbac = RoleBasedAccessControl::new();
        assert!(rbac.roles.is_empty());
        assert!(rbac.users.is_empty());
    }

    #[test]
    fn test_rbac_role_management() {
        let mut rbac = RoleBasedAccessControl::new();

        let role = Role::new(
            "admin".to_string(),
            "Administrator".to_string(),
            PermissionSet::from_vec(vec![Permission::Admin]),
        );

        rbac.add_role(role);
        assert!(rbac.get_role("admin").is_some());

        rbac.remove_role("admin");
        assert!(rbac.get_role("admin").is_none());
    }

    #[test]
    fn test_rbac_user_management() {
        let mut rbac = RoleBasedAccessControl::new();

        let user = User::new("user1".to_string(), "testuser".to_string());

        rbac.add_user(user);
        assert!(rbac.get_user("user1").is_some());

        rbac.remove_user("user1");
        assert!(rbac.get_user("user1").is_none());
    }

    #[test]
    fn test_user_permission_checking() {
        let mut rbac = RoleBasedAccessControl::new();

        // Create admin role
        let admin_role = Role::new(
            "admin".to_string(),
            "Administrator".to_string(),
            PermissionSet::from_vec(vec![Permission::Admin]),
        );
        rbac.add_role(admin_role);

        // Create user with admin role
        let mut user = User::new("user1".to_string(), "testuser".to_string());
        user.add_role("admin".to_string());
        rbac.add_user(user);

        // Check permissions
        assert!(rbac.user_has_permission("user1", &Permission::Read));
        assert!(rbac.user_has_permission("user1", &Permission::Write));
        assert!(rbac.user_has_permission("user1", &Permission::Admin));
        assert!(!rbac.user_has_permission("user1", &Permission::Super));
    }

    #[test]
    fn test_root_user_creation() {
        let mut rbac = RoleBasedAccessControl::new();

        let result = rbac.create_root_user("root".to_string(), "hashed_password".to_string());
        assert!(result.is_ok());

        let root_user = rbac.get_user("root");
        assert!(root_user.is_some());
        let root_user = root_user.unwrap();
        assert!(root_user.is_root);
        assert!(
            root_user
                .additional_permissions
                .has_permission(&Permission::Super)
        );
    }

    #[test]
    fn test_root_user_cannot_be_deleted() {
        let mut rbac = RoleBasedAccessControl::new();

        rbac.create_root_user("root".to_string(), "hashed_password".to_string())
            .unwrap();

        let result = rbac.remove_user("root");
        assert!(result.is_none()); // Root user cannot be deleted
        assert!(rbac.get_user("root").is_some()); // Still exists
    }

    #[test]
    fn test_root_user_disable() {
        let mut rbac = RoleBasedAccessControl::new();

        rbac.create_root_user("root".to_string(), "hashed_password".to_string())
            .unwrap();
        assert!(rbac.is_root_enabled());

        rbac.disable_root_user().unwrap();
        assert!(!rbac.is_root_enabled());
    }

    #[test]
    fn test_user_with_password_hash() {
        let user = User::with_password_hash(
            "user1".to_string(),
            "testuser".to_string(),
            "hashed_password".to_string(),
        );

        assert_eq!(user.password_hash, Some("hashed_password".to_string()));
        assert!(!user.is_root);
    }

    #[test]
    fn test_default_roles_creation() {
        let mut rbac = RoleBasedAccessControl::new();
        rbac.create_default_roles();

        assert!(rbac.get_role("read_only").is_some());
        assert!(rbac.get_role("read_write").is_some());
        assert!(rbac.get_role("admin").is_some());
        assert!(rbac.get_role("super").is_some());
    }
}
