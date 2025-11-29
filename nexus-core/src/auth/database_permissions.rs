//! Database-level permission system
//!
//! This module provides fine-grained access control for multi-database support,
//! allowing users to have different permissions for different databases.

use crate::auth::Permission;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Database-specific permission
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DatabasePermission {
    /// Read access to a database
    Read,
    /// Write access to a database
    Write,
    /// Admin access (schema changes, indexes)
    Admin,
    /// Full control (including drop database)
    Owner,
}

impl DatabasePermission {
    /// Check if this permission includes another permission
    pub fn includes(&self, other: &DatabasePermission) -> bool {
        matches!(
            (self, other),
            (DatabasePermission::Owner, _)
                | (
                    DatabasePermission::Admin,
                    DatabasePermission::Read | DatabasePermission::Write
                )
                | (DatabasePermission::Write, DatabasePermission::Read)
                | (DatabasePermission::Read, DatabasePermission::Read)
        )
    }

    /// Get the hierarchy level of this permission
    pub fn level(&self) -> u8 {
        match self {
            DatabasePermission::Read => 1,
            DatabasePermission::Write => 2,
            DatabasePermission::Admin => 3,
            DatabasePermission::Owner => 4,
        }
    }
}

/// Database access control list (ACL)
/// Maps users/API keys to their permissions for specific databases
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseACL {
    /// Map of database name -> (user_id/api_key_id -> permissions)
    acls: HashMap<String, HashMap<String, Vec<DatabasePermission>>>,
    /// Default permissions for new databases
    default_permissions: Vec<DatabasePermission>,
}

impl DatabaseACL {
    /// Create a new database ACL
    pub fn new() -> Self {
        Self {
            acls: HashMap::new(),
            default_permissions: vec![DatabasePermission::Read, DatabasePermission::Write],
        }
    }

    /// Grant permissions to a user/key for a specific database
    pub fn grant(
        &mut self,
        database: &str,
        principal_id: &str,
        permissions: Vec<DatabasePermission>,
    ) {
        let db_acl = self
            .acls
            .entry(database.to_string())
            .or_insert_with(HashMap::new);
        db_acl.insert(principal_id.to_string(), permissions);
    }

    /// Revoke all permissions from a user/key for a specific database
    pub fn revoke(&mut self, database: &str, principal_id: &str) {
        if let Some(db_acl) = self.acls.get_mut(database) {
            db_acl.remove(principal_id);
        }
    }

    /// Revoke specific permissions from a user/key for a database
    pub fn revoke_permission(
        &mut self,
        database: &str,
        principal_id: &str,
        permission: &DatabasePermission,
    ) {
        if let Some(db_acl) = self.acls.get_mut(database) {
            if let Some(perms) = db_acl.get_mut(principal_id) {
                perms.retain(|p| p != permission);
                if perms.is_empty() {
                    db_acl.remove(principal_id);
                }
            }
        }
    }

    /// Check if a user/key has a specific permission for a database
    pub fn has_permission(
        &self,
        database: &str,
        principal_id: &str,
        required_permission: &DatabasePermission,
    ) -> bool {
        // Check database-specific permissions
        if let Some(db_acl) = self.acls.get(database) {
            if let Some(permissions) = db_acl.get(principal_id) {
                return permissions
                    .iter()
                    .any(|p| p.includes(required_permission) || p == required_permission);
            }
        }

        // No specific permissions found
        false
    }

    /// Check if a user/key has any access to a database
    pub fn has_any_access(&self, database: &str, principal_id: &str) -> bool {
        if let Some(db_acl) = self.acls.get(database) {
            return db_acl.contains_key(principal_id);
        }
        false
    }

    /// Get all permissions for a user/key on a specific database
    pub fn get_permissions(&self, database: &str, principal_id: &str) -> Vec<DatabasePermission> {
        if let Some(db_acl) = self.acls.get(database) {
            if let Some(permissions) = db_acl.get(principal_id) {
                return permissions.clone();
            }
        }
        Vec::new()
    }

