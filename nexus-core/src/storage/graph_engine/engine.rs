//! Core Graph Storage Engine Implementation
//!
//! This module implements the main GraphStorageEngine that provides
//! high-performance graph storage operations optimized for relationships.

use super::compression::RelationshipCompressor;
use super::format::*;
use super::io::DirectFile;
use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Core graph storage engine optimized for relationship operations
pub struct GraphStorageEngine {
    /// Memory-mapped file for data storage
    mmap: MmapMut,
    /// File handle for growth operations
    file: File,
    /// Current storage layout
    layout: StorageLayout,
    /// Next available node ID
    next_node_id: AtomicU64,
    /// Next available relationship ID
    next_rel_id: AtomicU64,
    /// Relationship compressor for adjacency lists
    compressor: RelationshipCompressor,
    /// Path to the storage file
    file_path: std::path::PathBuf,
}

impl GraphStorageEngine {
    /// Create a new graph storage engine at the specified path
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Create or open the file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)?;

        // Initialize with larger size to accommodate relationships and indices
        let node_space = INITIAL_NODE_CAPACITY as u64 * NodeRecord::SIZE as u64;
        let rel_space = INITIAL_REL_CAPACITY as u64 * RelationshipRecord::SIZE as u64;
        let index_space = 10000 * AdjacencyIndexEntry::SIZE as u64; // Estimate for indices
        let initial_size = GraphHeader::SIZE as u64 + node_space + rel_space + index_space;
        file.set_len(initial_size)?;

        // Create memory mapping
        let mut mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        // Initialize header
        let header = GraphHeader::new();
        let header_bytes = header.to_bytes();
        mmap[0..GraphHeader::SIZE].copy_from_slice(&header_bytes);

        // Initialize layout
        let mut layout = StorageLayout::new();
        let node_start = GraphHeader::SIZE as u64;
        let node_end = node_start + INITIAL_NODE_CAPACITY as u64 * NodeRecord::SIZE as u64;
        layout.nodes = node_start..node_end;

        // Set free space after nodes
        layout.free_space = node_end..initial_size;

        Ok(Self {
            mmap,
            file,
            layout,
            next_node_id: AtomicU64::new(0),
            next_rel_id: AtomicU64::new(0),
            compressor: RelationshipCompressor::new(),
            file_path: path.to_path_buf(),
        })
    }

    /// Open an existing graph storage engine
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        // Open existing file
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        // Create memory mapping
        let mmap = unsafe { MmapOptions::new().map_mut(&file)? };

        // Read and validate header
        let header_bytes = &mmap[0..GraphHeader::SIZE];
        let header = GraphHeader::from_bytes(header_bytes)
            .map_err(|e| Error::Storage(format!("Failed to read header: {}", e)))?;

        if !header.is_valid() {
            return Err(Error::Storage("Invalid graph file header".to_string()));
        }

        // TODO: Reconstruct layout from file (simplified for now)
        let layout = StorageLayout::new();

        // TODO: Reconstruct next IDs from file
        let next_node_id = AtomicU64::new(0);
        let next_rel_id = AtomicU64::new(0);

        Ok(Self {
            mmap,
            file,
            layout,
            next_node_id,
            next_rel_id,
            compressor: RelationshipCompressor::new(),
            file_path: path.to_path_buf(),
        })
    }

    /// Create a new node and return its ID
    pub fn create_node(&mut self, label_id: u32) -> Result<NodeId> {
        let node_id = self.next_node_id.fetch_add(1, Ordering::SeqCst);

        // Ensure we have space for the node
        self.ensure_node_capacity(node_id)?;

        // Create node record
        let node = NodeRecord::new(node_id, label_id);

        // Write to mmap storage
        self.write_node_record(node_id, &node)?;

        Ok(node_id)
    }

    /// Create a relationship between two nodes
    pub fn create_relationship(
        &mut self,
        from_node: NodeId,
        to_node: NodeId,
        type_id: TypeId,
    ) -> Result<u64> {
        let rel_id = self.next_rel_id.fetch_add(1, Ordering::SeqCst);

        // Ensure we have space for the relationship
        self.ensure_relationship_capacity(type_id, rel_id)?;

        // Create relationship record
        let relationship = RelationshipRecord::new(rel_id, from_node, to_node, type_id);

        // Write to mmap storage
        self.write_relationship_record(type_id, rel_id, &relationship)?;

        // Update adjacency lists
        self.update_adjacency_lists(from_node, to_node, type_id, rel_id)?;

        // Update node relationship pointers
        self.update_node_relationship_pointers(from_node, to_node, rel_id)?;

        Ok(rel_id)
    }

    /// Read a node by ID
    pub fn read_node(&self, node_id: NodeId) -> Result<NodeRecord> {
        let offset = self.node_offset(node_id);

        if offset + NodeRecord::SIZE as u64 > self.layout.nodes.end {
            return Err(Error::NotFound(format!("Node {} not found", node_id)));
        }

        let start = offset as usize;
        let end = start + NodeRecord::SIZE;
        let bytes = &self.mmap[start..end];

        NodeRecord::from_bytes(bytes)
            .map_err(|e| Error::Storage(format!("Failed to read node: {}", e)))
    }

    /// Read a relationship by type and ID with optimized sequential access
    pub fn read_relationship(&self, type_id: TypeId, rel_id: u64) -> Result<RelationshipRecord> {
        let segment =
            self.layout.relationships.get(&type_id).ok_or_else(|| {
                Error::NotFound(format!("Relationship type {} not found", type_id))
            })?;

        let offset = segment.data_range.start + rel_id * RelationshipRecord::SIZE as u64;

        if offset + RelationshipRecord::SIZE as u64 > segment.data_range.end {
            return Err(Error::NotFound(format!(
                "Relationship {} not found in type {}",
                rel_id, type_id
            )));
        }

        // Optimized sequential read with prefetch hint
        let start = offset as usize;
        let end = start + RelationshipRecord::SIZE;

        // Prefetch adjacent relationships for sequential access patterns
        self.prefetch_relationships(type_id, rel_id);

        let bytes = &self.mmap[start..end];

        RelationshipRecord::from_bytes(bytes)
            .map_err(|e| Error::Storage(format!("Failed to read relationship: {}", e)))
    }

    /// Prefetch adjacent relationships for sequential access optimization
    fn prefetch_relationships(&self, type_id: TypeId, current_rel_id: u64) {
        if let Some(segment) = self.layout.relationships.get(&type_id) {
            // Prefetch next 4 relationships (64KB ahead if relationship size is 64 bytes)
            let prefetch_count = 4;
            for i in 1..=prefetch_count {
                let prefetch_rel_id = current_rel_id + i;
                let prefetch_offset =
                    segment.data_range.start + prefetch_rel_id * RelationshipRecord::SIZE as u64;

                if prefetch_offset + RelationshipRecord::SIZE as u64 <= segment.data_range.end {
                    // Use prefetch intrinsic if available (helps with sequential scans)
                    #[cfg(target_arch = "x86_64")]
                    unsafe {
                        use std::arch::x86_64::*;
                        // Prefetch data into L1 cache with high locality
                        _mm_prefetch(
                            self.mmap.as_ptr().add(prefetch_offset as usize) as *const i8,
                            _MM_HINT_T0,
                        );
                    }
                } else {
                    break;
                }
            }
        }
    }

    /// Flush all pending changes to disk
    pub fn flush(&mut self) -> Result<()> {
        // Persist adjacency indices to mmap
        self.persist_adjacency_indices()?;

        // Flush the memory map
        self.mmap
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush graph storage: {}", e)))
    }

    /// Persist adjacency indices to memory-mapped storage
    fn persist_adjacency_indices(&mut self) -> Result<()> {
        let type_ids: Vec<TypeId> = self.layout.relationships.keys().cloned().collect();

        for type_id in type_ids {
            if let Some(segment) = self.layout.relationships.get(&type_id) {
                // Clone indices to avoid borrowing issues
                let outgoing_index = segment.outgoing_index.clone();
                let incoming_index = segment.incoming_index.clone();

                // Persist outgoing index
                self.persist_index(&outgoing_index)?;

                // Persist incoming index
                self.persist_index(&incoming_index)?;
            }
        }
        Ok(())
    }

    /// Persist a single adjacency index
    fn persist_index(&mut self, index: &AdjacencyIndex) -> Result<()> {
        let mut offset = index.base_offset;

        for entry in index.entries.values() {
            if offset + AdjacencyIndexEntry::SIZE as u64 > self.mmap.len() as u64 {
                // Need to grow the file
                self.grow_file_and_remap((offset + AdjacencyIndexEntry::SIZE as u64) as u64)?;
            }

            let entry_bytes = entry.to_bytes();
            let start = offset as usize;
            let end = start + AdjacencyIndexEntry::SIZE;
            self.mmap[start..end].copy_from_slice(&entry_bytes);

            offset += AdjacencyIndexEntry::SIZE as u64;
        }

        Ok(())
    }

    /// Read multiple relationships sequentially with optimized access
    pub fn read_relationships_sequential(
        &self,
        type_id: TypeId,
        start_rel_id: u64,
        count: usize,
    ) -> Result<Vec<RelationshipRecord>> {
        let segment =
            self.layout.relationships.get(&type_id).ok_or_else(|| {
                Error::NotFound(format!("Relationship type {} not found", type_id))
            })?;

        let mut results = Vec::with_capacity(count);

        // Bulk prefetch for sequential access
        let prefetch_ahead = 16; // Prefetch 16 relationships ahead
        for i in 0..std::cmp::min(
            count + prefetch_ahead,
            (segment.data_range.end - segment.data_range.start) as usize / RelationshipRecord::SIZE,
        ) {
            let rel_id = start_rel_id + i as u64;
            let offset = segment.data_range.start + rel_id * RelationshipRecord::SIZE as u64;

            if offset + RelationshipRecord::SIZE as u64 > segment.data_range.end {
                break;
            }

            #[cfg(target_arch = "x86_64")]
            unsafe {
                use std::arch::x86_64::*;
                // Prefetch with medium temporal locality for sequential scans
                _mm_prefetch(
                    self.mmap.as_ptr().add(offset as usize) as *const i8,
                    _MM_HINT_T1,
                );
            }
        }

        // Sequential read with optimized memory access
        for i in 0..count {
            let rel_id = start_rel_id + i as u64;
            let offset = segment.data_range.start + rel_id * RelationshipRecord::SIZE as u64;

            if offset + RelationshipRecord::SIZE as u64 > segment.data_range.end {
                break;
            }

            let start = offset as usize;
            let end = start + RelationshipRecord::SIZE;
            let bytes = &self.mmap[start..end];

            if let Ok(rel) = RelationshipRecord::from_bytes(bytes) {
                results.push(rel);
            }
        }

        Ok(results)
    }

    /// Get outgoing relationships for a node with optimized access
    pub fn get_outgoing_relationships(
        &self,
        node_id: NodeId,
        type_id: TypeId,
    ) -> Result<Vec<RelationshipRecord>> {
        let mut results = Vec::new();

        if let Some(segment) = self.layout.relationships.get(&type_id) {
            // Use the adjacency index for fast lookup
            if let Some(index_entry) = segment.outgoing_index.get_entry(node_id) {
                // Decompress adjacency list from mmap
                if index_entry.list_offset > 0 && index_entry.count > 0 {
                    // Calculate the size of compressed data (estimate based on compression type)
                    let max_compressed_size = match segment.compression {
                        CompressionType::None => {
                            index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>()
                        }
                        CompressionType::VarInt | CompressionType::Delta => {
                            // VarInt/Delta: average 3-5 bytes per entry, use 10 for safety
                            index_entry.count as usize * 10
                        }
                        CompressionType::Dictionary => {
                            // Dictionary: estimate 4 bytes per entry
                            index_entry.count as usize * 4
                        }
                        CompressionType::LZ4 => {
                            // LZ4: typically 50-80% of original size
                            (index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>() * 3)
                                / 4
                        }
                        CompressionType::Zstd => {
                            // Zstd: typically 30-60% of original size
                            (index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>() * 2)
                                / 5
                        }
                        CompressionType::Adaptive | CompressionType::SimdRLE => {
                            // Adaptive/SIMD RLE: variable compression, use conservative estimate
                            index_entry.count as usize * 6
                        }
                    };

                    // Read compressed data from mmap
                    let offset = index_entry.list_offset as usize;
                    let end = std::cmp::min(
                        offset + max_compressed_size,
                        segment.adjacency_data_range.end as usize,
                    );

                    if offset < self.mmap.len() && end <= self.mmap.len() {
                        let compressed_data = &self.mmap[offset..end];

                        // Decompress the adjacency list
                        match self.compressor.decompress_adjacency_list(
                            compressed_data,
                            segment.compression,
                            index_entry.count as usize,
                        ) {
                            Ok(adjacency_entries) => {
                                // Optimize sequential access for multiple relationships
                                if adjacency_entries.len() > 3 {
                                    // Check if relationships are mostly sequential
                                    let mut sorted_entries = adjacency_entries.clone();
                                    sorted_entries.sort_by_key(|e| e.rel_id);

                                    let mut sequential_count = 1;
                                    for i in 1..sorted_entries.len() {
                                        if sorted_entries[i].rel_id
                                            == sorted_entries[i - 1].rel_id + 1
                                        {
                                            sequential_count += 1;
                                        }
                                    }

                                    if sequential_count > sorted_entries.len() / 2 {
                                        // Mostly sequential - use optimized sequential read
                                        if let Ok(seq_rels) = self.read_relationships_sequential(
                                            type_id,
                                            sorted_entries[0].rel_id,
                                            sorted_entries.len(),
                                        ) {
                                            // Filter to only include requested relationships
                                            let requested_ids: std::collections::HashSet<u64> =
                                                adjacency_entries
                                                    .iter()
                                                    .map(|e| e.rel_id)
                                                    .collect();
                                            for rel in seq_rels {
                                                if requested_ids.contains(&rel.id) {
                                                    results.push(rel);
                                                }
                                            }
                                        } else {
                                            // Fallback to individual reads
                                            for entry in adjacency_entries {
                                                if let Ok(rel) =
                                                    self.read_relationship(type_id, entry.rel_id)
                                                {
                                                    results.push(rel);
                                                }
                                            }
                                        }
                                    } else {
                                        // Not sequential - read individually with prefetch
                                        for entry in adjacency_entries {
                                            if let Ok(rel) =
                                                self.read_relationship(type_id, entry.rel_id)
                                            {
                                                results.push(rel);
                                            }
                                        }
                                    }
                                } else {
                                    // Small number - read individually
                                    for entry in adjacency_entries {
                                        if let Ok(rel) =
                                            self.read_relationship(type_id, entry.rel_id)
                                        {
                                            results.push(rel);
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                // If decompression fails, fall back to scan for safety
                                tracing::warn!(
                                    "Failed to decompress adjacency list for node {}: {}. Falling back to scan.",
                                    node_id,
                                    e
                                );
                                return self.get_outgoing_relationships_fallback(node_id, type_id);
                            }
                        }
                    } else {
                        // Invalid offset/size, fall back to scan
                        return self.get_outgoing_relationships_fallback(node_id, type_id);
                    }
                } else {
                    // No compressed list stored yet, fall back to scan
                    return self.get_outgoing_relationships_fallback(node_id, type_id);
                }
            }
        }

        Ok(results)
    }

    /// Get incoming relationships for a node
    pub fn get_incoming_relationships(
        &self,
        node_id: NodeId,
        type_id: TypeId,
    ) -> Result<Vec<RelationshipRecord>> {
        let mut results = Vec::new();

        if let Some(segment) = self.layout.relationships.get(&type_id) {
            // Use the adjacency index for fast lookup
            if let Some(index_entry) = segment.incoming_index.get_entry(node_id) {
                // Decompress adjacency list from mmap
                if index_entry.list_offset > 0 && index_entry.count > 0 {
                    // Calculate the size of compressed data (estimate based on compression type)
                    let max_compressed_size = match segment.compression {
                        CompressionType::None => {
                            index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>()
                        }
                        CompressionType::VarInt | CompressionType::Delta => {
                            // VarInt/Delta: average 3-5 bytes per entry, use 10 for safety
                            index_entry.count as usize * 10
                        }
                        CompressionType::Dictionary => {
                            // Dictionary: estimate 4 bytes per entry
                            index_entry.count as usize * 4
                        }
                        CompressionType::LZ4 => {
                            // LZ4: typically 50-80% of original size
                            (index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>() * 3)
                                / 4
                        }
                        CompressionType::Zstd => {
                            // Zstd: typically 30-60% of original size
                            (index_entry.count as usize * std::mem::size_of::<AdjacencyEntry>() * 2)
                                / 5
                        }
                        CompressionType::Adaptive | CompressionType::SimdRLE => {
                            // Adaptive/SIMD RLE: variable compression, use conservative estimate
                            index_entry.count as usize * 6
                        }
                    };

                    // Read compressed data from mmap
                    let offset = index_entry.list_offset as usize;
                    let end = std::cmp::min(
                        offset + max_compressed_size,
                        segment.adjacency_data_range.end as usize,
                    );

                    if offset < self.mmap.len() && end <= self.mmap.len() {
                        let compressed_data = &self.mmap[offset..end];

                        // Decompress the adjacency list
                        match self.compressor.decompress_adjacency_list(
                            compressed_data,
                            segment.compression,
                            index_entry.count as usize,
                        ) {
                            Ok(adjacency_entries) => {
                                // Read each relationship record directly
                                for entry in adjacency_entries {
                                    if let Ok(rel) = self.read_relationship(type_id, entry.rel_id) {
                                        results.push(rel);
                                    }
                                }
                            }
                            Err(e) => {
                                // If decompression fails, fall back to scan for safety
                                tracing::warn!(
                                    "Failed to decompress adjacency list for node {}: {}. Falling back to scan.",
                                    node_id,
                                    e
                                );
                                return self.get_incoming_relationships_fallback(node_id, type_id);
                            }
                        }
                    } else {
                        // Invalid offset/size, fall back to scan
                        return self.get_incoming_relationships_fallback(node_id, type_id);
                    }
                } else {
                    // No compressed list stored yet, fall back to scan
                    return self.get_incoming_relationships_fallback(node_id, type_id);
                }
            }
        }

        Ok(results)
    }

    /// Fallback: Get outgoing relationships by scanning (used when decompression fails)
    fn get_outgoing_relationships_fallback(
        &self,
        node_id: NodeId,
        type_id: TypeId,
    ) -> Result<Vec<RelationshipRecord>> {
        let mut results = Vec::new();

        if let Some(segment) = self.layout.relationships.get(&type_id) {
            let max_scan = std::cmp::min(
                self.next_rel_id.load(Ordering::SeqCst),
                segment.count + 1000,
            );

            for rel_id in 0..max_scan {
                if let Ok(rel) = self.read_relationship(type_id, rel_id) {
                    if rel.from_node == node_id && rel.type_id == type_id {
                        results.push(rel);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Fallback: Get incoming relationships by scanning (used when decompression fails)
    fn get_incoming_relationships_fallback(
        &self,
        node_id: NodeId,
        type_id: TypeId,
    ) -> Result<Vec<RelationshipRecord>> {
        let mut results = Vec::new();

        if let Some(segment) = self.layout.relationships.get(&type_id) {
            let max_scan = std::cmp::min(
                self.next_rel_id.load(Ordering::SeqCst),
                segment.count + 1000,
            );

            for rel_id in 0..max_scan {
                if let Ok(rel) = self.read_relationship(type_id, rel_id) {
                    if rel.to_node == node_id && rel.type_id == type_id {
                        results.push(rel);
                    }
                }
            }
        }

        Ok(results)
    }

    /// Get statistics about the storage
    pub fn stats(&self) -> StorageStats {
        StorageStats {
            node_count: self.next_node_id.load(Ordering::SeqCst),
            relationship_count: self.next_rel_id.load(Ordering::SeqCst),
            file_size: self.layout.total_size(),
            relationship_types: self.layout.relationships.len(),
        }
    }

    // Internal helper methods

    fn node_offset(&self, node_id: NodeId) -> u64 {
        self.layout.nodes.start + node_id * NodeRecord::SIZE as u64
    }

    fn ensure_node_capacity(&mut self, node_id: NodeId) -> Result<()> {
        let required_offset = self.node_offset(node_id) + NodeRecord::SIZE as u64;

        if required_offset > self.layout.nodes.end {
            self.grow_node_segment(required_offset)?;
        }

        Ok(())
    }

    fn ensure_relationship_capacity(&mut self, type_id: TypeId, rel_id: u64) -> Result<()> {
        // Check if segment exists, if not create it
        if !self.layout.relationships.contains_key(&type_id) {
            let segment = self.create_relationship_segment(type_id);
            self.layout.relationships.insert(type_id, segment);
        }

        let segment = self.layout.relationships.get(&type_id).unwrap();
        let required_offset =
            segment.data_range.start + (rel_id + 1) * RelationshipRecord::SIZE as u64;

        if required_offset > segment.data_range.end {
            self.grow_relationship_segment(type_id, required_offset)?;
        }

        Ok(())
    }

    fn create_relationship_segment(&mut self, type_id: TypeId) -> RelationshipSegment {
        // Allocate space for relationship data
        let rel_segment_size = INITIAL_REL_CAPACITY as u64 * RelationshipRecord::SIZE as u64;
        let rel_segment_start = self.allocate_space(rel_segment_size);

        // Allocate space for adjacency indices (estimate based on nodes)
        let index_estimate = 1000; // Assume ~1000 nodes initially
        let index_size = index_estimate * AdjacencyIndexEntry::SIZE as u64;
        let outgoing_index_start = self.allocate_space(index_size);
        let incoming_index_start = self.allocate_space(index_size);

        // Allocate space for compressed adjacency lists
        let adjacency_data_size = INITIAL_REL_CAPACITY as u64 * 8; // Estimate 8 bytes per relationship
        let adjacency_data_start = self.allocate_space(adjacency_data_size);

        RelationshipSegment {
            type_id,
            data_range: rel_segment_start..(rel_segment_start + rel_segment_size),
            outgoing_index: AdjacencyIndex::new(outgoing_index_start),
            incoming_index: AdjacencyIndex::new(incoming_index_start),
            adjacency_data_range: adjacency_data_start
                ..(adjacency_data_start + adjacency_data_size),
            count: 0,
            compression: CompressionType::VarInt,
        }
    }

    fn allocate_space(&mut self, size: u64) -> u64 {
        let start = self.layout.free_space.start;

        // Ensure we have enough space in the file
        let required_end = start + size;
        if required_end > self.mmap.len() as u64 {
            // Need to grow the file
            self.grow_file_and_remap(required_end).unwrap();
        }

        self.layout.free_space.start += size;
        start
    }

    fn grow_node_segment(&mut self, required_size: u64) -> Result<()> {
        let current_size = self.layout.nodes.end - self.layout.nodes.start;
        let required_capacity = required_size - self.layout.nodes.start;

        if required_capacity > current_size {
            let growth_size = (required_capacity as f64 * FILE_GROWTH_FACTOR) as u64;
            let new_end = self.layout.nodes.start + growth_size.max(MIN_GROWTH_SIZE);

            // Update layout
            self.layout.nodes.end = new_end;

            // Grow file and remap
            self.grow_file_and_remap(new_end)?;
        }

        Ok(())
    }

    fn grow_relationship_segment(&mut self, type_id: TypeId, required_size: u64) -> Result<()> {
        if let Some(segment) = self.layout.relationships.get_mut(&type_id) {
            let current_size = segment.data_range.end - segment.data_range.start;
            let required_capacity = required_size - segment.data_range.start;

            if required_capacity > current_size {
                let growth_size = (required_capacity as f64 * FILE_GROWTH_FACTOR) as u64;
                let new_end = segment.data_range.start + growth_size.max(MIN_GROWTH_SIZE);

                // Update segment
                segment.data_range.end = new_end;

                // Grow file and remap
                self.grow_file_and_remap(new_end)?;
            }
        }

        Ok(())
    }

    fn grow_file_and_remap(&mut self, minimum_size: u64) -> Result<()> {
        let current_size = self.file.metadata()?.len();
        let new_size = minimum_size.max((current_size as f64 * FILE_GROWTH_FACTOR) as u64);

        // Grow file
        self.file.set_len(new_size)?;

        // Remap memory
        self.mmap = unsafe { MmapOptions::new().map_mut(&self.file)? };

        Ok(())
    }

    fn write_node_record(&mut self, node_id: NodeId, record: &NodeRecord) -> Result<()> {
        let offset = self.node_offset(node_id) as usize;
        let end = offset + NodeRecord::SIZE;

        if end > self.mmap.len() {
            return Err(Error::Storage(
                "Node record extends beyond mapped memory".to_string(),
            ));
        }

        let record_bytes = record.to_bytes();
        self.mmap[offset..end].copy_from_slice(&record_bytes);

        Ok(())
    }

    fn write_relationship_record(
        &mut self,
        type_id: TypeId,
        rel_id: u64,
        record: &RelationshipRecord,
    ) -> Result<()> {
        let segment = self
            .layout
            .relationships
            .get(&type_id)
            .ok_or_else(|| Error::Storage("Relationship segment not found".to_string()))?;

        let offset = segment.data_range.start + rel_id * RelationshipRecord::SIZE as u64;
        let offset_usize = offset as usize;
        let end = offset_usize + RelationshipRecord::SIZE;

        if end > self.mmap.len() {
            return Err(Error::Storage(
                "Relationship record extends beyond mapped memory".to_string(),
            ));
        }

        let record_bytes = record.to_bytes();
        self.mmap[offset_usize..end].copy_from_slice(&record_bytes);

        Ok(())
    }

    fn update_adjacency_lists(
        &mut self,
        from_node: NodeId,
        to_node: NodeId,
        type_id: TypeId,
        rel_id: u64,
    ) -> Result<()> {
        // Ensure relationship segment exists
        if !self.layout.relationships.contains_key(&type_id) {
            let segment = self.create_relationship_segment(type_id);
            self.layout.relationships.insert(type_id, segment);
        }

        let segment = self.layout.relationships.get_mut(&type_id).unwrap();

        // Update outgoing adjacency index
        segment.outgoing_index.add_relationship(from_node, rel_id);

        // Update incoming adjacency  index
        segment.incoming_index.add_relationship(to_node, rel_id);

        segment.count += 1;

        // Store compressed adjacency lists in mmap periodically
        // For performance, we batch compress/store every 100 relationships
        if segment.count % 100 == 0 {
            self.compress_and_store_adjacency_lists(type_id)?;
        }

        Ok(())
    }

    /// Compress and store adjacency lists for a relationship type
    fn compress_and_store_adjacency_lists(&mut self, type_id: TypeId) -> Result<()> {
        // Collect all data we need before mutable borrows
        let (node_ids, compression_type, initial_offset, adjacency_range_end) = {
            let segment = self.layout.relationships.get(&type_id).ok_or_else(|| {
                Error::Storage(format!(
                    "Relationship segment not found for type {}",
                    type_id
                ))
            })?;

            let node_ids: Vec<(NodeId, Vec<u64>)> = segment
                .outgoing_index
                .entries
                .keys()
                .filter_map(|&node_id| {
                    segment
                        .outgoing_index
                        .get_rel_ids(node_id)
                        .map(|ids| (node_id, ids.clone()))
                })
                .collect();

            (
                node_ids,
                segment.compression,
                segment.adjacency_data_range.start,
                segment.adjacency_data_range.end,
            )
        };

        let mut current_offset = initial_offset;

        for (node_id, mut rel_ids) in node_ids {
            if rel_ids.is_empty() {
                continue;
            }

            // Sort for better compression
            rel_ids.sort_unstable();

            // Convert to adjacency entries
            let adjacency_entries: Vec<AdjacencyEntry> = rel_ids
                .iter()
                .map(|&rel_id| AdjacencyEntry { rel_id })
                .collect();

            // Compress
            let compressed = self
                .compressor
                .compress_adjacency_list(&adjacency_entries, compression_type)?;

            // Check if we need to grow
            let required_size = current_offset + compressed.len() as u64;
            if required_size > adjacency_range_end {
                self.grow_adjacency_data_range(type_id, required_size)?;
                // Reset offset after growth
                current_offset = self
                    .layout
                    .relationships
                    .get(&type_id)
                    .unwrap()
                    .adjacency_data_range
                    .start;
            }

            // Store in mmap
            let start = current_offset as usize;
            let end = start + compressed.len();
            if end <= self.mmap.len() {
                self.mmap[start..end].copy_from_slice(&compressed);

                // Update index entry with offset
                if let Some(segment) = self.layout.relationships.get_mut(&type_id) {
                    if let Some(entry) = segment.outgoing_index.entries.get_mut(&node_id) {
                        entry.list_offset = current_offset;
                    }
                }
                current_offset += compressed.len() as u64;
            }
        }

        Ok(())
    }

    /// Grow the adjacency data range for a relationship type
    fn grow_adjacency_data_range(&mut self, type_id: TypeId, required_size: u64) -> Result<()> {
        let segment = self.layout.relationships.get_mut(&type_id).ok_or_else(|| {
            Error::Storage(format!(
                "Relationship segment not found for type {}",
                type_id
            ))
        })?;

        let current_size = segment.adjacency_data_range.end - segment.adjacency_data_range.start;
        let growth_size = (required_size as f64 * FILE_GROWTH_FACTOR) as u64;
        let new_end = segment.adjacency_data_range.start + growth_size.max(current_size * 2);

        segment.adjacency_data_range.end = new_end;

        // Grow file and remap if necessary
        if new_end > self.mmap.len() as u64 {
            self.grow_file_and_remap(new_end)?;
        }

        Ok(())
    }

    fn update_node_relationship_pointers(
        &mut self,
        from_node: NodeId,
        to_node: NodeId,
        rel_id: u64,
    ) -> Result<()> {
        // TODO: Implement proper relationship pointer updates
        // For now, just mark nodes as updated
        Ok(())
    }

    /// Load adjacency indices from memory-mapped storage (for existing files)
    fn load_adjacency_indices(&mut self) -> Result<()> {
        // TODO: Implement loading indices from mmap for existing files
        // For now, indices start empty for new files
        Ok(())
    }

    /// Get adjacency index statistics
    pub fn adjacency_stats(&self) -> AdjacencyStats {
        let mut total_entries = 0;
        let mut total_relationships = 0;
        let mut index_size = 0;

        for segment in self.layout.relationships.values() {
            total_entries += segment.outgoing_index.entries.len();
            total_entries += segment.incoming_index.entries.len();

            total_relationships += segment
                .outgoing_index
                .entries
                .values()
                .map(|e| e.count as u64)
                .sum::<u64>();
            total_relationships += segment
                .incoming_index
                .entries
                .values()
                .map(|e| e.count as u64)
                .sum::<u64>();

            index_size += segment.outgoing_index.calculate_size();
            index_size += segment.incoming_index.calculate_size();
        }

        AdjacencyStats {
            total_index_entries: total_entries,
            total_indexed_relationships: total_relationships,
            index_memory_usage: index_size,
            relationship_types_indexed: self.layout.relationships.len(),
        }
    }
}

/// Statistics about adjacency indices
#[derive(Clone, Debug)]
pub struct AdjacencyStats {
    pub total_index_entries: usize,
    pub total_indexed_relationships: u64,
    pub index_memory_usage: u64,
    pub relationship_types_indexed: usize,
}

impl Drop for GraphStorageEngine {
    fn drop(&mut self) {
        // Ensure all changes are flushed before dropping
        let _ = self.flush();
    }
}

/// Statistics about the storage engine
#[derive(Clone, Debug)]
pub struct StorageStats {
    pub node_count: u64,
    pub relationship_count: u64,
    pub file_size: u64,
    pub relationship_types: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_create_engine() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let engine = GraphStorageEngine::create(path).unwrap();
        let stats = engine.stats();

        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.relationship_count, 0);
        assert!(stats.file_size > 0);
    }

    #[test]
    fn test_create_node() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let mut engine = GraphStorageEngine::create(path).unwrap();
        let node_id = engine.create_node(1).unwrap();

        assert_eq!(node_id, 0);

        let node = engine.read_node(node_id).unwrap();
        assert_eq!(node.id, node_id);
        assert_eq!(node.label_id, 1);
    }

    #[test]
    fn test_create_relationship() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let mut engine = GraphStorageEngine::create(path).unwrap();

        // Create nodes first
        let node1 = engine.create_node(1).unwrap();
        let node2 = engine.create_node(2).unwrap();

        // Create relationship
        let rel_id = engine.create_relationship(node1, node2, 5).unwrap();

        assert_eq!(rel_id, 0);

        let relationship = engine.read_relationship(5, rel_id).unwrap();
        assert_eq!(relationship.id, rel_id);
        assert_eq!(relationship.from_node, node1);
        assert_eq!(relationship.to_node, node2);
        assert_eq!(relationship.type_id, 5);
    }

    #[test]
    fn test_adjacency_lists() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        let mut engine = GraphStorageEngine::create(path).unwrap();

        // Create nodes
        let node1 = engine.create_node(1).unwrap();
        let node2 = engine.create_node(2).unwrap();
        let node3 = engine.create_node(3).unwrap();

        // Create relationships
        engine.create_relationship(node1, node2, 10).unwrap(); // node1 -> node2
        engine.create_relationship(node1, node3, 10).unwrap(); // node1 -> node3
        engine.create_relationship(node2, node3, 10).unwrap(); // node2 -> node3
        engine.create_relationship(node3, node1, 20).unwrap(); // node3 -> node1 (different type)

        // Test outgoing relationships from node1 (should have 2 relationships of type 10)
        let outgoing = engine.get_outgoing_relationships(node1, 10).unwrap();
        assert_eq!(outgoing.len(), 2);
        assert!(
            outgoing
                .iter()
                .all(|r| r.from_node == node1 && r.type_id == 10)
        );

        // Test incoming relationships to node3 (should have 2 relationships of type 10)
        let incoming = engine.get_incoming_relationships(node3, 10).unwrap();
        assert_eq!(incoming.len(), 2);
        assert!(
            incoming
                .iter()
                .all(|r| r.to_node == node3 && r.type_id == 10)
        );

        // Test different relationship types (node3 -> node1 of type 20)
        let outgoing_type20 = engine.get_outgoing_relationships(node3, 20).unwrap();
        assert_eq!(outgoing_type20.len(), 1);
        assert_eq!(outgoing_type20[0].to_node, node1);
        assert_eq!(outgoing_type20[0].type_id, 20);

        // Test adjacency statistics
        let stats = engine.adjacency_stats();
        assert!(stats.total_index_entries > 0);
        assert!(stats.total_indexed_relationships >= 4); // At least the 4 relationships we created
        assert!(stats.index_memory_usage > 0);
        assert!(stats.relationship_types_indexed >= 2); // Types 10 and 20
    }
}
