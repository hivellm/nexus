//! Primitive types and value types shared across all catalog sub-modules.
//!
//! Keeps the common surface small so every other sub-module can `use
//! super::types::*` without pulling in LMDB or parking-lot.

/// Label ID type.
pub type LabelId = u32;

/// Relationship type ID.
pub type TypeId = u32;

/// Property key ID.
pub type KeyId = u32;

/// Statistics for catalog.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CatalogStats {
    /// Total number of nodes per label.
    pub node_counts: std::collections::HashMap<LabelId, u64>,
    /// Total number of relationships per type.
    pub rel_counts: std::collections::HashMap<TypeId, u64>,
    /// Total number of unique labels.
    pub label_count: u32,
    /// Total number of unique types.
    pub type_count: u32,
    /// Total number of unique keys.
    pub key_count: u32,
}

/// Metadata stored in catalog.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CatalogMetadata {
    /// Storage format version.
    pub version: u32,
    /// Current epoch (for MVCC).
    pub epoch: u64,
    /// Page size in bytes.
    pub page_size: u32,
}

impl Default for CatalogMetadata {
    fn default() -> Self {
        Self {
            version: 1,
            epoch: 0,
            page_size: 8192, // 8KB pages
        }
    }
}
