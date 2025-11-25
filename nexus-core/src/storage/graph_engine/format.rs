//! Storage format definitions for the graph-native storage engine.
//!
//! This module defines the on-disk format and memory layout for graph data,
//! optimized for relationship-centric access patterns.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;
use std::ops::Range;

/// Type alias for relationship type IDs
pub type TypeId = u32;

/// Type alias for node IDs
pub type NodeId = u64;

/// Header for the entire graph storage file
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct GraphHeader {
    /// Magic number to identify graph files (0x67726170686462 = "graphdb")
    pub magic: u64,
    /// Version of the storage format
    pub version: u32,
    /// Total file size
    pub file_size: u64,
    /// Offset to node segment
    pub nodes_offset: u64,
    /// Size of node segment
    pub nodes_size: u64,
    /// Offset to properties segment
    pub properties_offset: u64,
    /// Size of properties segment
    pub properties_size: u64,
    /// Number of relationship types
    pub relationship_type_count: u32,
    /// Reserved for future use
    pub reserved: [u64; 8],
}

impl GraphHeader {
    pub const MAGIC: u64 = 0x67726170686462; // "graphdb"
    pub const VERSION: u32 = 1;
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn new() -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            file_size: Self::SIZE as u64,
            nodes_offset: Self::SIZE as u64,
            nodes_size: 0,
            properties_offset: 0,
            properties_size: 0,
            relationship_type_count: 0,
            reserved: [0; 8],
        }
    }

    pub fn is_valid(&self) -> bool {
        self.magic == Self::MAGIC && self.version <= Self::VERSION
    }
}

/// Node record - optimized for graph access patterns (64 bytes, cache line aligned)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NodeRecord {
    /// Node ID
    pub id: NodeId,
    /// Offset to first relationship (0 if none)
    pub first_rel_offset: u64,
    /// Offset to properties (0 if none)
    pub prop_offset: u64,
    /// Label ID
    pub label_id: u32,
    /// Node flags (for future extensions)
    pub flags: u32,
    /// Creation timestamp
    pub created_at: u64,
    /// Last update timestamp
    pub updated_at: u64,
}

impl NodeRecord {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn new(id: NodeId, label_id: u32) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            first_rel_offset: 0,
            prop_offset: 0,
            label_id,
            flags: 0,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn update_timestamp(&mut self) {
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

/// Relationship record - core data structure for graph relationships (32 bytes)
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RelationshipRecord {
    /// Relationship ID
    pub id: u64,
    /// Source node ID
    pub from_node: NodeId,
    /// Target node ID
    pub to_node: NodeId,
    /// Relationship type ID
    pub type_id: TypeId,
    /// Offset to properties (0 if none)
    pub prop_offset: u32,
    /// Flags for relationship state
    pub flags: u16,
    /// Checksum for data integrity
    pub checksum: u16,
}

impl RelationshipRecord {
    pub const SIZE: usize = std::mem::size_of::<Self>();

    pub fn new(id: u64, from: NodeId, to: NodeId, type_id: TypeId) -> Self {
        Self {
            id,
            from_node: from,
            to_node: to,
            type_id,
            prop_offset: 0,
            flags: 0,
            checksum: 0, // TODO: Implement checksum calculation
        }
    }

    pub fn calculate_checksum(&self) -> u16 {
        // Simple checksum calculation (can be improved)
        let data = unsafe {
            std::slice::from_raw_parts(
                self as *const Self as *const u8,
                Self::SIZE - 2, // Exclude checksum field
            )
        };
        data.iter().fold(0u16, |acc, &x| acc.wrapping_add(x as u16))
    }

    pub fn validate_checksum(&self) -> bool {
        self.calculate_checksum() == self.checksum
    }
}

/// Adjacency list entry for fast relationship lookups
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdjacencyEntry {
    /// Relationship ID
    pub rel_id: u64,
}

/// Trait for serializable storage records
pub trait StorageRecord {
    const SIZE: usize;

    /// Serialize to bytes
    fn to_bytes(&self) -> Vec<u8>;

    /// Deserialize from bytes
    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>>
    where
        Self: Sized;
}

impl StorageRecord for GraphHeader {
    const SIZE: usize = std::mem::size_of::<Self>();

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                bytes.as_mut_ptr(),
                Self::SIZE,
            );
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() != Self::SIZE {
            return Err("Invalid byte length".into());
        }
        let mut result = Self {
            magic: 0,
            version: 0,
            file_size: 0,
            nodes_offset: 0,
            nodes_size: 0,
            properties_offset: 0,
            properties_size: 0,
            relationship_type_count: 0,
            reserved: [0; 8],
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut result as *mut Self as *mut u8,
                Self::SIZE,
            );
        }
        Ok(result)
    }
}

