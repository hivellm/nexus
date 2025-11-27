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
    /// Bloom filter for fast edge existence checks
    pub edge_filter: BloomFilter,
}

impl RelationshipSegment {
    /// Check if an edge might exist (fast probabilistic check)
    ///
    /// Returns false if the edge definitely does not exist.
    /// Returns true if the edge might exist (requires verification).
    #[inline]
    pub fn might_have_edge(&self, from_node: NodeId, to_node: NodeId) -> bool {
        self.edge_filter.might_contain_edge(from_node, to_node)
    }

    /// Add an edge to the bloom filter
    #[inline]
    pub fn add_edge_to_filter(&mut self, from_node: NodeId, to_node: NodeId) {
        self.edge_filter.insert_edge(from_node, to_node);
    }

    /// Get bloom filter statistics
    pub fn filter_stats(&self) -> BloomFilterStats {
        BloomFilterStats {
            count: self.edge_filter.count(),
            memory_bytes: self.edge_filter.memory_usage(),
            estimated_fpr: self.edge_filter.estimated_false_positive_rate(),
        }
    }
}

/// Statistics about a bloom filter
#[derive(Clone, Debug)]
pub struct BloomFilterStats {
    /// Number of items inserted
    pub count: u64,
    /// Memory usage in bytes
    pub memory_bytes: usize,
    /// Estimated false positive rate
    pub estimated_fpr: f64,
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

/// Bloom Filter for fast probabilistic existence checks
///
/// Used to quickly check if a relationship exists between two nodes
/// without requiring disk I/O. False positives are possible (≤1%),
/// but false negatives never occur.
#[derive(Clone, Debug)]
pub struct BloomFilter {
    /// Bit vector for the filter
    bits: Vec<u64>,
    /// Number of hash functions (k)
    num_hashes: u32,
    /// Total number of bits (m)
    num_bits: u64,
    /// Number of items inserted
    count: u64,
}

impl BloomFilter {
    /// Create a new Bloom filter optimized for the given capacity and false positive rate
    ///
    /// # Arguments
    /// * `expected_items` - Expected number of items to insert
    /// * `false_positive_rate` - Target false positive rate (default 0.01 = 1%)
    pub fn new(expected_items: u64, false_positive_rate: f64) -> Self {
        // Calculate optimal number of bits: m = -n * ln(p) / (ln(2)^2)
        let ln2_squared = std::f64::consts::LN_2 * std::f64::consts::LN_2;
        let num_bits =
            (-1.0 * expected_items as f64 * false_positive_rate.ln() / ln2_squared).ceil() as u64;

        // Ensure minimum size and round up to 64-bit boundary
        let num_bits = num_bits.max(64);
        let num_bits = ((num_bits + 63) / 64) * 64;

        // Calculate optimal number of hash functions: k = (m/n) * ln(2)
        let num_hashes =
            ((num_bits as f64 / expected_items as f64) * std::f64::consts::LN_2).round() as u32;
        let num_hashes = num_hashes.clamp(1, 16); // Limit hash functions for performance

        let num_words = (num_bits / 64) as usize;

        Self {
            bits: vec![0u64; num_words],
            num_hashes,
            num_bits,
            count: 0,
        }
    }

    /// Create a Bloom filter with default 1% false positive rate
    pub fn with_capacity(expected_items: u64) -> Self {
        Self::new(expected_items, 0.01)
    }

    /// Insert an item into the filter
    pub fn insert(&mut self, item: u64) {
        for i in 0..self.num_hashes {
            let hash = self.hash(item, i);
            let bit_index = (hash % self.num_bits) as usize;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;
            self.bits[word_index] |= 1u64 << bit_offset;
        }
        self.count += 1;
    }

    /// Insert a relationship edge (from_node, to_node pair)
    pub fn insert_edge(&mut self, from_node: NodeId, to_node: NodeId) {
        // Combine from_node and to_node into a single hash key
        let key = Self::edge_key(from_node, to_node);
        self.insert(key);
    }