    /// List all databases a user/key has access to
    pub fn list_accessible_databases(&self, principal_id: &str) -> Vec<String> {
        self.acls
            .iter()
            .filter(|(_, db_acl)| db_acl.contains_key(principal_id))
            .map(|(db_name, _)| db_name.clone())
            .collect()
    }

    /// List all principals (users/keys) with access to a database
    pub fn list_principals(&self, database: &str) -> Vec<String> {
        if let Some(db_acl) = self.acls.get(database) {
            return db_acl.keys().cloned().collect();
        }
        Vec::new()
    }

    /// Set default permissions for new databases
    pub fn set_default_permissions(&mut self, permissions: Vec<DatabasePermission>) {
        self.default_permissions = permissions;
    }

    /// Get default permissions
    pub fn default_permissions(&self) -> &[DatabasePermission] {
        &self.default_permissions
    }

    /// Remove all permissions for a database (when database is dropped)
    pub fn remove_database(&mut self, database: &str) {
        self.acls.remove(database);
    }
}

impl Default for DatabaseACL {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to check database access with global permissions
/// Combines database-specific permissions with global API key permissions
pub fn check_database_access(
    database: &str,
    principal_id: &str,
    required_db_permission: &DatabasePermission,
    global_permissions: &[Permission],
    acl: &DatabaseACL,
) -> bool {
    // Super users have access to everything
    if global_permissions.contains(&Permission::Super) {
        return true;
    }

    // Admin users have full database access
    if global_permissions.contains(&Permission::Admin) {
        return true;
    }

    // Check database-specific permissions
    if acl.has_permission(database, principal_id, required_db_permission) {
        return true;
    }

    // For Read/Write, also check global permissions
    match required_db_permission {
        DatabasePermission::Read => global_permissions.contains(&Permission::Read),
        DatabasePermission::Write => {
            global_permissions.contains(&Permission::Write)
                || global_permissions.contains(&Permission::Admin)
        }
        DatabasePermission::Admin | DatabasePermission::Owner => {
            // These require explicit database-level grants
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_permission_includes() {
        assert!(DatabasePermission::Owner.includes(&DatabasePermission::Read));
        assert!(DatabasePermission::Owner.includes(&DatabasePermission::Write));
        assert!(DatabasePermission::Owner.includes(&DatabasePermission::Admin));
        assert!(DatabasePermission::Owner.includes(&DatabasePermission::Owner));

        assert!(DatabasePermission::Admin.includes(&DatabasePermission::Read));
        assert!(DatabasePermission::Admin.includes(&DatabasePermission::Write));
        assert!(!DatabasePermission::Admin.includes(&DatabasePermission::Owner));

        assert!(DatabasePermission::Write.includes(&DatabasePermission::Read));
        assert!(!DatabasePermission::Write.includes(&DatabasePermission::Admin));
    }

    #[test]
    fn test_database_permission_level() {
        assert_eq!(DatabasePermission::Read.level(), 1);
        assert_eq!(DatabasePermission::Write.level(), 2);
        assert_eq!(DatabasePermission::Admin.level(), 3);
        assert_eq!(DatabasePermission::Owner.level(), 4);
    }

    #[test]
    fn test_database_acl_grant_revoke() {
        let mut acl = DatabaseACL::new();

        // Grant permissions
        acl.grant("db1", "user1", vec![DatabasePermission::Read]);
        assert!(acl.has_permission("db1", "user1", &DatabasePermission::Read));
        assert!(!acl.has_permission("db1", "user1", &DatabasePermission::Write));

        // Revoke permissions
        acl.revoke("db1", "user1");
        assert!(!acl.has_permission("db1", "user1", &DatabasePermission::Read));
    }

    #[test]
    fn test_database_acl_has_permission() {
        let mut acl = DatabaseACL::new();

        acl.grant("db1", "user1", vec![DatabasePermission::Admin]);

        // Admin includes Read and Write
        assert!(acl.has_permission("db1", "user1", &DatabasePermission::Read));
        assert!(acl.has_permission("db1", "user1", &DatabasePermission::Write));
        assert!(acl.has_permission("db1", "user1", &DatabasePermission::Admin));
        assert!(!acl.has_permission("db1", "user1", &DatabasePermission::Owner));

        // Different user has no access
        assert!(!acl.has_permission("db1", "user2", &DatabasePermission::Read));

        // Different database has no access
        assert!(!acl.has_permission("db2", "user1", &DatabasePermission::Read));
    }

    #[test]
    fn test_database_acl_list_accessible_databases() {
        let mut acl = DatabaseACL::new();

        acl.grant("db1", "user1", vec![DatabasePermission::Read]);
        acl.grant("db2", "user1", vec![DatabasePermission::Write]);
        acl.grant("db3", "user2", vec![DatabasePermission::Admin]);

        let user1_dbs = acl.list_accessible_databases("user1");
        assert_eq!(user1_dbs.len(), 2);
        assert!(user1_dbs.contains(&"db1".to_string()));
        assert!(user1_dbs.contains(&"db2".to_string()));

        let user2_dbs = acl.list_accessible_databases("user2");
        assert_eq!(user2_dbs.len(), 1);
        assert!(user2_dbs.contains(&"db3".to_string()));
    }

    #[test]
    fn test_database_acl_list_principals() {
        let mut acl = DatabaseACL::new();

        acl.grant("db1", "user1", vec![DatabasePermission::Read]);
        acl.grant("db1", "user2", vec![DatabasePermission::Write]);

        let principals = acl.list_principals("db1");
        assert_eq!(principals.len(), 2);
        assert!(principals.contains(&"user1".to_string()));
        assert!(principals.contains(&"user2".to_string()));
    }

    #[test]
    fn test_database_acl_revoke_permission() {
        let mut acl = DatabaseACL::new();

        acl.grant(
            "db1",
            "user1",
            vec![DatabasePermission::Read, DatabasePermission::Write],
        );

        // Revoke specific permission
        acl.revoke_permission("db1", "user1", &DatabasePermission::Write);

        assert!(acl.has_permission("db1", "user1", &DatabasePermission::Read));
        assert!(!acl.has_permission("db1", "user1", &DatabasePermission::Write));
    }

    #[test]
    fn test_check_database_access_with_global_permissions() {
        let acl = DatabaseACL::new();

        // Super user has access
        assert!(check_database_access(
            "db1",
            "super_user",
            &DatabasePermission::Owner,
            &[Permission::Super],
            &acl
        ));

        // Admin user has access
        assert!(check_database_access(
            "db1",
            "admin_user",
            &DatabasePermission::Write,
            &[Permission::Admin],
            &acl
        ));

        // Regular user with Read permission
        assert!(check_database_access(
            "db1",
            "regular_user",
            &DatabasePermission::Read,
            &[Permission::Read],
            &acl
        ));

        // Regular user cannot admin
        assert!(!check_database_access(
            "db1",
            "regular_user",
            &DatabasePermission::Admin,
            &[Permission::Read],
            &acl
        ));
    }

    #[test]
    fn test_check_database_access_with_database_acl() {
        let mut acl = DatabaseACL::new();
        acl.grant("db1", "user1", vec![DatabasePermission::Owner]);

        // User has Owner permission via ACL
        assert!(check_database_access(
            "db1",
            "user1",
            &DatabasePermission::Owner,
            &[Permission::Read], // Global permission is Read, but ACL grants Owner
            &acl
        ));

        // User doesn't have access to db2
        assert!(!check_database_access(
            "db2",
            "user1",
            &DatabasePermission::Read,
            &[],
            &acl
        ));
    }

    #[test]
    fn test_database_acl_remove_database() {
        let mut acl = DatabaseACL::new();

        acl.grant("db1", "user1", vec![DatabasePermission::Read]);
        acl.grant("db1", "user2", vec![DatabasePermission::Write]);

        assert!(acl.has_any_access("db1", "user1"));
        assert!(acl.has_any_access("db1", "user2"));

        // Remove database
        acl.remove_database("db1");

        assert!(!acl.has_any_access("db1", "user1"));
        assert!(!acl.has_any_access("db1", "user2"));
    }
}
