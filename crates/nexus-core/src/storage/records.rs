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

/// `flags` bit 0 (both [`NodeRecord`] and [`RelationshipRecord`]): the
/// record has been soft-deleted. Set by `mark_deleted`; checked by
/// `is_deleted`. Unchanged by phase0_fix-anonymous-node-lost-on-restart.
pub const FLAG_DELETED: u32 = 0b01;

/// `flags` bit 1 (both [`NodeRecord`] and [`RelationshipRecord`]): the
/// record is allocated / in use. Set on every live write via
/// [`super::record_store::RecordStore::write_node`] /
/// [`super::record_store::RecordStore::write_rel`].
///
/// phase0_fix-anonymous-node-lost-on-restart: without this bit, a node with
/// no labels, no properties and no relationships (or a relationship with
/// `src_id == dst_id == 0`, `type_id == 0`, no pointers) persists as a
/// byte-for-byte all-zero record, indistinguishable from an unallocated
/// slot. The restart recovery scan reconstructs `next_node_id` /
/// `next_rel_id` from this bit instead of "any byte is non-zero", so an
/// all-zero live record is no longer silently dropped and its id reused.
pub const FLAG_ALLOCATED: u32 = 0b10;

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
        self.flags |= FLAG_DELETED;
    }

    /// Check if this node is deleted
    pub fn is_deleted(&self) -> bool {
        (self.flags & FLAG_DELETED) != 0
    }

    /// Check if this node has the allocated (in-use) bit set.
    ///
    /// Set automatically by [`super::record_store::RecordStore::write_node`]
    /// on every write. Used by the restart recovery scan to reconstruct
    /// `next_node_id` without relying on "any byte is non-zero", which
    /// cannot distinguish a live anonymous node (no labels, no properties,
    /// no relationships) from a free slot. See
    /// phase0_fix-anonymous-node-lost-on-restart.
    pub fn is_allocated(&self) -> bool {
        (self.flags & FLAG_ALLOCATED) != 0
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
        self.flags |= FLAG_DELETED;
    }

    /// Check if this relationship is deleted
    pub fn is_deleted(&self) -> bool {
        (self.flags & FLAG_DELETED) != 0
    }

    /// Check if this relationship has the allocated (in-use) bit set.
    ///
    /// Set automatically by [`super::record_store::RecordStore::write_rel`]
    /// on every write. Closes the degenerate all-zero self-loop gap (`src_id
    /// == dst_id == 0`, `type_id == 0`, no pointers) with the same scheme
    /// used for [`NodeRecord::is_allocated`]. See
    /// phase0_fix-anonymous-node-lost-on-restart.
    pub fn is_allocated(&self) -> bool {
        (self.flags & FLAG_ALLOCATED) != 0
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