    /// Check if an item might be in the filter
    ///
    /// Returns true if the item *might* be in the set (with false positive rate ≤1%)
    /// Returns false if the item is *definitely not* in the set
    pub fn might_contain(&self, item: u64) -> bool {
        for i in 0..self.num_hashes {
            let hash = self.hash(item, i);
            let bit_index = (hash % self.num_bits) as usize;
            let word_index = bit_index / 64;
            let bit_offset = bit_index % 64;
            if (self.bits[word_index] & (1u64 << bit_offset)) == 0 {
                return false;
            }
        }
        true
    }

    /// Check if an edge (from_node, to_node) might exist
    pub fn might_contain_edge(&self, from_node: NodeId, to_node: NodeId) -> bool {
        let key = Self::edge_key(from_node, to_node);
        self.might_contain(key)
    }

    /// Compute hash for item with given seed
    #[inline]
    fn hash(&self, item: u64, seed: u32) -> u64 {
        // Use a variant of MurmurHash3 finalizer for speed
        let mut h = item.wrapping_add((seed as u64).wrapping_mul(0x9e3779b97f4a7c15));
        h ^= h >> 33;
        h = h.wrapping_mul(0xff51afd7ed558ccd);
        h ^= h >> 33;
        h = h.wrapping_mul(0xc4ceb9fe1a85ec53);
        h ^= h >> 33;
        h
    }

    /// Compute edge key from node pair
    #[inline]
    fn edge_key(from_node: NodeId, to_node: NodeId) -> u64 {
        // Combine two 64-bit node IDs into a single 64-bit key
        // Use a mixing function to distribute bits evenly
        let h1 = from_node.wrapping_mul(0x9e3779b97f4a7c15);
        let h2 = to_node.wrapping_mul(0xc6a4a7935bd1e995);
        h1 ^ h2.rotate_left(31)
    }

    /// Get the number of items inserted
    pub fn count(&self) -> u64 {
        self.count
    }

    /// Get the memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.bits.len() * 8
    }

    /// Get the current estimated false positive rate
    pub fn estimated_false_positive_rate(&self) -> f64 {
        if self.count == 0 {
            return 0.0;
        }
        // FPR = (1 - e^(-k*n/m))^k
        let exp = (-1.0 * self.num_hashes as f64 * self.count as f64 / self.num_bits as f64).exp();
        (1.0 - exp).powi(self.num_hashes as i32)
    }

    /// Clear the filter
    pub fn clear(&mut self) {
        for word in &mut self.bits {
            *word = 0;
        }
        self.count = 0;
    }

    /// Serialize the bloom filter to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let header_size = 24; // 8 + 4 + 8 + 4 bytes
        let mut bytes = Vec::with_capacity(header_size + self.bits.len() * 8);

        // Header: num_bits (8), num_hashes (4), count (8), num_words (4)
        bytes.extend_from_slice(&self.num_bits.to_le_bytes());
        bytes.extend_from_slice(&self.num_hashes.to_le_bytes());
        bytes.extend_from_slice(&self.count.to_le_bytes());
        bytes.extend_from_slice(&(self.bits.len() as u32).to_le_bytes());

        // Bit vector
        for word in &self.bits {
            bytes.extend_from_slice(&word.to_le_bytes());
        }

        bytes
    }

    /// Deserialize a bloom filter from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() < 24 {
            return Err("Bloom filter data too short");
        }

        let num_bits = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
        let num_hashes = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let count = u64::from_le_bytes(bytes[12..20].try_into().unwrap());
        let num_words = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;

        let expected_len = 24 + num_words * 8;
        if bytes.len() < expected_len {
            return Err("Bloom filter data truncated");
        }

        let mut bits = Vec::with_capacity(num_words);
        for i in 0..num_words {
            let offset = 24 + i * 8;
            let word = u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
            bits.push(word);
        }

        Ok(Self {
            bits,
            num_hashes,
            num_bits,
            count,
        })
    }
}

impl Default for BloomFilter {
    fn default() -> Self {
        Self::with_capacity(10000) // Default capacity for 10K items
    }
}

/// Skip List for fast O(log n) traversal of large adjacency lists
///
/// Skip lists provide efficient range queries and lookups for large
/// relationship lists, avoiding O(n) scans for high-degree nodes.
#[derive(Clone, Debug)]
pub struct SkipList {
    /// Skip list levels (level 0 is the base level with all elements)
    levels: Vec<Vec<SkipListNode>>,
    /// Maximum level (height) of the skip list
    max_level: usize,
    /// Number of elements
    len: usize,
    /// Probability for level promotion (1/P)
    p_inv: u32,
}

