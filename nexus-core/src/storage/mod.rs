//! Storage layer for Nexus graph database
//!
//! This module provides the core storage functionality including:
//! - Record stores for nodes and relationships
//! - File-based storage with growth capabilities
//! - Memory-mapped file access for performance
//! - CRUD operations for graph entities
//! - Property storage and retrieval

use crate::error::{Error, Result};
use memmap2::{MmapMut, MmapOptions};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod property_store;

/// Size of a node record in bytes (32 bytes)
pub const NODE_RECORD_SIZE: usize = 32;

/// Size of a relationship record in bytes (52 bytes)
pub const REL_RECORD_SIZE: usize = 52;

/// Initial file size for nodes.store (1MB)
const INITIAL_NODES_FILE_SIZE: usize = 1024 * 1024;

/// Initial file size for rels.store (1MB)
const INITIAL_RELS_FILE_SIZE: usize = 1024 * 1024;

/// Growth factor for file expansion
const FILE_GROWTH_FACTOR: f64 = 1.5;

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

/// Record store for managing nodes and relationships
pub struct RecordStore {
    /// Path to the storage directory
    path: PathBuf,
    /// Nodes file handle
    nodes_file: File,
    /// Relationships file handle
    rels_file: File,
    /// Memory-mapped nodes file
    nodes_mmap: MmapMut,
    /// Memory-mapped relationships file
    rels_mmap: MmapMut,
    /// Property store for node and relationship properties
    pub property_store: property_store::PropertyStore,
    /// Next available node ID
    next_node_id: u64,
    /// Next available relationship ID
    next_rel_id: u64,
    /// Current nodes file size
    nodes_file_size: usize,
    /// Current relationships file size
    rels_file_size: usize,
}

impl RecordStore {
    /// Create a new record store at the given path
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        std::fs::create_dir_all(&path)?;

        let nodes_path = path.join("nodes.store");
        let rels_path = path.join("rels.store");

