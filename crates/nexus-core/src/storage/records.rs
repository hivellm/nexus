//! Record layout types for nodes and relationships.
//!
//! This module contains the on-disk/in-memory layout structs for `NodeRecord`
//! and `RelationshipRecord`, the file-size constants used by `RecordStore`,
//! and the `RecordStoreStats` snapshot type.

/// Size of a node record in bytes (32 bytes)
pub const NODE_RECORD_SIZE: usize = 32;

/// Size of a relationship record in bytes (52 bytes)
pub const REL_RECORD_SIZE: usize = 52;

/// Initial file size for nodes.store (1MB)
pub(super) const INITIAL_NODES_FILE_SIZE: usize = 1024 * 1024;

/// Initial file size for rels.store (1MB)
pub(super) const INITIAL_RELS_FILE_SIZE: usize = 1024 * 1024;

/// Growth factor for file expansion
pub(super) const FILE_GROWTH_FACTOR: f64 = 1.5;

/// Node record structure (32 bytes)
#[derive(Debug, Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct NodeRecord {
    /// Bitmap of labels (64 bits = 64 possible labels)
    pub label_bits: u64,
    /// Pointer to first relationship
    pub first_rel_ptr: u64,
    /// Pointer to properties
    pub prop_ptr: u64,
    /// Flags (deleted, etc.)
    pub flags: u32,
    /// Reserved for future use
    pub reserved: u32,
}

impl NodeRecord {
    /// Create a new empty node record
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a label to this node
    pub fn add_label(&mut self, label_id: u32) {
        if label_id < 64 {
            self.label_bits |= 1 << label_id;
        }
    }

    /// Remove a label from this node
    pub fn remove_label(&mut self, label_id: u32) {
        if label_id < 64 {
            self.label_bits &= !(1 << label_id);
        }
    }

    /// Check if this node has a specific label
    pub fn has_label(&self, label_id: u32) -> bool {
        if label_id < 64 {
            (self.label_bits & (1 << label_id)) != 0
        } else {
            false
        }
    }

    /// Mark this node as deleted
    pub fn mark_deleted(&mut self) {
        self.flags |= 1;
    }

    /// Check if this node is deleted
    pub fn is_deleted(&self) -> bool {
        (self.flags & 1) != 0
    }

    /// Get all labels for this node
    pub fn get_labels(&self) -> Vec<u32> {
        let mut labels = Vec::new();
        for i in 0..64 {
            if self.has_label(i) {
                labels.push(i);
            }
        }
        labels
    }
}

/// Relationship record structure (52 bytes)
#[derive(Debug, Clone, Copy, Default)]
#[repr(C, packed)]
pub struct RelationshipRecord {
    /// Source node ID
    pub src_id: u64,
    /// Destination node ID
    pub dst_id: u64,
    /// Relationship type ID
    pub type_id: u32,
    /// Pointer to next relationship from source
    pub next_src_ptr: u64,
    /// Pointer to next relationship from destination
    pub next_dst_ptr: u64,
    /// Pointer to properties
    pub prop_ptr: u64,
    /// Flags (deleted, etc.)
    pub flags: u32,
    /// Reserved for future use
    pub reserved: u32,
}

unsafe impl bytemuck::Pod for RelationshipRecord {}
unsafe impl bytemuck::Zeroable for RelationshipRecord {}

impl RelationshipRecord {
    /// Create a new relationship record
    pub fn new(src_id: u64, dst_id: u64, type_id: u32) -> Self {
        Self {
            src_id,
            dst_id,
            type_id,
            next_src_ptr: u64::MAX,
            next_dst_ptr: u64::MAX,
            prop_ptr: u64::MAX,
            flags: 0,
            reserved: 0,
        }
    }

    /// Mark this relationship as deleted
    pub fn mark_deleted(&mut self) {
        self.flags |= 1;
    }

    /// Check if this relationship is deleted
    pub fn is_deleted(&self) -> bool {
        (self.flags & 1) != 0
    }
}

/// Record store statistics
#[derive(Debug, Clone)]
pub struct RecordStoreStats {
    /// Total number of nodes
    pub node_count: u64,
    /// Total number of relationships
    pub rel_count: u64,
    /// Size of nodes.store file in bytes
    pub nodes_file_size: usize,
    /// Size of rels.store file in bytes
    pub rels_file_size: usize,
}