/// A node in the skip list
#[derive(Clone, Debug)]
pub struct SkipListNode {
    /// Relationship ID (key)
    pub rel_id: u64,
    /// Target node ID for quick access
    pub target_node: NodeId,
    /// Index in the next level (for traversal)
    pub next_level_index: Option<usize>,
}

impl SkipList {
    /// Maximum number of levels (16 supports ~65K elements efficiently)
    pub const MAX_LEVELS: usize = 16;

    /// Create a new skip list with default parameters
    pub fn new() -> Self {
        Self {
            levels: vec![Vec::new()],
            max_level: 1,
            len: 0,
            p_inv: 4, // 1/4 probability of promotion
        }
    }

    /// Create a skip list with specified maximum level
    pub fn with_max_level(max_level: usize) -> Self {
        Self {
            levels: vec![Vec::new()],
            max_level: max_level.min(Self::MAX_LEVELS),
            len: 0,
            p_inv: 4,
        }
    }

    /// Insert a relationship into the skip list
    pub fn insert(&mut self, rel_id: u64, target_node: NodeId) {
        // Determine level for this node using probabilistic promotion
        let level = self.random_level();

        // Ensure we have enough levels
        while self.levels.len() <= level {
            self.levels.push(Vec::new());
        }

        // Create node for base level (and optionally higher levels)
        let base_node = SkipListNode {
            rel_id,
            target_node,
            next_level_index: None,
        };

        // Find insertion position in base level (binary search since sorted)
        let base_pos = self.levels[0]
            .binary_search_by_key(&rel_id, |n| n.rel_id)
            .unwrap_or_else(|x| x);

        // Insert at base level
        self.levels[0].insert(base_pos, base_node);

        // Insert at higher levels with index tracking
        let mut prev_index = base_pos;
        for l in 1..=level {
            if l >= self.levels.len() {
                break;
            }

            let node = SkipListNode {
                rel_id,
                target_node,
                next_level_index: Some(prev_index),
            };

            let pos = self.levels[l]
                .binary_search_by_key(&rel_id, |n| n.rel_id)
                .unwrap_or_else(|x| x);

            self.levels[l].insert(pos, node);
            prev_index = pos;
        }

        self.len += 1;
    }

    /// Find a relationship by ID in O(log n) time
    pub fn find(&self, rel_id: u64) -> Option<&SkipListNode> {
        if self.levels.is_empty() || self.levels[0].is_empty() {
            return None;
        }

        // Start from top level and work down
        let top_level = self.levels.len() - 1;
        let mut current_level = top_level;
        let mut pos = 0;

        loop {
            let level = &self.levels[current_level];

            // Binary search at current level
            match level[pos..].binary_search_by_key(&rel_id, |n| n.rel_id) {
                Ok(found) => {
                    // Found at this level, descend to base for exact node
                    let node = &level[pos + found];
                    if current_level == 0 {
                        return Some(node);
                    }
                    // Follow down pointer
                    if let Some(next_idx) = node.next_level_index {
                        current_level -= 1;
                        pos = next_idx;
                    } else {
                        current_level -= 1;
                    }
                }
                Err(insert_pos) => {
                    if current_level == 0 {
                        // Not found at base level
                        return None;
                    }
                    // Go down a level
                    if insert_pos > 0 {
                        // Use the previous node's down pointer
                        if let Some(next_idx) = level[pos + insert_pos - 1].next_level_index {
                            pos = next_idx;
                        }
                    }
                    current_level -= 1;
                }
            }
        }
    }

    /// Range query: find all relationships in [start_id, end_id]
    pub fn range(&self, start_id: u64, end_id: u64) -> Vec<&SkipListNode> {
        if self.levels.is_empty() || self.levels[0].is_empty() {
            return Vec::new();
        }

        // Find start position using skip list traversal
        let start_pos = self.levels[0]
            .binary_search_by_key(&start_id, |n| n.rel_id)
            .unwrap_or_else(|x| x);

        // Collect all elements in range
        let mut result = Vec::new();
        for node in &self.levels[0][start_pos..] {
            if node.rel_id > end_id {
                break;
            }
            result.push(node);
        }

        result
    }

