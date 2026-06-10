/// Size of adjacency list header (20 bytes: node_id(8) + count(4) + type_id(4) + total_size(4))
/// Note: Packed struct, no padding
pub(super) const ADJACENCY_HEADER_SIZE: usize = std::mem::size_of::<AdjacencyListHeader>();

/// Adjacency list entry (16 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct AdjacencyEntry {
    /// Target node ID (for outgoing) or source node ID (for incoming)
    pub node_id: u64,
    /// Relationship ID
    pub rel_id: u64,
}

unsafe impl bytemuck::Pod for AdjacencyEntry {}
unsafe impl bytemuck::Zeroable for AdjacencyEntry {}

/// Adjacency list header
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct AdjacencyListHeader {
    /// Node ID that owns this adjacency list
    pub node_id: u64,
    /// Number of entries in this list
    pub count: u32,
    /// Relationship type ID (0 = all types, or specific type for filtered lists)
    pub type_id: u32,
    /// Total size of this adjacency list (header + entries)
    pub total_size: u32,
}

unsafe impl bytemuck::Pod for AdjacencyListHeader {}
unsafe impl bytemuck::Zeroable for AdjacencyListHeader {}
