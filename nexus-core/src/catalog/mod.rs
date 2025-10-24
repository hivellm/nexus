//! Catalog module - Label/Type/Key mappings
//!
//! The catalog maintains bidirectional mappings between:
//! - Labels (node labels) ↔ LabelId
//! - Types (relationship types) ↔ TypeId
//! - Keys (property keys) ↔ KeyId
//!
//! Uses LMDB (via heed) for durable storage of these mappings.

use crate::{Error, Result};

/// Catalog for managing label/type/key mappings
pub struct Catalog {
    // Will use heed::Database for persistent storage
}

impl Catalog {
    /// Create a new catalog instance
    pub fn new() -> Result<Self> {
        todo!("Catalog::new - to be implemented in MVP")
    }

    /// Get or create a label ID
    pub fn get_or_create_label(&mut self, _label: &str) -> Result<u32> {
        todo!("get_or_create_label - to be implemented in MVP")
    }

    /// Get or create a type ID
    pub fn get_or_create_type(&mut self, _type_name: &str) -> Result<u32> {
        todo!("get_or_create_type - to be implemented in MVP")
    }

    /// Get or create a key ID
    pub fn get_or_create_key(&mut self, _key: &str) -> Result<u32> {
        todo!("get_or_create_key - to be implemented in MVP")
    }
}

impl Default for Catalog {
    fn default() -> Self {
        Self::new().expect("Failed to create default catalog")
    }
}