    /// Get all relationships to a specific target node
    pub fn find_by_target(&self, target_node: NodeId) -> Vec<&SkipListNode> {
        // This requires a linear scan since we're not indexed by target
        // For frequent target lookups, consider a secondary index
        self.levels[0]
            .iter()
            .filter(|n| n.target_node == target_node)
            .collect()
    }

    /// Get the number of elements
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Get all elements in sorted order
    pub fn iter(&self) -> impl Iterator<Item = &SkipListNode> {
        self.levels[0].iter()
    }

    /// Generate random level using geometric distribution
    fn random_level(&self) -> usize {
        let mut level = 0;
        // Simple PRNG-based level selection
        let mut x = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64)
            .wrapping_mul(0x9e3779b97f4a7c15);

        while level < self.max_level - 1 {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;

            if (x % self.p_inv as u64) != 0 {
                break;
            }
            level += 1;
        }
        level
    }

    /// Get memory usage estimate
    pub fn memory_usage(&self) -> usize {
        let node_size = std::mem::size_of::<SkipListNode>();
        self.levels.iter().map(|l| l.len() * node_size).sum()
    }

    /// Get skip list statistics
    pub fn stats(&self) -> SkipListStats {
        SkipListStats {
            len: self.len,
            levels: self.levels.len(),
            memory_bytes: self.memory_usage(),
            avg_level_size: if self.levels.is_empty() {
                0.0
            } else {
                self.levels.iter().map(|l| l.len()).sum::<usize>() as f64 / self.levels.len() as f64
            },
        }
    }

    /// Build a skip list from a sorted iterator of (rel_id, target_node) pairs
    pub fn from_sorted_iter<I>(iter: I) -> Self
    where
        I: Iterator<Item = (u64, NodeId)>,
    {
        let mut skip_list = Self::new();
        for (rel_id, target_node) in iter {
            skip_list.insert(rel_id, target_node);
        }
        skip_list
    }
}

impl Default for SkipList {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about a skip list
#[derive(Clone, Debug)]
pub struct SkipListStats {
    /// Number of elements
    pub len: usize,
    /// Number of levels
    pub levels: usize,
    /// Memory usage in bytes
    pub memory_bytes: usize,
    /// Average elements per level
    pub avg_level_size: f64,
}

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

    #[test]
    fn test_bloom_filter_basic() {
        let mut filter = BloomFilter::with_capacity(1000);

        // Insert some items
        filter.insert(42);
        filter.insert(100);
        filter.insert(999);

        // Check that inserted items are found
        assert!(filter.might_contain(42));
        assert!(filter.might_contain(100));
        assert!(filter.might_contain(999));

        // Check that non-inserted items are (mostly) not found
        // Note: There might be false positives, but probability is low
        let mut false_positives = 0;
        for i in 1000..2000 {
            if filter.might_contain(i) {
                false_positives += 1;
            }
        }
        // With 1% FPR and 1000 checks, expect ~10 false positives
        assert!(
            false_positives < 50,
            "Too many false positives: {}",
            false_positives
        );
    }

    #[test]
    fn test_bloom_filter_edges() {
        let mut filter = BloomFilter::with_capacity(10000);

        // Insert some edges
        filter.insert_edge(1, 2);
        filter.insert_edge(2, 3);
        filter.insert_edge(1, 3);

        // Check that inserted edges are found
        assert!(filter.might_contain_edge(1, 2));
        assert!(filter.might_contain_edge(2, 3));
        assert!(filter.might_contain_edge(1, 3));

        // Check that non-inserted edges are (mostly) not found
        assert!(!filter.might_contain_edge(3, 1)); // Direction matters
        assert!(!filter.might_contain_edge(100, 200));
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut filter = BloomFilter::with_capacity(1000);
        filter.insert(42);
        filter.insert(100);
        filter.insert_edge(1, 2);

        // Serialize and deserialize
        let bytes = filter.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes).unwrap();

