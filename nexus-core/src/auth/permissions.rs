//! Permission system for Nexus authentication

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Permissions that can be granted to API keys
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// Read access to the database
    Read,
    /// Write access to the database
    Write,
    /// Administrative access (schema changes, etc.)
    Admin,
    /// Super user access (replication, cluster management)
    Super,
    /// Queue operations (publish, consume)
    Queue,
    /// Chatroom operations
    Chatroom,
    /// REST API access
    Rest,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Permission::Read => write!(f, "read"),
            Permission::Write => write!(f, "write"),
            Permission::Admin => write!(f, "admin"),
            Permission::Super => write!(f, "super"),
            Permission::Queue => write!(f, "queue"),
            Permission::Chatroom => write!(f, "chatroom"),
            Permission::Rest => write!(f, "rest"),
        }
    }
}

impl Permission {
    /// Get all available permissions
    pub fn all() -> Vec<Permission> {
        vec![
            Permission::Read,
            Permission::Write,
            Permission::Admin,
            Permission::Super,
            Permission::Queue,
            Permission::Chatroom,
            Permission::Rest,
        ]
    }

    /// Check if this permission includes another permission
    pub fn includes(&self, other: &Permission) -> bool {
        matches!(
            (self, other),
            (Permission::Super, _)
                | (
                    Permission::Admin,
                    Permission::Read | Permission::Write | Permission::Queue | Permission::Chatroom
                )
                | (Permission::Write, Permission::Read)
                | (Permission::Read, Permission::Read)
                | (Permission::Queue, Permission::Queue)
                | (Permission::Chatroom, Permission::Chatroom)
        )
    }

    /// Get the hierarchy level of this permission
    pub fn level(&self) -> u8 {
        match self {
            Permission::Read => 1,
            Permission::Write => 2,
            Permission::Admin => 3,
            Permission::Super => 4,
            Permission::Queue => 2,
            Permission::Chatroom => 2,
            Permission::Rest => 1,
        }
    }
}

impl FromStr for Permission {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "read" => Ok(Permission::Read),
            "write" => Ok(Permission::Write),
            "admin" => Ok(Permission::Admin),
            "super" => Ok(Permission::Super),
            "queue" => Ok(Permission::Queue),
            "chatroom" => Ok(Permission::Chatroom),
            "rest" => Ok(Permission::Rest),
            _ => Err(format!("Invalid permission: {}", s)),
        }
    }
}

/// A set of permissions with utility methods
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSet {
    permissions: Vec<Permission>,
}

impl PermissionSet {
    /// Create a new empty permission set
    pub fn new() -> Self {
        Self {
            permissions: Vec::new(),
        }
    }

    /// Create a permission set from a vector of permissions
    pub fn from_vec(permissions: Vec<Permission>) -> Self {
        Self { permissions }
    }

    /// Add a permission to the set
    pub fn add(&mut self, permission: Permission) {
        if !self.permissions.contains(&permission) {
            self.permissions.push(permission);
        }
    }

    /// Remove a permission from the set
    pub fn remove(&mut self, permission: &Permission) {
        self.permissions.retain(|p| p != permission);
    }

    /// Check if the set contains a permission
    pub fn contains(&self, permission: &Permission) -> bool {
        self.permissions.contains(permission)
    }

    /// Check if the set has any permission that includes the given permission
    pub fn has_permission(&self, permission: &Permission) -> bool {
        self.permissions
            .iter()
            .any(|p| p.includes(permission) || p == permission)
    }

    /// Get all permissions in the set
    pub fn permissions(&self) -> &[Permission] {
        &self.permissions
    }

    /// Check if the set is empty
    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }

    /// Get the number of permissions in the set
    pub fn len(&self) -> usize {
        self.permissions.len()
    }

    /// Check if this permission set includes all permissions from another set
    pub fn includes_all(&self, other: &PermissionSet) -> bool {
        other.permissions.iter().all(|p| self.has_permission(p))
    }

    /// Merge another permission set into this one
    pub fn merge(&mut self, other: &PermissionSet) {
        for permission in &other.permissions {
            self.add(permission.clone());
        }
    }
}

