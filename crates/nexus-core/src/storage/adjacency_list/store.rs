use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::types::{ADJACENCY_HEADER_SIZE, AdjacencyEntry, AdjacencyListHeader};

/// Adjacency list store for efficient relationship traversal
pub struct AdjacencyListStore {
    /// Path to the storage directory
    pub(super) path: PathBuf,
    /// Outgoing relationships file handle (shared via Arc to prevent file descriptor leaks)
    outgoing_file: Arc<File>,
    /// Incoming relationships file handle (shared via Arc to prevent file descriptor leaks)
    incoming_file: Arc<File>,
    /// Memory-mapped outgoing relationships file
    pub(super) outgoing_mmap: MmapMut,
    /// Memory-mapped incoming relationships file
    pub(super) incoming_mmap: MmapMut,
    /// Current outgoing file size
    pub(super) outgoing_file_size: usize,
    /// Current incoming file size
    pub(super) incoming_file_size: usize,
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
            outgoing_file: Arc::new(outgoing_file),
            incoming_file: Arc::new(incoming_file),
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
                // node_id field carries rel_id here; target node resolution is in Phase 3.1.2
                let entry = AdjacencyEntry {
                    node_id: rel_id,
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
                // node_id field carries rel_id here; target node resolution is in Phase 3.1.2
                let entry = AdjacencyEntry {
                    node_id: rel_id,
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
            self.incoming_mmap = unsafe { MmapOptions::new().map_mut(&*self.incoming_file)? };
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
            self.outgoing_mmap = unsafe { MmapOptions::new().map_mut(&*self.outgoing_file)? };
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

impl Clone for AdjacencyListStore {
    fn clone(&self) -> Self {
        // Clone by sharing file handles but recreating memory mappings
        Self::new(&self.path).expect("Failed to clone AdjacencyListStore")
    }
}
