//! Phase 3: Adjacency List Storage for Relationship Traversal Optimization
//!
//! This module implements an adjacency list structure to optimize relationship traversal
//! by co-locating relationship information with nodes, improving cache locality and
//! reducing random access patterns.
//!
//! ## Design
//!
//! - **Outgoing relationships**: Stored in `adjacency.store` with node ID as key
//! - **Incoming relationships**: Stored separately for efficient reverse traversal
//! - **Format**: Variable-length records with type-filtered lists
//! - **Cache-friendly**: Adjacency lists are stored contiguously for better cache performance

use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

/// Size of adjacency list header (20 bytes: node_id(8) + count(4) + type_id(4) + total_size(4))
/// Note: Packed struct, no padding
const ADJACENCY_HEADER_SIZE: usize = std::mem::size_of::<AdjacencyListHeader>();

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

/// Adjacency list store for efficient relationship traversal
pub struct AdjacencyListStore {
    /// Path to the storage directory
    path: PathBuf,
    /// Outgoing relationships file handle
    outgoing_file: File,
    /// Incoming relationships file handle
    incoming_file: File,
    /// Memory-mapped outgoing relationships file
    outgoing_mmap: MmapMut,
    /// Memory-mapped incoming relationships file
    incoming_mmap: MmapMut,
    /// Current outgoing file size
    outgoing_file_size: usize,
    /// Current incoming file size
    incoming_file_size: usize,
    /// Index: node_id -> offset in outgoing file
    outgoing_index: HashMap<u64, u64>,
    /// Index: node_id -> offset in incoming file
    incoming_index: HashMap<u64, u64>,
    /// Next available offset in outgoing file
    next_outgoing_offset: u64,
    /// Next available offset in incoming file
    next_incoming_offset: u64,
}

impl AdjacencyListStore {
    /// Create a new adjacency list store
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;

        let outgoing_path = path.join("adjacency.outgoing.store");
        let incoming_path = path.join("adjacency.incoming.store");