impl StorageRecord for NodeRecord {
    const SIZE: usize = std::mem::size_of::<Self>();

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                bytes.as_mut_ptr(),
                Self::SIZE,
            );
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() != Self::SIZE {
            return Err("Invalid byte length".into());
        }
        let mut result = Self {
            id: 0,
            first_rel_offset: 0,
            prop_offset: 0,
            label_id: 0,
            flags: 0,
            created_at: 0,
            updated_at: 0,
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut result as *mut Self as *mut u8,
                Self::SIZE,
            );
        }
        Ok(result)
    }
}

impl StorageRecord for RelationshipRecord {
    const SIZE: usize = std::mem::size_of::<Self>();

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                bytes.as_mut_ptr(),
                Self::SIZE,
            );
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() != Self::SIZE {
            return Err("Invalid byte length".into());
        }
        let mut result = Self {
            id: 0,
            from_node: 0,
            to_node: 0,
            type_id: 0,
            prop_offset: 0,
            flags: 0,
            checksum: 0,
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut result as *mut Self as *mut u8,
                Self::SIZE,
            );
        }
        Ok(result)
    }
}

impl StorageRecord for AdjacencyEntry {
    const SIZE: usize = std::mem::size_of::<Self>();

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                bytes.as_mut_ptr(),
                Self::SIZE,
            );
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() != Self::SIZE {
            return Err("Invalid byte length".into());
        }
        let mut result = Self { rel_id: 0 };
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut result as *mut Self as *mut u8,
                Self::SIZE,
            );
        }
        Ok(result)
    }
}

/// Index entry for fast adjacency lookups
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AdjacencyIndexEntry {
    /// Node ID
    pub node_id: NodeId,
    /// Offset to adjacency list in mmap
    pub list_offset: u64,
    /// Number of relationships in this list
    pub count: u32,
    /// Compression type used
    pub compression: u8,
    /// Reserved for future use
    pub reserved: [u8; 3],
}

impl AdjacencyIndexEntry {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

impl StorageRecord for AdjacencyIndexEntry {
    const SIZE: usize = std::mem::size_of::<Self>();

    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; Self::SIZE];
        unsafe {
            std::ptr::copy_nonoverlapping(
                self as *const Self as *const u8,
                bytes.as_mut_ptr(),
                Self::SIZE,
            );
        }
        bytes
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        if bytes.len() != Self::SIZE {
            return Err("Invalid byte length".into());
        }
        let mut result = Self {
            node_id: 0,
            list_offset: 0,
            count: 0,
            compression: 0,
            reserved: [0; 3],
        };
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                &mut result as *mut Self as *mut u8,
                Self::SIZE,
            );
        }
        Ok(result)
    }
}

/// Adjacency index for fast relationship lookups
#[derive(Clone, Debug)]
pub struct AdjacencyIndex {
    /// Index entries keyed by node ID
    pub entries: std::collections::HashMap<NodeId, AdjacencyIndexEntry>,
    /// Temporary relationship ID lists (used before compression)
    pub rel_id_lists: std::collections::HashMap<NodeId, Vec<u64>>,
    /// Base offset for this index in the file
    pub base_offset: u64,
    /// Total size of the index
    pub size: u64,
}

impl AdjacencyIndex {
    pub fn new(base_offset: u64) -> Self {
        Self {
            entries: std::collections::HashMap::new(),
            rel_id_lists: std::collections::HashMap::new(),
            base_offset,
            size: 0,
        }
    }

    /// Add a relationship to the adjacency index
    pub fn add_relationship(&mut self, node_id: NodeId, rel_id: u64) {
        let entry = self
            .entries
            .entry(node_id)
            .or_insert_with(|| AdjacencyIndexEntry {
                node_id,
                list_offset: 0, // Will be set when persisting
                count: 0,
                compression: CompressionType::VarInt as u8,
                reserved: [0; 3],
            });
        entry.count += 1;

        // Also track the actual relationship ID
        self.rel_id_lists
            .entry(node_id)
            .or_insert_with(Vec::new)
            .push(rel_id);
    }

    /// Get the adjacency entry for a node
    pub fn get_entry(&self, node_id: NodeId) -> Option<&AdjacencyIndexEntry> {
        self.entries.get(&node_id)
    }

    /// Get the relationship ID list for a node (temporary, before compression)
    pub fn get_rel_ids(&self, node_id: NodeId) -> Option<&Vec<u64>> {
        self.rel_id_lists.get(&node_id)
    }

    /// Get all node IDs in this index
    pub fn node_ids(&self) -> std::collections::hash_map::Keys<NodeId, AdjacencyIndexEntry> {
        self.entries.keys()
    }

    /// Calculate the total size needed for this index
    pub fn calculate_size(&self) -> u64 {
        (self.entries.len() * AdjacencyIndexEntry::SIZE) as u64
    }
}