        // Check that restored filter works correctly
        assert!(restored.might_contain(42));
        assert!(restored.might_contain(100));
        assert!(restored.might_contain_edge(1, 2));
        assert_eq!(restored.count(), filter.count());
    }

    #[test]
    fn test_bloom_filter_false_positive_rate() {
        // Create filter with specific FPR
        let filter = BloomFilter::new(10000, 0.001); // 0.1% FPR

        // Initial FPR should be 0
        assert_eq!(filter.estimated_false_positive_rate(), 0.0);

        // Memory usage should be reasonable
        let mem = filter.memory_usage();
        // For 10K items at 0.1% FPR: m ≈ 143,776 bits ≈ 18KB
        assert!(mem < 100_000, "Memory usage too high: {}", mem);
    }

    #[test]
    fn test_skip_list_basic() {
        let mut skip_list = SkipList::new();

        // Insert some elements
        skip_list.insert(10, 100);
        skip_list.insert(20, 200);
        skip_list.insert(15, 150);
        skip_list.insert(5, 50);
        skip_list.insert(25, 250);

        assert_eq!(skip_list.len(), 5);

        // Find elements
        let found = skip_list.find(15).unwrap();
        assert_eq!(found.rel_id, 15);
        assert_eq!(found.target_node, 150);

        let found = skip_list.find(5).unwrap();
        assert_eq!(found.rel_id, 5);
        assert_eq!(found.target_node, 50);

        // Not found
        assert!(skip_list.find(99).is_none());
    }

    #[test]
    fn test_skip_list_range_query() {
        let mut skip_list = SkipList::new();

        // Insert elements
        for i in 0..100 {
            skip_list.insert(i, i * 10);
        }

        // Range query [20, 30]
        let range = skip_list.range(20, 30);
        assert_eq!(range.len(), 11); // 20, 21, ..., 30
        assert_eq!(range[0].rel_id, 20);
        assert_eq!(range[10].rel_id, 30);

        // Range query [50, 60]
        let range = skip_list.range(50, 60);
        assert_eq!(range.len(), 11);

        // Empty range
        let range = skip_list.range(1000, 2000);
        assert!(range.is_empty());
    }

    #[test]
    fn test_skip_list_sorted_order() {
        let mut skip_list = SkipList::new();

        // Insert out of order
        skip_list.insert(50, 500);
        skip_list.insert(10, 100);
        skip_list.insert(30, 300);
        skip_list.insert(20, 200);
        skip_list.insert(40, 400);

        // Iterate should be sorted
        let ids: Vec<u64> = skip_list.iter().map(|n| n.rel_id).collect();
        assert_eq!(ids, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_skip_list_find_by_target() {
        let mut skip_list = SkipList::new();

        // Insert with some duplicate targets
        skip_list.insert(1, 100);
        skip_list.insert(2, 200);
        skip_list.insert(3, 100); // Same target as rel 1
        skip_list.insert(4, 300);
        skip_list.insert(5, 100); // Same target as rel 1 and 3

        // Find by target
        let by_target = skip_list.find_by_target(100);
        assert_eq!(by_target.len(), 3);
        assert!(by_target.iter().all(|n| n.target_node == 100));
    }

    #[test]
    fn test_skip_list_large() {
        let mut skip_list = SkipList::with_max_level(8);

        // Insert many elements
        let n = 10000;
        for i in 0..n {
            skip_list.insert(i as u64, (i * 10) as u64);
        }

        assert_eq!(skip_list.len(), n);

        // Find all elements
        for i in 0..n {
            let found = skip_list.find(i as u64);
            assert!(found.is_some(), "Element {} not found", i);
            assert_eq!(found.unwrap().rel_id, i as u64);
        }

        // Check stats
        let stats = skip_list.stats();
        assert_eq!(stats.len, n);
        assert!(stats.levels > 1); // Should have multiple levels
        assert!(stats.memory_bytes > 0);
    }

    #[test]
    fn test_skip_list_from_sorted() {
        let data: Vec<(u64, u64)> = (0..100).map(|i| (i, i * 10)).collect();
        let skip_list = SkipList::from_sorted_iter(data.into_iter());

        assert_eq!(skip_list.len(), 100);
        assert!(skip_list.find(50).is_some());
    }
}