impl Default for PermissionSet {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Vec<Permission>> for PermissionSet {
    fn from(permissions: Vec<Permission>) -> Self {
        Self::from_vec(permissions)
    }
}

impl From<PermissionSet> for Vec<Permission> {
    fn from(set: PermissionSet) -> Self {
        set.permissions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_display() {
        assert_eq!(Permission::Read.to_string(), "read");
        assert_eq!(Permission::Write.to_string(), "write");
        assert_eq!(Permission::Admin.to_string(), "admin");
        assert_eq!(Permission::Super.to_string(), "super");
    }

    #[test]
    fn test_permission_from_str() {
        assert_eq!("read".parse::<Permission>(), Ok(Permission::Read));
        assert_eq!("READ".parse::<Permission>(), Ok(Permission::Read));
        assert_eq!("Write".parse::<Permission>(), Ok(Permission::Write));
        assert_eq!("admin".parse::<Permission>(), Ok(Permission::Admin));
        assert_eq!("super".parse::<Permission>(), Ok(Permission::Super));
        assert_eq!("queue".parse::<Permission>(), Ok(Permission::Queue));
        assert_eq!("chatroom".parse::<Permission>(), Ok(Permission::Chatroom));
        assert_eq!("rest".parse::<Permission>(), Ok(Permission::Rest));
        assert!("invalid".parse::<Permission>().is_err());
    }

    #[test]
    fn test_permission_includes() {
        assert!(Permission::Super.includes(&Permission::Read));
        assert!(Permission::Super.includes(&Permission::Write));
        assert!(Permission::Super.includes(&Permission::Admin));
        assert!(Permission::Super.includes(&Permission::Super));

        assert!(Permission::Admin.includes(&Permission::Read));
        assert!(Permission::Admin.includes(&Permission::Write));
        assert!(!Permission::Admin.includes(&Permission::Super));

        assert!(Permission::Write.includes(&Permission::Read));
        assert!(!Permission::Write.includes(&Permission::Admin));
        assert!(!Permission::Write.includes(&Permission::Super));

        assert!(Permission::Read.includes(&Permission::Read));
        assert!(!Permission::Read.includes(&Permission::Write));
    }

    #[test]
    fn test_permission_level() {
        assert_eq!(Permission::Read.level(), 1);
        assert_eq!(Permission::Write.level(), 2);
        assert_eq!(Permission::Admin.level(), 3);
        assert_eq!(Permission::Super.level(), 4);
        assert_eq!(Permission::Queue.level(), 2);
        assert_eq!(Permission::Chatroom.level(), 2);
        assert_eq!(Permission::Rest.level(), 1);
    }

    #[test]
    fn test_permission_set_creation() {
        let set = PermissionSet::new();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn test_permission_set_operations() {
        let mut set = PermissionSet::new();

        set.add(Permission::Read);
        assert!(set.contains(&Permission::Read));
        assert_eq!(set.len(), 1);

        set.add(Permission::Write);
        assert!(set.contains(&Permission::Write));
        assert_eq!(set.len(), 2);

        set.remove(&Permission::Read);
        assert!(!set.contains(&Permission::Read));
        assert!(set.contains(&Permission::Write));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn test_permission_set_has_permission() {
        let set = PermissionSet::from_vec(vec![Permission::Admin]);

        assert!(set.has_permission(&Permission::Read));
        assert!(set.has_permission(&Permission::Write));
        assert!(set.has_permission(&Permission::Admin));
        assert!(!set.has_permission(&Permission::Super));
    }

    #[test]
    fn test_permission_set_includes_all() {
        let set1 = PermissionSet::from_vec(vec![Permission::Super]);
        let set2 = PermissionSet::from_vec(vec![Permission::Read, Permission::Write]);
        let set3 = PermissionSet::from_vec(vec![Permission::Super]);

        assert!(set1.includes_all(&set2));
        assert!(set1.includes_all(&set3));
        assert!(!set2.includes_all(&set1));
    }

    #[test]
    fn test_permission_set_merge() {
        let mut set1 = PermissionSet::from_vec(vec![Permission::Read]);
        let set2 = PermissionSet::from_vec(vec![Permission::Write, Permission::Admin]);

        set1.merge(&set2);

        assert!(set1.contains(&Permission::Read));
        assert!(set1.contains(&Permission::Write));
        assert!(set1.contains(&Permission::Admin));
        assert_eq!(set1.len(), 3);
    }
}