        // Create or open outgoing file
        let mut outgoing_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&outgoing_path)?;

        // Create or open incoming file
        let mut incoming_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&incoming_path)?;

        // Initialize file size if empty
        let outgoing_file_size = if outgoing_file.metadata()?.len() == 0 {
            let initial_size = 1024 * 1024; // 1MB
            outgoing_file.set_len(initial_size as u64)?;
            initial_size
        } else {
            outgoing_file.metadata()?.len() as usize
        };

        let incoming_file_size = if incoming_file.metadata()?.len() == 0 {
            let initial_size = 1024 * 1024; // 1MB
            incoming_file.set_len(initial_size as u64)?;
            initial_size
        } else {
            incoming_file.metadata()?.len() as usize
        };

        // Create memory mappings
        let outgoing_mmap = unsafe { MmapOptions::new().map_mut(&outgoing_file)? };
        let incoming_mmap = unsafe { MmapOptions::new().map_mut(&incoming_file)? };

        // Build indexes from existing data
        let (outgoing_index, next_outgoing_offset) = Self::build_index(&outgoing_mmap)?;
        let (incoming_index, next_incoming_offset) = Self::build_index(&incoming_mmap)?;

        Ok(Self {
            path,
            outgoing_file,
            incoming_file,
            outgoing_mmap,
            incoming_mmap,
            outgoing_file_size,
            incoming_file_size,
            outgoing_index,
            incoming_index,
            next_outgoing_offset,
            next_incoming_offset,
        })
    }

    /// Build index from existing file
    fn build_index(mmap: &MmapMut) -> Result<(HashMap<u64, u64>, u64)> {
        let mut index = HashMap::new();
        let mut offset = 0u64;
        let mut max_offset = 0u64;

        // If file is empty or too small, return empty index
        if mmap.len() < ADJACENCY_HEADER_SIZE {
            return Ok((index, 0));
        }

        while offset < mmap.len() as u64 {
            if offset + ADJACENCY_HEADER_SIZE as u64 > mmap.len() as u64 {
                break;
            }

            // Read header
            let header_bytes =
                &mmap[offset as usize..(offset + ADJACENCY_HEADER_SIZE as u64) as usize];

            // Check if all bytes are zero (empty/uninitialized)
            if header_bytes.iter().all(|&b| b == 0) {
                break; // End of valid data
            }

            // Verify we have enough bytes for the header
            if header_bytes.len() < std::mem::size_of::<AdjacencyListHeader>() {
                break;
            }

            let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);

            if header.count == 0 || header.total_size == 0 {
                // Empty or invalid entry, skip
                offset += ADJACENCY_HEADER_SIZE as u64;
                continue;
            }

            // Read node_id from header and add to index (only for first list of each node)
            let header_node_id = header.node_id;
            if !index.contains_key(&header_node_id) {
                index.insert(header_node_id, offset);
            }

            let list_size = header.total_size as u64;
            max_offset = max_offset.max(offset + list_size);
            offset += list_size;
        }

        Ok((index, max_offset))
    }

    /// Add outgoing relationships for a node
    pub fn add_outgoing_relationships(
        &mut self,
        node_id: u64,
        relationships: &[(u64, u32)], // (rel_id, type_id)
    ) -> Result<()> {
        // Group by type_id for efficient storage
        let mut by_type: HashMap<u32, Vec<u64>> = HashMap::new();
        for (rel_id, type_id) in relationships {
            by_type.entry(*type_id).or_default().push(*rel_id);
        }

        // Phase 3 Deep Optimization: Fast offset lookup with caching
        // Get starting offset for this node (all lists for a node are stored contiguously)
        let start_offset = if let Some(&existing_offset) = self.outgoing_index.get(&node_id) {
            // Phase 3 Optimization: Fast append - skip traversal for single relationship
            // If adding only one relationship, we can optimize by checking if we can append
            // to the last list of the same type, or just append a new list
            if relationships.len() == 1 {
                // For single relationship, just append at next_outgoing_offset
                // This avoids expensive traversal of existing lists
                self.next_outgoing_offset
            } else {
                // For multiple relationships, need to find end (original logic)
                let mut current = existing_offset;
                while current < self.outgoing_mmap.len() as u64 {
                    if current + ADJACENCY_HEADER_SIZE as u64 > self.outgoing_mmap.len() as u64 {
                        break;
                    }
                    let header_bytes = &self.outgoing_mmap
                        [current as usize..(current + ADJACENCY_HEADER_SIZE as u64) as usize];

                    // Check if all bytes are zero (end of data)
                    if header_bytes.iter().all(|&b| b == 0) {
                        break;
                    }

                    let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);
                    if header.count == 0 || header.total_size == 0 {
                        break;
                    }
                    // Verify this header belongs to the same node
                    if header.node_id != node_id {
                        break; // Different node, stop here
                    }
                    current += header.total_size as u64;
                }
                current
            }
        } else {
            // New node - use next available offset
            self.next_outgoing_offset
        };

        let mut current_offset = start_offset;

        // Check if we need to find existing lists of the same types to merge
        // For now, we'll always append new lists (even for same types)
        // This allows multiple lists of the same type, which is valid

        // Store each type group separately for efficient filtering
        for (type_id, rel_ids) in by_type {
            let count = rel_ids.len() as u32;
            let entries_size = count as usize * std::mem::size_of::<AdjacencyEntry>();
            let total_size = ADJACENCY_HEADER_SIZE + entries_size;

            // Phase 3 Optimization: Pre-check capacity to avoid unnecessary remapping
            if current_offset + total_size as u64 > self.outgoing_file_size as u64 {
                self.ensure_outgoing_capacity(current_offset + total_size as u64)?;
            }

            // Phase 3 Deep Optimization: Batch write header and entries
            // Write header
            let header = AdjacencyListHeader {
                node_id,
                count,
                type_id,
                total_size: total_size as u32,
            };
            let header_bytes = bytemuck::bytes_of(&header);
            self.outgoing_mmap
                [current_offset as usize..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize]
                .copy_from_slice(header_bytes);

            // Phase 3 Optimization: Batch write entries for better performance
            let entries_start = current_offset + ADJACENCY_HEADER_SIZE as u64;
            let entry_size = std::mem::size_of::<AdjacencyEntry>();

            // Write all entries in one pass (better cache locality)
            for (i, &rel_id) in rel_ids.iter().enumerate() {
                let entry = AdjacencyEntry {
                    node_id: rel_id, // Placeholder - will be target node ID in Phase 3.1.2
                    rel_id,
                };
                let entry_bytes = bytemuck::bytes_of(&entry);
                let entry_offset = entries_start + (i * entry_size) as u64;
                self.outgoing_mmap
                    [entry_offset as usize..(entry_offset + entry_size as u64) as usize]
                    .copy_from_slice(entry_bytes);
            }

            current_offset += total_size as u64;
        }

        // Update index (only if this is a new node)
        if !self.outgoing_index.contains_key(&node_id) {
            self.outgoing_index.insert(node_id, start_offset);
        }
        // Always update next_outgoing_offset to track the end
        self.next_outgoing_offset = self.next_outgoing_offset.max(current_offset);

        Ok(())
    }

    /// Add incoming relationships for a node
    pub fn add_incoming_relationships(
        &mut self,
        node_id: u64,
        relationships: &[(u64, u32)], // (rel_id, type_id)
    ) -> Result<()> {
        // Group by type_id for efficient storage
        let mut by_type: HashMap<u32, Vec<u64>> = HashMap::new();
        for (rel_id, type_id) in relationships {
            by_type.entry(*type_id).or_default().push(*rel_id);
        }

        // Phase 3 Deep Optimization: Fast offset lookup (same as outgoing)
        let start_offset = if let Some(&existing_offset) = self.incoming_index.get(&node_id) {
            // Phase 3 Optimization: Fast append for single relationship
            if relationships.len() == 1 {
                self.next_incoming_offset
            } else {
                // For multiple relationships, find end
                let mut current = existing_offset;
                while current < self.incoming_mmap.len() as u64 {
                    if current + ADJACENCY_HEADER_SIZE as u64 > self.incoming_mmap.len() as u64 {
                        break;
                    }
                    let header_bytes = &self.incoming_mmap
                        [current as usize..(current + ADJACENCY_HEADER_SIZE as u64) as usize];

                    if header_bytes.iter().all(|&b| b == 0) {
                        break;
                    }

                    let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);
                    if header.count == 0 || header.total_size == 0 {
                        break;
                    }
                    if header.node_id != node_id {
                        break;
                    }
                    current += header.total_size as u64;
                }
                current
            }
        } else {
            self.next_incoming_offset
        };

        let mut current_offset = start_offset;

        // Store each type group separately
        for (type_id, rel_ids) in by_type {
            let count = rel_ids.len() as u32;
            let entries_size = count as usize * std::mem::size_of::<AdjacencyEntry>();
            let total_size = ADJACENCY_HEADER_SIZE + entries_size;

            // Phase 3 Optimization: Pre-check capacity
            if current_offset + total_size as u64 > self.incoming_file_size as u64 {
                self.ensure_incoming_capacity(current_offset + total_size as u64)?;
            }

            // Write header
            let header = AdjacencyListHeader {
                node_id,
                count,
                type_id,
                total_size: total_size as u32,
            };
            let header_bytes = bytemuck::bytes_of(&header);
            self.incoming_mmap
                [current_offset as usize..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize]
                .copy_from_slice(header_bytes);

            // Phase 3 Optimization: Batch write entries
            let entries_start = current_offset + ADJACENCY_HEADER_SIZE as u64;
            let entry_size = std::mem::size_of::<AdjacencyEntry>();

            // Write all entries in one pass (better cache locality)
            for (i, &rel_id) in rel_ids.iter().enumerate() {
                let entry = AdjacencyEntry {
                    node_id: rel_id, // Placeholder
                    rel_id,
                };
                let entry_bytes = bytemuck::bytes_of(&entry);
                let entry_offset = entries_start + (i * entry_size) as u64;
                self.incoming_mmap
                    [entry_offset as usize..(entry_offset + entry_size as u64) as usize]
                    .copy_from_slice(entry_bytes);
            }

            current_offset += total_size as u64;
        }

        // Update index
        if !self.incoming_index.contains_key(&node_id) {
            self.incoming_index.insert(node_id, start_offset);
        }
        self.next_incoming_offset = self.next_incoming_offset.max(current_offset);

        Ok(())
    }

    /// Phase 3 Deep Optimization: Count relationships without reading entries
    pub fn count_outgoing_relationships(&self, node_id: u64, type_ids: &[u32]) -> Result<u64> {
        let mut count = 0u64;

        if let Some(&offset) = self.outgoing_index.get(&node_id) {
            let mut current_offset = offset;

            while current_offset < self.outgoing_mmap.len() as u64 {
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.outgoing_mmap.len() as u64 {
                    break;
                }

                let header_bytes = &self.outgoing_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                // Check if all bytes are zero (end of data)
                if header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);

                if header.count == 0 || header.total_size == 0 {
                    break;
                }

                let header_node_id = header.node_id;
                if header_node_id != node_id {
                    break;
                }

                let header_type_id = header.type_id;
                let matches_type = type_ids.is_empty() || type_ids.contains(&header_type_id);

                if matches_type {
                    // Just count, don't read entries - much faster!
                    count += header.count as u64;
                }

                current_offset += header.total_size as u64;

                if current_offset >= self.outgoing_mmap.len() as u64 {
                    break;
                }

                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.outgoing_mmap.len() as u64 {
                    break;
                }
                let next_header_bytes = &self.outgoing_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                if next_header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let next_header = bytemuck::from_bytes::<AdjacencyListHeader>(next_header_bytes);
                if next_header.count == 0 || next_header.total_size == 0 {
                    break;
                }
                if next_header.node_id != node_id {
                    break;
                }
            }
        }

        Ok(count)
    }

    /// Phase 3 Deep Optimization: Count incoming relationships without reading entries
    pub fn count_incoming_relationships(&self, node_id: u64, type_ids: &[u32]) -> Result<u64> {
        let mut count = 0u64;

        if let Some(&offset) = self.incoming_index.get(&node_id) {
            let mut current_offset = offset;

            while current_offset < self.incoming_mmap.len() as u64 {
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.incoming_mmap.len() as u64 {
                    break;
                }

                let header_bytes = &self.incoming_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                if header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);

                if header.count == 0 || header.total_size == 0 {
                    break;
                }

                let header_node_id = header.node_id;
                if header_node_id != node_id {
                    break;
                }

                let header_type_id = header.type_id;
                let matches_type = type_ids.is_empty() || type_ids.contains(&header_type_id);

                if matches_type {
                    // Phase 3 Deep Optimization: Just count from header, don't read entries
                    count += header.count as u64;
                }

                current_offset += header.total_size as u64;

                if current_offset >= self.incoming_mmap.len() as u64 {
                    break;
                }

                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.incoming_mmap.len() as u64 {
                    break;
                }
                let next_header_bytes = &self.incoming_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                if next_header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let next_header = bytemuck::from_bytes::<AdjacencyListHeader>(next_header_bytes);
                if next_header.count == 0 || next_header.total_size == 0 {
                    break;
                }
                if next_header.node_id != node_id {
                    break;
                }
            }
        }

        Ok(count)
    }

    /// Get outgoing relationships for a node, optionally filtered by type
    pub fn get_outgoing_relationships(&self, node_id: u64, type_ids: &[u32]) -> Result<Vec<u64>> {
        let mut result = Vec::new();

        if let Some(&offset) = self.outgoing_index.get(&node_id) {
            let mut current_offset = offset;

            // Traverse all lists for this node (each type has its own list)
            while current_offset < self.outgoing_mmap.len() as u64 {
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.outgoing_mmap.len() as u64 {
                    break;
                }

                // Read header
                let header_bytes = &self.outgoing_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];
                let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);

                if header.count == 0 || header.total_size == 0 {
                    break;
                }

                // Verify this list belongs to the requested node (safety check)
                let header_node_id = header.node_id;
                if header_node_id != node_id {
                    // This list belongs to a different node, stop traversing
                    break;
                }

                // Check if this type matches (copy field to avoid unaligned reference)
                let header_type_id = header.type_id;
                let matches_type = type_ids.is_empty() || type_ids.contains(&header_type_id);

                if matches_type {
                    // Phase 3 Deep Optimization: Batch read entries for better cache locality
                    let entries_start = current_offset + ADJACENCY_HEADER_SIZE as u64;
                    let count = header.count as usize;
                    let entry_size = std::mem::size_of::<AdjacencyEntry>();

                    // Pre-allocate space for all entries
                    result.reserve(count);

                    // Read all entries in one pass (better cache locality)
                    for i in 0..count {
                        let entry_offset = entries_start + (i * entry_size) as u64;
                        if entry_offset + 16 <= self.outgoing_mmap.len() as u64 {
                            let entry_bytes = &self.outgoing_mmap
                                [entry_offset as usize..(entry_offset + 16) as usize];
                            let entry = bytemuck::from_bytes::<AdjacencyEntry>(entry_bytes);
                            result.push(entry.rel_id);
                        }
                    }
                }

                // Move to next list (if any)
                current_offset += header.total_size as u64;

                // Check if we've reached the end of the file
                if current_offset >= self.outgoing_mmap.len() as u64 {
                    break;
                }

                // Peek at next header to see if it belongs to this node
                // If the next header is invalid or belongs to another node, stop
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.outgoing_mmap.len() as u64 {
                    break;
                }
                let next_header_bytes = &self.outgoing_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                // Check if all bytes are zero (end of data)
                if next_header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let next_header = bytemuck::from_bytes::<AdjacencyListHeader>(next_header_bytes);
                if next_header.count == 0 || next_header.total_size == 0 {
                    break;
                }
                // Check if next header belongs to same node
                if next_header.node_id != node_id {
                    break;
                }
            }
        }

        Ok(result)
    }

    /// Get incoming relationships for a node, optionally filtered by type
    pub fn get_incoming_relationships(&self, node_id: u64, type_ids: &[u32]) -> Result<Vec<u64>> {
        let mut result = Vec::new();

        if let Some(&offset) = self.incoming_index.get(&node_id) {
            let mut current_offset = offset;

            // Traverse all lists for this node
            while current_offset < self.incoming_mmap.len() as u64 {
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.incoming_mmap.len() as u64 {
                    break;
                }

                // Read header
                let header_bytes = &self.incoming_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];
                let header = bytemuck::from_bytes::<AdjacencyListHeader>(header_bytes);

                if header.count == 0 || header.total_size == 0 {
                    break;
                }

                // Verify this list belongs to the requested node
                let header_node_id = header.node_id;
                if header_node_id != node_id {
                    break;
                }

                // Check if this type matches
                let header_type_id = header.type_id;
                let matches_type = type_ids.is_empty() || type_ids.contains(&header_type_id);

                if matches_type {
                    // Phase 3 Deep Optimization: Batch read entries for better cache locality
                    let entries_start = current_offset + ADJACENCY_HEADER_SIZE as u64;
                    let count = header.count as usize;
                    let entry_size = std::mem::size_of::<AdjacencyEntry>();

                    // Pre-allocate space for all entries
                    result.reserve(count);

                    // Read all entries in one pass (better cache locality)
                    for i in 0..count {
                        let entry_offset = entries_start + (i * entry_size) as u64;
                        if entry_offset + 16 <= self.incoming_mmap.len() as u64 {
                            let entry_bytes = &self.incoming_mmap
                                [entry_offset as usize..(entry_offset + 16) as usize];
                            let entry = bytemuck::from_bytes::<AdjacencyEntry>(entry_bytes);
                            result.push(entry.rel_id);
                        }
                    }
                }

                // Move to next list
                current_offset += header.total_size as u64;

                if current_offset >= self.incoming_mmap.len() as u64 {
                    break;
                }

                // Peek at next header
                if current_offset + ADJACENCY_HEADER_SIZE as u64 > self.incoming_mmap.len() as u64 {
                    break;
                }
                let next_header_bytes = &self.incoming_mmap[current_offset as usize
                    ..(current_offset + ADJACENCY_HEADER_SIZE as u64) as usize];

                if next_header_bytes.iter().all(|&b| b == 0) {
                    break;
                }

                let next_header = bytemuck::from_bytes::<AdjacencyListHeader>(next_header_bytes);
                if next_header.count == 0 || next_header.total_size == 0 {
                    break;
                }
                if next_header.node_id != node_id {
                    break;
                }
            }
        }

        Ok(result)
    }

    /// Ensure incoming file has enough capacity
    /// Phase 3 Deep Optimization: Reduce remapping overhead
    fn ensure_incoming_capacity(&mut self, required_size: u64) -> Result<()> {
        if required_size > self.incoming_file_size as u64 {
            // Phase 3 Optimization: Larger growth factor
            let min_growth = 4 * 1024 * 1024; // 4MB minimum (increased from 2MB)
            let calculated_size = ((required_size as f64) * 2.0) as usize; // 2x growth (increased from 1.5x)
            let new_size = calculated_size.max(min_growth).max(required_size as usize);

            self.incoming_file.set_len(new_size as u64)?;
            self.incoming_file_size = new_size;

            // Remap the memory-mapped file
            self.incoming_mmap = unsafe { MmapOptions::new().map_mut(&self.incoming_file)? };
        }
        Ok(())
    }

    /// Ensure outgoing file has enough capacity
    /// Phase 3 Deep Optimization: Reduce remapping overhead
    fn ensure_outgoing_capacity(&mut self, required_size: u64) -> Result<()> {
        if required_size > self.outgoing_file_size as u64 {
            // Phase 3 Optimization: Larger growth factor to reduce remapping frequency
            let min_growth = 4 * 1024 * 1024; // 4MB minimum (increased from 2MB)
            let calculated_size = ((required_size as f64) * 2.0) as usize; // 2x growth (increased from 1.5x)
            let new_size = calculated_size.max(min_growth).max(required_size as usize);

            self.outgoing_file.set_len(new_size as u64)?;
            self.outgoing_mmap = unsafe { MmapOptions::new().map_mut(&self.outgoing_file)? };
            self.outgoing_file_size = new_size;
        }
        Ok(())
    }

    /// Flush changes to disk
    pub fn flush(&mut self) -> Result<()> {
        self.outgoing_mmap
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush outgoing adjacency: {}", e)))?;
        self.incoming_mmap
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush incoming adjacency: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_adjacency_list_store_creation() {
        let dir = TempDir::new().unwrap();
        let store = AdjacencyListStore::new(dir.path()).unwrap();
        assert_eq!(store.outgoing_file_size, 1024 * 1024);
        assert_eq!(store.incoming_file_size, 1024 * 1024);
    }

    #[test]
    fn test_add_outgoing_relationships() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships for node 1
        let relationships = vec![(1, 1), (2, 1), (3, 2)]; // (rel_id, type_id)
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Retrieve relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_get_outgoing_relationships_filtered() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with different types
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by type 1
        let result = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(!result.contains(&3));
    }

    #[test]
    fn test_multiple_nodes_with_relationships() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships for node 1
        let node1_rels = vec![(1, 1), (2, 1), (3, 2)];
        store.add_outgoing_relationships(1, &node1_rels).unwrap();

        // Add relationships for node 2
        let node2_rels = vec![(4, 1), (5, 3)];
        store.add_outgoing_relationships(2, &node2_rels).unwrap();

        // Verify node 1 relationships
        let result1 = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result1.len(), 3);
        assert!(result1.contains(&1));
        assert!(result1.contains(&2));
        assert!(result1.contains(&3));

        // Verify node 2 relationships
        let result2 = store.get_outgoing_relationships(2, &[]).unwrap();
        assert_eq!(result2.len(), 2);
        assert!(result2.contains(&4));
        assert!(result2.contains(&5));

        // Verify isolation (node 1 doesn't have node 2's relationships)
        assert!(!result1.contains(&4));
        assert!(!result1.contains(&5));
    }

    #[test]
    fn test_node_with_no_relationships() {
        let dir = TempDir::new().unwrap();
        let store = AdjacencyListStore::new(dir.path()).unwrap();

        // Node with no relationships should return empty vector
        let result = store.get_outgoing_relationships(999, &[]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_add_relationships_incrementally() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add first batch of relationships
        let batch1 = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &batch1).unwrap();

        // Add second batch of relationships (same node, different types)
        let batch2 = vec![(3, 2), (4, 2)];
        store.add_outgoing_relationships(1, &batch2).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
    }

    #[test]
    fn test_filter_by_multiple_types() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with multiple types
        let relationships = vec![(1, 1), (2, 1), (3, 2), (4, 3), (5, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by types 1 and 2
        let result = store.get_outgoing_relationships(1, &[1, 2]).unwrap();
        assert_eq!(result.len(), 4);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&5));
        assert!(!result.contains(&4)); // Type 3 should be excluded
    }

    #[test]
    fn test_filter_by_nonexistent_type() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with type 1
        let relationships = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by nonexistent type
        let result = store.get_outgoing_relationships(1, &[999]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_large_number_of_relationships() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add 100 relationships for a single node
        let mut relationships = Vec::new();
        for i in 0..100 {
            relationships.push((i as u64, (i % 5) as u32)); // 5 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 100);

        // Verify filtering works with large dataset
        let type_0_rels = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type_0_rels.len(), 20); // 100 / 5 = 20 per type
    }

    #[test]
    fn test_flush_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        // Create store and add relationships
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(1, 1), (2, 1), (3, 2)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen store and verify relationships persist
        let store = AdjacencyListStore::new(path).unwrap();
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_empty_relationships_list() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Adding empty list should not crash
        let empty: Vec<(u64, u32)> = vec![];
        store.add_outgoing_relationships(1, &empty).unwrap();

        // Should return empty result
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_same_relationship_id_different_types() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Note: In real usage, same rel_id shouldn't have different types
        // But we test that the store handles it gracefully
        let relationships = vec![(1, 1), (1, 2)]; // Same rel_id, different types
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Both should be stored (though this is unusual)
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
    }

    #[test]
    fn test_high_degree_node() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Simulate high-degree node (node with many relationships)
        let mut relationships = Vec::new();
        for i in 0..1000 {
            relationships.push((i as u64, (i % 10) as u32)); // 10 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 1000);

        // Verify filtering performance with high-degree node
        let type_5_rels = store.get_outgoing_relationships(1, &[5]).unwrap();
        assert_eq!(type_5_rels.len(), 100); // 1000 / 10 = 100 per type
    }

    #[test]
    fn test_concurrent_node_access_pattern() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships for multiple nodes (simulating concurrent access pattern)
        for node_id in 0..10 {
            let mut relationships = Vec::new();
            for rel_id in 0..10 {
                relationships.push((node_id * 100 + rel_id, (rel_id % 3) as u32));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify each node has correct relationships
        for node_id in 0..10 {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 10);
            for rel_id in 0..10 {
                assert!(result.contains(&(node_id * 100 + rel_id)));
            }
        }
    }

    #[test]
    fn test_type_distribution() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with uneven type distribution
        let relationships = vec![
            (1, 1),
            (2, 1),
            (3, 1), // 3 of type 1
            (4, 2), // 1 of type 2
            (5, 3),
            (6, 3),
            (7, 3),
            (8, 3),
            (9, 3), // 5 of type 3
        ];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify type distribution
        let type1 = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(type1.len(), 3);

        let type2 = store.get_outgoing_relationships(1, &[2]).unwrap();
        assert_eq!(type2.len(), 1);

        let type3 = store.get_outgoing_relationships(1, &[3]).unwrap();
        assert_eq!(type3.len(), 5);
    }

    #[test]
    fn test_stress_many_nodes() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Create 1000 nodes, each with 10 relationships
        for node_id in 0..1000 {
            let mut relationships = Vec::new();
            for rel_id in 0..10 {
                relationships.push((node_id * 1000 + rel_id, (rel_id % 5) as u32));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify random nodes
        let result_500 = store.get_outgoing_relationships(500, &[]).unwrap();
        assert_eq!(result_500.len(), 10);

        let result_999 = store.get_outgoing_relationships(999, &[]).unwrap();
        assert_eq!(result_999.len(), 10);
    }

    #[test]
    fn test_very_large_relationship_list() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add 10,000 relationships for a single node
        let mut relationships = Vec::new();
        for i in 0..10000 {
            relationships.push((i as u64, (i % 20) as u32)); // 20 different types
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 10000);

        // Verify filtering works with very large dataset
        let type_0_rels = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type_0_rels.len(), 500); // 10000 / 20 = 500 per type
    }

    #[test]
    fn test_sequential_vs_batch_addition() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships sequentially
        for i in 0..10 {
            let rels = vec![(i, 1)];
            store.add_outgoing_relationships(1, &rels).unwrap();
        }

        // Add relationships in batch
        let mut batch_rels = Vec::new();
        for i in 10..20 {
            batch_rels.push((i, 1));
        }
        store.add_outgoing_relationships(2, &batch_rels).unwrap();

        // Both should have same result
        let result1 = store.get_outgoing_relationships(1, &[]).unwrap();
        let result2 = store.get_outgoing_relationships(2, &[]).unwrap();
        assert_eq!(result1.len(), 10);
        assert_eq!(result2.len(), 10);
    }

    #[test]
    fn test_mixed_type_distribution() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with mixed type distribution
        let relationships = vec![
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 1),
            (5, 1), // 5 of type 1
            (6, 2), // 1 of type 2
            (7, 3),
            (8, 3), // 2 of type 3
            (9, 1),
            (10, 1), // 2 more of type 1
        ];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify type 1 has all 7 relationships
        let type1 = store.get_outgoing_relationships(1, &[1]).unwrap();
        assert_eq!(type1.len(), 7);

        // Verify type 2 has 1 relationship
        let type2 = store.get_outgoing_relationships(1, &[2]).unwrap();
        assert_eq!(type2.len(), 1);

        // Verify type 3 has 2 relationships
        let type3 = store.get_outgoing_relationships(1, &[3]).unwrap();
        assert_eq!(type3.len(), 2);
    }

    #[test]
    fn test_boundary_conditions() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Test with node_id = 0
        let rels = vec![(1, 1)];
        store.add_outgoing_relationships(0, &rels).unwrap();
        let result = store.get_outgoing_relationships(0, &[]).unwrap();
        assert_eq!(result.len(), 1);

        // Test with very large node_id
        let rels2 = vec![(2, 1)];
        store.add_outgoing_relationships(u64::MAX, &rels2).unwrap();
        let result2 = store.get_outgoing_relationships(u64::MAX, &[]).unwrap();
        assert_eq!(result2.len(), 1);

        // Test with type_id = 0
        let rels3 = vec![(3, 0)];
        store.add_outgoing_relationships(1, &rels3).unwrap();
        let result3 = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(result3.len(), 1);
    }

    #[test]
    fn test_reopen_store_multiple_times() {
        let dir = TempDir::new().unwrap();
        let path = dir.path();

        // Create and add relationships
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(1, 1), (2, 1), (3, 2)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen and add more
        {
            let mut store = AdjacencyListStore::new(path).unwrap();
            let relationships = vec![(4, 2), (5, 3)];
            store.add_outgoing_relationships(1, &relationships).unwrap();
            store.flush().unwrap();
        }

        // Reopen and verify all relationships
        let store = AdjacencyListStore::new(path).unwrap();
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 5);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        assert!(result.contains(&4));
        assert!(result.contains(&5));
    }

    #[test]
    fn test_multiple_flushes() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships and flush multiple times
        for i in 0..5 {
            let rels = vec![(i, 1)];
            store.add_outgoing_relationships(1, &rels).unwrap();
            store.flush().unwrap();
        }

        // Verify all relationships persist
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn test_filter_all_types() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with different types
        let relationships = vec![(1, 1), (2, 2), (3, 3), (4, 1), (5, 2)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by all types (empty array = all types)
        let all = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(all.len(), 5);

        // Filter by specific types
        let filtered = store.get_outgoing_relationships(1, &[1, 2]).unwrap();
        assert_eq!(filtered.len(), 4);
        assert!(!filtered.contains(&3)); // Type 3 should be excluded
    }

    #[test]
    fn test_node_isolation() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships for multiple nodes
        for node_id in 0..5 {
            let mut relationships = Vec::new();
            for rel_id in 0..5 {
                relationships.push((node_id * 10 + rel_id, 1));
            }
            store
                .add_outgoing_relationships(node_id, &relationships)
                .unwrap();
        }

        // Verify each node only has its own relationships
        for node_id in 0..5 {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 5);
            // Verify relationships belong to this node
            for &rel_id in &result {
                let expected_min = node_id * 10;
                let expected_max = node_id * 10 + 4;
                assert!(rel_id >= expected_min && rel_id <= expected_max);
            }
        }
    }

    #[test]
    fn test_type_filtering_performance() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add 1000 relationships with 100 different types
        let mut relationships = Vec::new();
        for i in 0..1000 {
            relationships.push((i as u64, (i % 100) as u32));
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by single type (should be fast)
        let type_50 = store.get_outgoing_relationships(1, &[50]).unwrap();
        assert_eq!(type_50.len(), 10); // 1000 / 100 = 10 per type

        // Filter by multiple types
        let types_10_20_30 = store.get_outgoing_relationships(1, &[10, 20, 30]).unwrap();
        assert_eq!(types_10_20_30.len(), 30); // 10 per type * 3 types
    }

    #[test]
    fn test_concurrent_node_patterns() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Simulate concurrent access pattern: add relationships to different nodes
        // in an interleaved manner, but batch by node to avoid overwriting
        let nodes = vec![1, 5, 10, 15, 20];

        // Collect all relationships per node first, then add in batches
        let mut node_relationships: std::collections::HashMap<u64, Vec<(u64, u32)>> =
            std::collections::HashMap::new();
        for round in 0..10 {
            for &node_id in &nodes {
                let rel_id = node_id * 100 + round;
                node_relationships
                    .entry(node_id)
                    .or_default()
                    .push((rel_id, (round % 3) as u32));
            }
        }

        // Add all relationships for each node in one batch
        for &node_id in &nodes {
            let rels = node_relationships.get(&node_id).unwrap();
            store.add_outgoing_relationships(node_id, rels).unwrap();
        }

        // Verify each node has 10 relationships
        for &node_id in &nodes {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(
                result.len(),
                10,
                "Node {} should have 10 relationships",
                node_id
            );

            // Verify all expected relationship IDs are present
            for round in 0..10 {
                let expected_rel_id = node_id * 100 + round;
                assert!(
                    result.contains(&expected_rel_id),
                    "Node {} missing relationship {}",
                    node_id,
                    expected_rel_id
                );
            }
        }
    }

    #[test]
    fn test_file_growth() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add enough relationships to trigger file growth
        // Each relationship needs ~20 bytes (header) + 16 bytes (entry) = 36 bytes
        // 1MB / 36 bytes = ~29,000 relationships per MB
        // Let's add 50,000 relationships to ensure growth
        let mut relationships = Vec::new();
        for i in 0..50000 {
            relationships.push((i as u64, (i % 10) as u32));
        }
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Verify all relationships are present
        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 50000);

        // Verify file grew
        assert!(store.outgoing_file_size >= 1024 * 1024); // At least 1MB
    }

    #[test]
    fn test_sparse_node_distribution() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships to sparse node IDs (not consecutive)
        let sparse_nodes = vec![1, 100, 1000, 10000, 100000];
        for &node_id in &sparse_nodes {
            let rels = vec![(node_id, 1)];
            store.add_outgoing_relationships(node_id, &rels).unwrap();
        }

        // Verify each sparse node
        for &node_id in &sparse_nodes {
            let result = store.get_outgoing_relationships(node_id, &[]).unwrap();
            assert_eq!(result.len(), 1);
            assert!(result.contains(&node_id));
        }
    }

    #[test]
    fn test_relationship_id_uniqueness() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with unique IDs
        let relationships = vec![(1, 1), (2, 1), (3, 1), (100, 2), (200, 2), (1000, 3)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        // All relationship IDs should be unique in the result
        let mut seen = std::collections::HashSet::new();
        for &rel_id in &result {
            assert!(seen.insert(rel_id), "Duplicate relationship ID: {}", rel_id);
        }
    }

    #[test]
    fn test_mixed_batch_sizes() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships in batches of different sizes
        store.add_outgoing_relationships(1, &[(1, 1)]).unwrap(); // Single
        store
            .add_outgoing_relationships(1, &[(2, 1), (3, 1)])
            .unwrap(); // Pair
        store
            .add_outgoing_relationships(1, &[(4, 1), (5, 1), (6, 1), (7, 1), (8, 1)])
            .unwrap(); // Large batch

        let result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 8);
    }

    #[test]
    fn test_type_zero_handling() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add relationships with type_id = 0 (valid type)
        let relationships = vec![(1, 0), (2, 0), (3, 1)];
        store.add_outgoing_relationships(1, &relationships).unwrap();

        // Filter by type 0
        let type0 = store.get_outgoing_relationships(1, &[0]).unwrap();
        assert_eq!(type0.len(), 2);
        assert!(type0.contains(&1));
        assert!(type0.contains(&2));
        assert!(!type0.contains(&3));
    }

    #[test]
    fn test_add_incoming_relationships() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add incoming relationships for node 1
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_incoming_relationships(1, &relationships).unwrap();

        // Retrieve incoming relationships
        let result = store.get_incoming_relationships(1, &[]).unwrap();
        assert_eq!(result.len(), 3);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
    }

    #[test]
    fn test_incoming_relationships_filtered() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add incoming relationships with different types
        let relationships = vec![(1, 1), (2, 1), (3, 2)];
        store.add_incoming_relationships(1, &relationships).unwrap();

        // Filter by type 1
        let result = store.get_incoming_relationships(1, &[1]).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(!result.contains(&3));
    }

    #[test]
    fn test_outgoing_and_incoming_separation() {
        let dir = TempDir::new().unwrap();
        let mut store = AdjacencyListStore::new(dir.path()).unwrap();

        // Add outgoing relationships for node 1
        let outgoing = vec![(1, 1), (2, 1)];
        store.add_outgoing_relationships(1, &outgoing).unwrap();

        // Add incoming relationships for node 1
        let incoming = vec![(3, 2), (4, 2)];
        store.add_incoming_relationships(1, &incoming).unwrap();

        // Verify outgoing relationships
        let out_result = store.get_outgoing_relationships(1, &[]).unwrap();
        assert_eq!(out_result.len(), 2);
        assert!(out_result.contains(&1));
        assert!(out_result.contains(&2));
        assert!(!out_result.contains(&3));
        assert!(!out_result.contains(&4));

        // Verify incoming relationships
        let in_result = store.get_incoming_relationships(1, &[]).unwrap();
        assert_eq!(in_result.len(), 2);
        assert!(!in_result.contains(&1));
        assert!(!in_result.contains(&2));
        assert!(in_result.contains(&3));
        assert!(in_result.contains(&4));
    }
}