        // Create or open nodes file
        let mut nodes_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&nodes_path)?;

        // Create or open relationships file
        let mut rels_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&rels_path)?;

        // Get file sizes
        let nodes_file_size = nodes_file.metadata()?.len() as usize;
        let rels_file_size = rels_file.metadata()?.len() as usize;

        // Initialize files if empty
        let nodes_file_size = if nodes_file_size == 0 {
            nodes_file.set_len(INITIAL_NODES_FILE_SIZE as u64)?;
            // Zero out the file to ensure it's filled with zeros
            nodes_file.write_all(&vec![0u8; INITIAL_NODES_FILE_SIZE])?;
            nodes_file.sync_all()?;
            INITIAL_NODES_FILE_SIZE
        } else {
            nodes_file_size
        };

        let rels_file_size = if rels_file_size == 0 {
            rels_file.set_len(INITIAL_RELS_FILE_SIZE as u64)?;
            // Zero out the file to ensure it's filled with zeros
            rels_file.write_all(&vec![0u8; INITIAL_RELS_FILE_SIZE])?;
            rels_file.sync_all()?;
            INITIAL_RELS_FILE_SIZE
        } else {
            rels_file_size
        };

        // Create memory mappings
        let nodes_mmap = unsafe { MmapOptions::new().map_mut(&nodes_file)? };

        let rels_mmap = unsafe { MmapOptions::new().map_mut(&rels_file)? };

        // Calculate next available IDs by scanning existing data
        // Count non-empty records (records where any field is non-zero)
        let mut next_node_id = 0u64;
        for i in 0..(nodes_file_size / NODE_RECORD_SIZE) {
            let offset = i * NODE_RECORD_SIZE;
            let slice = &nodes_mmap[offset..offset + NODE_RECORD_SIZE];
            // Check if record is non-empty (any byte is non-zero)
            if slice.iter().any(|&b| b != 0) {
                next_node_id = (i + 1) as u64;
            } else {
                // Found first empty record, stop scanning
                break;
            }
        }

        let mut next_rel_id = 0u64;
        for i in 0..(rels_file_size / REL_RECORD_SIZE) {
            let offset = i * REL_RECORD_SIZE;
            let slice = &rels_mmap[offset..offset + REL_RECORD_SIZE];
            // Check if record is non-empty (any byte is non-zero)
            if slice.iter().any(|&b| b != 0) {
                next_rel_id = (i + 1) as u64;
            } else {
                // Found first empty record, stop scanning
                break;
            }
        }

        // Initialize property store
        let property_store = property_store::PropertyStore::new(path.clone())?;

        Ok(Self {
            path,
            nodes_file,
            rels_file,
            nodes_mmap,
            rels_mmap,
            property_store,
            next_node_id,
            next_rel_id,
            nodes_file_size,
            rels_file_size,
        })
    }

    /// Allocate a new node ID
    pub fn allocate_node_id(&mut self) -> u64 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Allocate a new relationship ID
    pub fn allocate_rel_id(&mut self) -> u64 {
        let id = self.next_rel_id;
        self.next_rel_id += 1;
        id
    }

    /// Write a node record
    pub fn write_node(&mut self, node_id: u64, record: &NodeRecord) -> Result<()> {
        let offset = (node_id as usize * NODE_RECORD_SIZE) as u64;

        // Check if we need to grow the file
        if offset + NODE_RECORD_SIZE as u64 > self.nodes_file_size as u64 {
            self.grow_nodes_file()?;
        }

        // Write the record
        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        self.nodes_mmap[start..end].copy_from_slice(bytemuck::bytes_of(record));

        Ok(())
    }

    /// Read a node record
    pub fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
        let offset = (node_id as usize * NODE_RECORD_SIZE) as u64;

        if offset + NODE_RECORD_SIZE as u64 > self.nodes_file_size as u64 {
            return Err(Error::NotFound(format!("Node {} not found", node_id)));
        }

        let start = offset as usize;
        let end = start + NODE_RECORD_SIZE;
        let bytes = &self.nodes_mmap[start..end];

        Ok(*bytemuck::from_bytes(bytes))
    }

    /// Write a relationship record
    pub fn write_rel(&mut self, rel_id: u64, record: &RelationshipRecord) -> Result<()> {
        let offset = (rel_id as usize * REL_RECORD_SIZE) as u64;

        // Check if we need to grow the file
        if offset + REL_RECORD_SIZE as u64 > self.rels_file_size as u64 {
            self.grow_rels_file()?;
        }

        // Write the record
        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        self.rels_mmap[start..end].copy_from_slice(bytemuck::bytes_of(record));

        Ok(())
    }

    /// Read a relationship record
    pub fn read_rel(&self, rel_id: u64) -> Result<RelationshipRecord> {
        let offset = (rel_id as usize * REL_RECORD_SIZE) as u64;

        if offset + REL_RECORD_SIZE as u64 > self.rels_file_size as u64 {
            return Err(Error::NotFound(format!(
                "Relationship {} not found",
                rel_id
            )));
        }

        let start = offset as usize;
        let end = start + REL_RECORD_SIZE;
        let bytes = &self.rels_mmap[start..end];

        Ok(*bytemuck::from_bytes(bytes))
    }

    /// Delete a node (mark as deleted)
    pub fn delete_node(&mut self, node_id: u64) -> Result<()> {
        let mut record = self.read_node(node_id)?;
        record.mark_deleted();
        self.write_node(node_id, &record)
    }

    /// Delete a relationship (mark as deleted)
    pub fn delete_rel(&mut self, rel_id: u64) -> Result<()> {
        let mut record = self.read_rel(rel_id)?;
        record.mark_deleted();
        self.write_rel(rel_id, &record)
    }

    /// Get statistics about the record store
    pub fn stats(&self) -> RecordStoreStats {
        RecordStoreStats {
            node_count: self.next_node_id,
            rel_count: self.next_rel_id,
            nodes_file_size: self.nodes_file_size,
            rels_file_size: self.rels_file_size,
        }
    }

    /// Grow the nodes file
    fn grow_nodes_file(&mut self) -> Result<()> {
        let new_size = ((self.nodes_file_size as f64) * FILE_GROWTH_FACTOR) as usize;

        // Resize the file
        self.nodes_file.set_len(new_size as u64)?;

        // Recreate the memory mapping
        self.nodes_mmap = unsafe { MmapOptions::new().map_mut(&self.nodes_file)? };

        self.nodes_file_size = new_size;
        Ok(())
    }

    /// Grow the relationships file
    fn grow_rels_file(&mut self) -> Result<()> {
        let new_size = ((self.rels_file_size as f64) * FILE_GROWTH_FACTOR) as usize;

        // Resize the file
        self.rels_file.set_len(new_size as u64)?;

        // Recreate the memory mapping
        self.rels_mmap = unsafe { MmapOptions::new().map_mut(&self.rels_file)? };

        self.rels_file_size = new_size;
        Ok(())
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> u64 {
        self.next_node_id
    }

    /// Get the number of relationships
    pub fn relationship_count(&self) -> u64 {
        self.next_rel_id
    }

    /// Health check for the record store
    pub fn health_check(&self) -> Result<()> {
        // Check if files are accessible and readable
        if !self.path.join("nodes.store").exists() {
            return Err(Error::storage("Nodes file does not exist"));
        }
        if !self.path.join("rels.store").exists() {
            return Err(Error::storage("Relationships file does not exist"));
        }

        // Try to read from the memory-mapped files
        let _ = self.nodes_mmap.len();
        let _ = self.rels_mmap.len();

        Ok(())
    }

    /// Create a new node
    pub fn create_node(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        labels: Vec<String>,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let node_id = self.next_node_id;
        self.next_node_id += 1;

        // Create node record
        let mut record = NodeRecord::new();

        // Set label bits (for now, just use simple bit setting)
        // In a full implementation, this would map label names to IDs
        for (i, _label) in labels.iter().enumerate() {
            if i < 64 {
                record.label_bits |= 1u64 << i;
            }
        }

        // Store properties and get property pointer
        record.prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.store_properties(
                node_id,
                property_store::EntityType::Node,
                properties,
            )?
        } else {
            0
        };

        // Set first relationship pointer to 0 (no relationships yet)
        record.first_rel_ptr = 0;

        // Write the record to storage
        self.write_node(node_id, &record)?;

        Ok(node_id)
    }

    /// Create a new node with pre-computed label bits
    pub fn create_node_with_label_bits(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        label_bits: u64,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let node_id = self.next_node_id;
        self.next_node_id += 1;

        // Create node record
        let mut record = NodeRecord::new();
        record.label_bits = label_bits;

        // Store properties and get property pointer
        record.prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.store_properties(
                node_id,
                property_store::EntityType::Node,
                properties,
            )?
        } else {
            0
        };

        // Write the record
        self.write_node(node_id, &record)?;

        Ok(node_id)
    }

    /// Create a new relationship
    pub fn create_relationship(
        &mut self,
        _tx: &mut crate::transaction::Transaction,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let rel_id = self.next_rel_id;
        self.next_rel_id += 1;

        // Create relationship record
        let mut record = RelationshipRecord::new(from, to, 0);

        // Set source and destination node IDs
        record.src_id = from;
        record.dst_id = to;

        // Set relationship type ID (for now, use a simple hash)
        // In a full implementation, this would map type names to IDs via catalog
        record.type_id = rel_type.len() as u32; // Simple placeholder

        // Store properties and get property pointer
        record.prop_ptr = if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?
        } else {
            0
        };

        // Set relationship chain pointers to 0 (for now)
        // In a full implementation, this would link to existing relationships
        record.next_src_ptr = 0;
        record.next_dst_ptr = 0;

        // Write the record to storage
        self.write_rel(rel_id, &record)?;

        Ok(rel_id)
    }

    /// Get a node by ID
    pub fn get_node(
        &self,
        _tx: &crate::transaction::Transaction,
        id: u64,
    ) -> Result<Option<NodeRecord>> {
        // Check if node ID is valid
        if id >= self.next_node_id {
            return Ok(None);
        }

        // Read the node record from storage
        match self.read_node(id) {
            Ok(record) => {
                // Check if the node is deleted
                if record.is_deleted() {
                    Ok(None)
                } else {
                    Ok(Some(record))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Get a relationship by ID
    pub fn get_relationship(
        &self,
        _tx: &crate::transaction::Transaction,
        id: u64,
    ) -> Result<Option<RelationshipRecord>> {
        // Check if relationship ID is valid
        if id >= self.next_rel_id {
            return Ok(None);
        }

        // Read the relationship record from storage
        match self.read_rel(id) {
            Ok(record) => {
                // Check if the relationship is deleted
                if record.is_deleted() {
                    Ok(None)
                } else {
                    Ok(Some(record))
                }
            }
            Err(_) => Ok(None),
        }
    }

    /// Clear all data from the storage
    pub fn clear_all(&mut self) -> Result<()> {
        // Reset counters
        self.next_node_id = 0;
        self.next_rel_id = 0;

        // Truncate files to initial size
        self.nodes_file.set_len(INITIAL_NODES_FILE_SIZE as u64)?;
        self.rels_file.set_len(INITIAL_RELS_FILE_SIZE as u64)?;

        // Update file sizes
        self.nodes_file_size = INITIAL_NODES_FILE_SIZE;
        self.rels_file_size = INITIAL_RELS_FILE_SIZE;

        // Recreate memory mappings
        self.nodes_mmap = unsafe { MmapOptions::new().map_mut(&self.nodes_file)? };
        self.rels_mmap = unsafe { MmapOptions::new().map_mut(&self.rels_file)? };

        Ok(())
    }

    /// Load properties for a node
    pub fn load_node_properties(&self, node_id: u64) -> Result<Option<serde_json::Value>> {
        self.property_store
            .load_properties(node_id, property_store::EntityType::Node)
    }

    /// Load properties for a relationship
    pub fn load_relationship_properties(&self, rel_id: u64) -> Result<Option<serde_json::Value>> {
        self.property_store
            .load_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Update properties for a node
    pub fn update_node_properties(
        &mut self,
        node_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.store_properties(
                node_id,
                property_store::EntityType::Node,
                properties,
            )?;
        } else {
            self.property_store
                .delete_properties(node_id, property_store::EntityType::Node)?;
        }
        Ok(())
    }

    /// Update properties for a relationship
    pub fn update_relationship_properties(
        &mut self,
        rel_id: u64,
        properties: serde_json::Value,
    ) -> Result<()> {
        if properties.is_object() && !properties.as_object().unwrap().is_empty() {
            self.property_store.store_properties(
                rel_id,
                property_store::EntityType::Relationship,
                properties,
            )?;
        } else {
            self.property_store
                .delete_properties(rel_id, property_store::EntityType::Relationship)?;
        }
        Ok(())
    }

    /// Delete properties for a node
    pub fn delete_node_properties(&mut self, node_id: u64) -> Result<()> {
        self.property_store
            .delete_properties(node_id, property_store::EntityType::Node)
    }

    /// Delete properties for a relationship
    pub fn delete_relationship_properties(&mut self, rel_id: u64) -> Result<()> {
        self.property_store
            .delete_properties(rel_id, property_store::EntityType::Relationship)
    }

    /// Get property store statistics
    pub fn property_count(&self) -> usize {
        self.property_store.property_count()
    }
}

impl Clone for RecordStore {
    fn clone(&self) -> Self {
        // For MVP, create a new RecordStore with the same path
        // This is not a true clone but sufficient for the current implementation
        Self::new(&self.path).expect("Failed to clone RecordStore")
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_store() -> (RecordStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = RecordStore::new(dir.path()).unwrap();
        (store, dir)
    }

    #[test]
    fn test_node_record_size() {
        assert_eq!(std::mem::size_of::<NodeRecord>(), NODE_RECORD_SIZE);
    }

    #[test]
    fn test_rel_record_size() {
        assert_eq!(std::mem::size_of::<RelationshipRecord>(), REL_RECORD_SIZE);
    }

    #[test]
    fn test_node_crud() {
        let (mut store, _dir) = create_test_store();

        let node_id = store.allocate_node_id();
        assert_eq!(node_id, 0);

        // Create node record
        let mut record = NodeRecord::default();
        record.add_label(5);
        record.prop_ptr = 123;

        // Write
        store.write_node(node_id, &record).unwrap();

        // Read
        let read_record = store.read_node(node_id).unwrap();
        assert_eq!(read_record.label_bits, record.label_bits);
        assert_eq!(read_record.prop_ptr, 123);
        assert!(read_record.has_label(5));
    }

    #[test]
    fn test_relationship_crud() {
        let (mut store, _dir) = create_test_store();

        let rel_id = store.allocate_rel_id();
        assert_eq!(rel_id, 0);

        // Create relationship record
        let record = RelationshipRecord::new(10, 20, 1);

        // Write
        store.write_rel(rel_id, &record).unwrap();

        // Read
        let read_record = store.read_rel(rel_id).unwrap();
        let src_id = read_record.src_id;
        let dst_id = read_record.dst_id;
        let type_id = read_record.type_id;
        assert_eq!(src_id, 10);
        assert_eq!(dst_id, 20);
        assert_eq!(type_id, 1);
    }

    #[test]
    fn test_node_labels() {
        let (mut store, _dir) = create_test_store();

        let node_id = store.allocate_node_id();
        let mut record = NodeRecord::default();

        // Add multiple labels
        record.add_label(0);
        record.add_label(5);
        record.add_label(10);
        record.add_label(63);

        store.write_node(node_id, &record).unwrap();

        let read_record = store.read_node(node_id).unwrap();
        assert!(read_record.has_label(0));
        assert!(read_record.has_label(5));
        assert!(read_record.has_label(10));
        assert!(read_record.has_label(63));
        assert!(!read_record.has_label(1));
        assert!(!read_record.has_label(64)); // Out of range

        let labels = read_record.get_labels();
        assert_eq!(labels.len(), 4);
        assert!(labels.contains(&0));
        assert!(labels.contains(&5));
        assert!(labels.contains(&10));
        assert!(labels.contains(&63));
    }

    #[test]
    fn test_node_deletion() {
        let (mut store, _dir) = create_test_store();

        let node_id = store.allocate_node_id();
        let mut record = NodeRecord::default();
        record.add_label(5);
        store.write_node(node_id, &record).unwrap();

        // Verify node exists
        let read_record = store.read_node(node_id).unwrap();
        assert!(!read_record.is_deleted());

        // Delete node
        store.delete_node(node_id).unwrap();

        // Verify node is marked as deleted
        let deleted_record = store.read_node(node_id).unwrap();
        assert!(deleted_record.is_deleted());
    }

    #[test]
    fn test_relationship_deletion() {
        let (mut store, _dir) = create_test_store();

        let rel_id = store.allocate_rel_id();
        let record = RelationshipRecord::new(10, 20, 1);
        store.write_rel(rel_id, &record).unwrap();

        // Verify relationship exists
        let read_record = store.read_rel(rel_id).unwrap();
        assert!(!read_record.is_deleted());

        // Delete relationship
        store.delete_rel(rel_id).unwrap();

        // Verify relationship is marked as deleted
        let deleted_record = store.read_rel(rel_id).unwrap();
        assert!(deleted_record.is_deleted());
    }

    #[test]
    fn test_file_growth() {
        let (mut store, _dir) = create_test_store();

        // Write many nodes to trigger file growth
        for i in 0..50000 {
            let node_id = store.allocate_node_id();
            let mut record = NodeRecord::default();
            record.add_label((i % 64) as u32);
            store.write_node(node_id, &record).unwrap();
        }

        let stats = store.stats();
        assert_eq!(stats.node_count, 50000);
        assert!(stats.nodes_file_size > INITIAL_NODES_FILE_SIZE);
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Create store and write data
        {
            let mut store = RecordStore::new(&path).unwrap();
            let node_id = store.allocate_node_id();

            let mut record = NodeRecord::default();
            record.add_label(42);
            record.prop_ptr = 999;
            store.write_node(node_id, &record).unwrap();
        }

        // Reopen store and read data
        {
            let store = RecordStore::new(&path).unwrap();
            let read_record = store.read_node(0).unwrap();
            assert!(read_record.has_label(42));
            assert_eq!(read_record.prop_ptr, 999);
        }
    }

    #[test]
    fn test_stats() {
        let (mut store, _dir) = create_test_store();

        // Allocate some IDs
        store.allocate_node_id();
        store.allocate_node_id();
        store.allocate_rel_id();

        let stats = store.stats();
        assert_eq!(stats.node_count, 2);
        assert_eq!(stats.rel_count, 1);
        assert!(stats.nodes_file_size > 0);
        assert!(stats.rels_file_size > 0);
    }
}