impl AdjacencyEntry {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// Header for adjacency lists
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AdjacencyHeader {
    /// Node ID this adjacency list belongs to
    pub node_id: NodeId,
    /// Number of relationships in this list
    pub count: u32,
    /// Relationship type ID
    pub type_id: TypeId,
}

impl AdjacencyHeader {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// Type table entry for relationship type metadata
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TypeTableEntry {
    /// Type ID
    pub id: TypeId,
    /// Offset to relationship segment for this type
    pub segment_offset: u64,
    /// Size of relationship segment
    pub segment_size: u64,
    /// Number of relationships of this type
    pub relationship_count: u64,
}

impl TypeTableEntry {
    pub const SIZE: usize = std::mem::size_of::<Self>();
}

/// Storage layout describing the physical organization of data in the file
#[derive(Clone, Debug)]
pub struct StorageLayout {
    /// File header
    pub header: Range<u64>,
    /// Node storage segment
    pub nodes: Range<u64>,
    /// Properties storage segment
    pub properties: Range<u64>,
    /// Type table segment
    pub type_table: Range<u64>,
    /// Relationship segments by type
    pub relationships: HashMap<TypeId, RelationshipSegment>,
    /// Free space for growth
    pub free_space: Range<u64>,
}

impl StorageLayout {
    pub fn new() -> Self {
        Self {
            header: 0..GraphHeader::SIZE as u64,
            nodes: 0..0,
            properties: 0..0,
            type_table: 0..0,
            relationships: HashMap::new(),
            free_space: 0..0,
        }
    }

    pub fn total_size(&self) -> u64 {
        [
            self.header.end,
            self.nodes.end,
            self.properties.end,
            self.type_table.end,
            self.relationships
                .values()
                .map(|s| s.data_range.end)
                .max()
                .unwrap_or(0),
            self.free_space.end,
        ]
        .into_iter()
        .max()
        .unwrap_or(0)
    }
}

/// Relationship segment containing all relationships of a specific type
#[derive(Clone, Debug)]
pub struct RelationshipSegment {
    /// Type ID for this segment
    pub type_id: TypeId,
    /// Range of relationship data in the file
    pub data_range: Range<u64>,
    /// Outgoing adjacency index (node -> relationships)
    pub outgoing_index: AdjacencyIndex,
    /// Incoming adjacency index (node -> incoming relationships)
    pub incoming_index: AdjacencyIndex,
    /// Range of compressed adjacency lists storage
    pub adjacency_data_range: Range<u64>,
    /// Number of relationships in this segment
    pub count: u64,
    /// Compression type used for adjacency lists
    pub compression: CompressionType,
}

/// Advanced compression types for relationship data
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CompressionType {
    /// No compression
    None,
    /// Variable-length integer encoding
    VarInt,
    /// Delta encoding for sorted relationship IDs
    Delta,
    /// Dictionary-based compression
    Dictionary,
    /// LZ4 fast compression
    LZ4,
    /// Zstandard compression (configurable level)
    Zstd,
    /// Adaptive compression (chooses best algorithm automatically)
    Adaptive,
    /// SIMD-accelerated run-length encoding
    SimdRLE,
}

/// File growth constants
pub const INITIAL_NODE_CAPACITY: usize = 1_000_000; // 1M nodes
pub const INITIAL_REL_CAPACITY: usize = 5_000_000; // 5M relationships
pub const FILE_GROWTH_FACTOR: f64 = 2.0;
pub const MIN_GROWTH_SIZE: u64 = 64 * 1024 * 1024; // 64MB minimum growth

/// Constants for segment alignment (SSD block size)
pub const SEGMENT_ALIGNMENT: u64 = 4096; // 4KB blocks

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_header() {
        let header = GraphHeader::new();
        assert_eq!(header.magic, GraphHeader::MAGIC);
        assert_eq!(header.version, GraphHeader::VERSION);
        assert!(header.is_valid());
    }

    #[test]
    fn test_node_record() {
        let node = NodeRecord::new(42, 1);
        assert_eq!(node.id, 42);
        assert_eq!(node.label_id, 1);
        assert_eq!(node.first_rel_offset, 0);
        assert!(node.created_at > 0);
    }

    #[test]
    fn test_relationship_record() {
        let rel = RelationshipRecord::new(100, 1, 2, 5);
        assert_eq!(rel.id, 100);
        assert_eq!(rel.from_node, 1);
        assert_eq!(rel.to_node, 2);
        assert_eq!(rel.type_id, 5);
    }

    #[test]
    fn test_storage_layout() {
        let layout = StorageLayout::new();
        assert_eq!(layout.header, 0..GraphHeader::SIZE as u64);
        assert!(layout.relationships.is_empty());
    }
}
