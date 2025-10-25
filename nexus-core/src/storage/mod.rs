//! Storage layer - Record stores for nodes, relationships, and properties
//!
//! Neo4j-inspired record stores:
//! - `nodes.store`: Fixed-size records for nodes (label_bits, first_rel_ptr, prop_ptr, flags)
//! - `rels.store`: Fixed-size records for relationships (src, dst, type, next_src, next_dst, prop_ptr)
//! - `props.store`: Property records with overflow chains
//! - `strings.store`: String/blob dictionary with varint length + CRC
//!
//! All stores use append-only architecture with periodic compaction.
//!
//! # Record Sizes
//!
//! - NodeRecord: 32 bytes (label_bits: 8, first_rel_ptr: 8, prop_ptr: 8, flags: 4, padding: 4)
//! - RelationshipRecord: 48 bytes (src: 8, dst: 8, type: 4, next_src: 8, next_dst: 8, prop_ptr: 8, flags: 4)

use crate::{Error, Result};
use bytemuck::{Pod, Zeroable};
use memmap2::{MmapMut, MmapOptions};
use parking_lot::RwLock;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Node record in nodes.store (32 bytes, fixed-size)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct NodeRecord {
    /// Bitmap of label IDs (supports up to 64 labels)
    pub label_bits: u64,
    /// Pointer to first relationship (doubly-linked list head)
    pub first_rel_ptr: u64,
    /// Pointer to property chain
    pub prop_ptr: u64,
    /// Flags (bit 0: deleted, bit 1: locked)
    pub flags: u32,
    /// Padding to align to 32 bytes
    _padding: u32,
}

const NODE_RECORD_SIZE: usize = 32;

impl Default for NodeRecord {
    fn default() -> Self {
        Self {
            label_bits: 0,
            first_rel_ptr: u64::MAX, // NULL pointer
            prop_ptr: u64::MAX,      // NULL pointer
            flags: 0,
            _padding: 0,
        }
    }
}

impl NodeRecord {
    /// Check if node is deleted
    pub fn is_deleted(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Mark node as deleted
    pub fn set_deleted(&mut self) {
        self.flags |= 0x01;
    }

    /// Check if node has label
    pub fn has_label(&self, label_id: u32) -> bool {
        if label_id >= 64 {
            return false;
        }
        (self.label_bits & (1u64 << label_id)) != 0
    }

    /// Add label to node
    pub fn add_label(&mut self, label_id: u32) {
        if label_id < 64 {
            self.label_bits |= 1u64 << label_id;
        }
    }

    /// Remove label from node
    pub fn remove_label(&mut self, label_id: u32) {
        if label_id < 64 {
            self.label_bits &= !(1u64 << label_id);
        }
    }
}

/// Relationship record in rels.store (48 bytes, fixed-size)
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct RelationshipRecord {
    /// Source node ID
    pub src_id: u64,
    /// Destination node ID
    pub dst_id: u64,
    /// Next relationship pointer from source (linked list)
    pub next_src_ptr: u64,
    /// Next relationship pointer to destination (linked list)
    pub next_dst_ptr: u64,
    /// Pointer to property chain
    pub prop_ptr: u64,
    /// Relationship type ID
    pub type_id: u32,
    /// Flags (bit 0: deleted)
    pub flags: u32,
}

const REL_RECORD_SIZE: usize = 48;

impl Default for RelationshipRecord {
    fn default() -> Self {
        Self {
            src_id: 0,
            dst_id: 0,
            next_src_ptr: u64::MAX, // NULL
            next_dst_ptr: u64::MAX, // NULL
            prop_ptr: u64::MAX,     // NULL
            type_id: 0,
            flags: 0,
        }
    }
}

impl RelationshipRecord {
    /// Check if relationship is deleted
    pub fn is_deleted(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Mark relationship as deleted
    pub fn set_deleted(&mut self) {
        self.flags |= 0x01;
    }
}

/// Property value types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyType {
    /// Null value
    Null = 0,
    /// Boolean value
    Bool = 1,
    /// 64-bit integer value
    Int64 = 2,
    /// 64-bit floating point value
    Float64 = 3,
    /// Reference to string in strings.store
    StringRef = 4,
    /// Reference to bytes in strings.store
    BytesRef = 5,
}

/// Property record in props.store (variable size)
#[derive(Debug, Clone)]
pub struct PropertyRecord {
    /// Property key ID
    pub key_id: u32,
    /// Value type
    pub prop_type: PropertyType,
    /// Value (inline for small types, offset for strings/bytes)
    pub value: PropertyValue,
    /// Next property pointer (linked list)
    pub next_ptr: u64,
}

/// Property value union
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// 64-bit integer value
    Int64(i64),
    /// 64-bit floating point value
    Float64(f64),
    /// Reference to string in strings.store (offset)
    StringRef(u64),
    /// Reference to bytes in strings.store (offset)
    BytesRef(u64),
}

/// Memory-mapped file wrapper
struct MappedFile {
    file: File,
    mmap: MmapMut,
    path: PathBuf,
    current_size: usize,
}

impl MappedFile {
    /// Create or open a memory-mapped file
    fn new<P: AsRef<Path>>(path: P, initial_size: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();

        // Create or open file
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;

        // Get current size or resize to initial size
        let metadata = file.metadata()?;
        let current_size = metadata.len() as usize;

        let current_size = if current_size == 0 {
            // New file - resize to initial size
            file.set_len(initial_size as u64)?;
            initial_size
        } else {
            current_size
        };

        // Memory map the file
        let mmap = unsafe { MmapOptions::new().len(current_size).map_mut(&file)? };

        Ok(Self {
            file,
            mmap,
            path,
            current_size,
        })
    }

    /// Resize the file (double the size)
    fn resize(&mut self, new_size: usize) -> Result<()> {
        // Flush current mmap
        self.mmap.flush()?;

        // Drop mmap before resizing file
        drop(std::mem::replace(&mut self.mmap, unsafe {
            MmapOptions::new().len(0).map_mut(&self.file)?
        }));

        // Resize file
        self.file.set_len(new_size as u64)?;
        self.current_size = new_size;

        // Remap
        self.mmap = unsafe { MmapOptions::new().len(new_size).map_mut(&self.file)? };

        Ok(())
    }

    /// Get slice at offset
    fn get_slice(&self, offset: usize, size: usize) -> Result<&[u8]> {
        if offset + size > self.current_size {
            return Err(Error::storage(format!(
                "Read beyond file size: offset={}, size={}, file_size={}",
                offset, size, self.current_size
            )));
        }
        Ok(&self.mmap[offset..offset + size])
    }

    /// Get mutable slice at offset
    fn get_slice_mut(&mut self, offset: usize, size: usize) -> Result<&mut [u8]> {
        if offset + size > self.current_size {
            return Err(Error::storage(format!(
                "Write beyond file size: offset={}, size={}, file_size={}",
                offset, size, self.current_size
            )));
        }
        Ok(&mut self.mmap[offset..offset + size])
    }

    /// Flush to disk
    fn flush(&mut self) -> Result<()> {
        self.mmap.flush()?;
        Ok(())
    }
}

/// Record store manager
pub struct RecordStore {
    /// Directory path
    data_dir: PathBuf,

    /// Node store (nodes.store)
    nodes: Arc<RwLock<MappedFile>>,

    /// Relationship store (rels.store)
    rels: Arc<RwLock<MappedFile>>,

    /// Next node ID
    next_node_id: Arc<RwLock<u64>>,

    /// Next relationship ID
    next_rel_id: Arc<RwLock<u64>>,
}

impl RecordStore {
    /// Create a new record store
    ///
    /// # Arguments
    ///
    /// * `data_dir` - Directory path for store files
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use nexus_core::storage::RecordStore;
    ///
    /// let store = RecordStore::new("./data").unwrap();
    /// ```
    pub fn new<P: AsRef<Path>>(data_dir: P) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(&data_dir)?;

        // Initial size: 1MB for each store
        const INITIAL_SIZE: usize = 1024 * 1024;

        // Create/open stores
        let nodes_path = data_dir.join("nodes.store");
        let rels_path = data_dir.join("rels.store");

        let nodes = Arc::new(RwLock::new(MappedFile::new(&nodes_path, INITIAL_SIZE)?));
        let rels = Arc::new(RwLock::new(MappedFile::new(&rels_path, INITIAL_SIZE)?));

        Ok(Self {
            data_dir,
            nodes,
            rels,
            next_node_id: Arc::new(RwLock::new(0)),
            next_rel_id: Arc::new(RwLock::new(0)),
        })
    }

    /// Allocate a new node ID
    pub fn allocate_node_id(&self) -> u64 {
        let mut next_id = self.next_node_id.write();
        let id = *next_id;
        *next_id += 1;
        id
    }

    /// Allocate a new relationship ID
    pub fn allocate_rel_id(&self) -> u64 {
        let mut next_id = self.next_rel_id.write();
        let id = *next_id;
        *next_id += 1;
        id
    }

    /// Read a node record by ID
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nexus_core::storage::{RecordStore, NodeRecord};
    /// # let store = RecordStore::new("./data").unwrap();
    /// let node_id = store.allocate_node_id();
    /// let record = store.read_node(node_id).unwrap();
    /// ```
    pub fn read_node(&self, node_id: u64) -> Result<NodeRecord> {
        let offset = (node_id as usize) * NODE_RECORD_SIZE;

        let nodes = self.nodes.read();
        let slice = nodes.get_slice(offset, NODE_RECORD_SIZE)?;

        // Safety: We know the slice is exactly NODE_RECORD_SIZE bytes and NodeRecord is Pod
        let record = bytemuck::pod_read_unaligned::<NodeRecord>(slice);

        Ok(record)
    }

    /// Write a node record
    ///
    /// Automatically grows the file if necessary.
    pub fn write_node(&self, node_id: u64, record: &NodeRecord) -> Result<()> {
        let offset = (node_id as usize) * NODE_RECORD_SIZE;
        let required_size = offset + NODE_RECORD_SIZE;

        let mut nodes = self.nodes.write();

        // Grow file if necessary (double the size)
        if required_size > nodes.current_size {
            let new_size = (nodes.current_size * 2).max(required_size);
            nodes.resize(new_size)?;
        }

        let slice = nodes.get_slice_mut(offset, NODE_RECORD_SIZE)?;

        // Write record
        let bytes = bytemuck::bytes_of(record);
        slice.copy_from_slice(bytes);

        Ok(())
    }

    /// Read a relationship record by ID
    pub fn read_rel(&self, rel_id: u64) -> Result<RelationshipRecord> {
        let offset = (rel_id as usize) * REL_RECORD_SIZE;

        let rels = self.rels.read();
        let slice = rels.get_slice(offset, REL_RECORD_SIZE)?;

        let record = bytemuck::pod_read_unaligned::<RelationshipRecord>(slice);

        Ok(record)
    }

    /// Write a relationship record
    pub fn write_rel(&self, rel_id: u64, record: &RelationshipRecord) -> Result<()> {
        let offset = (rel_id as usize) * REL_RECORD_SIZE;
        let required_size = offset + REL_RECORD_SIZE;

        let mut rels = self.rels.write();

        // Grow file if necessary
        if required_size > rels.current_size {
            let new_size = (rels.current_size * 2).max(required_size);
            rels.resize(new_size)?;
        }

        let slice = rels.get_slice_mut(offset, REL_RECORD_SIZE)?;

        let bytes = bytemuck::bytes_of(record);
        slice.copy_from_slice(bytes);

        Ok(())
    }

    /// Flush all stores to disk
    pub fn flush(&self) -> Result<()> {
        self.nodes.write().flush()?;
        self.rels.write().flush()?;
        Ok(())
    }

    /// Get statistics
    pub fn stats(&self) -> RecordStoreStats {
        RecordStoreStats {
            node_count: *self.next_node_id.read(),
            rel_count: *self.next_rel_id.read(),
            nodes_file_size: self.nodes.read().current_size,
            rels_file_size: self.rels.read().current_size,
        }
    }
}

impl Default for RecordStore {
    fn default() -> Self {
        Self::new("./data").expect("Failed to create default record store")
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
    fn test_create_store() {
        let (store, _dir) = create_test_store();
        let stats = store.stats();
        assert_eq!(stats.node_count, 0);
        assert_eq!(stats.rel_count, 0);
    }

    #[test]
    fn test_node_crud() {
        let (store, _dir) = create_test_store();

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
        let (store, _dir) = create_test_store();

        let rel_id = store.allocate_rel_id();
        assert_eq!(rel_id, 0);

        // Create relationship record
        let mut record = RelationshipRecord::default();
        record.src_id = 10;
        record.dst_id = 20;
        record.type_id = 1;

        // Write
        store.write_rel(rel_id, &record).unwrap();

        // Read
        let read_record = store.read_rel(rel_id).unwrap();
        assert_eq!(read_record.src_id, 10);
        assert_eq!(read_record.dst_id, 20);
        assert_eq!(read_record.type_id, 1);
    }

    #[test]
    fn test_file_growth() {
        let (store, _dir) = create_test_store();

        // Write many nodes to trigger file growth
        // 50000 nodes * 32 bytes = 1.6MB (will trigger growth from initial 1MB)
        for i in 0..50000 {
            let node_id = store.allocate_node_id();
            let mut record = NodeRecord::default();
            record.add_label((i % 64) as u32);
            store.write_node(node_id, &record).unwrap();
        }

        let stats = store.stats();
        assert_eq!(stats.node_count, 50000);
        assert!(stats.nodes_file_size > 1024 * 1024); // Grew beyond initial 1MB
    }

    #[test]
    fn test_node_labels() {
        let (_store, _dir) = create_test_store();

        let mut record = NodeRecord::default();

        // Add labels
        record.add_label(0);
        record.add_label(5);
        record.add_label(63);

        assert!(record.has_label(0));
        assert!(record.has_label(5));
        assert!(record.has_label(63));
        assert!(!record.has_label(1));
        assert!(!record.has_label(64)); // Out of range

        // Remove label
        record.remove_label(5);
        assert!(!record.has_label(5));
        assert!(record.has_label(0));
        assert!(record.has_label(63));
    }

    #[test]
    fn test_node_deleted_flag() {
        let mut record = NodeRecord::default();
        assert!(!record.is_deleted());

        record.set_deleted();
        assert!(record.is_deleted());
    }

    #[test]
    fn test_linked_list_pointers() {
        let (store, _dir) = create_test_store();

        // Create chain of nodes
        let node1_id = store.allocate_node_id();
        let node2_id = store.allocate_node_id();
        let node3_id = store.allocate_node_id();

        // Node 1 points to first relationship
        let mut node1 = NodeRecord::default();
        node1.first_rel_ptr = 100;
        store.write_node(node1_id, &node1).unwrap();

        // Create relationships
        let rel1_id = store.allocate_rel_id();
        let rel2_id = store.allocate_rel_id();

        let mut rel1 = RelationshipRecord::default();
        rel1.src_id = node1_id;
        rel1.dst_id = node2_id;
        rel1.next_src_ptr = rel2_id; // Points to next relationship from src
        store.write_rel(rel1_id, &rel1).unwrap();

        let mut rel2 = RelationshipRecord::default();
        rel2.src_id = node1_id;
        rel2.dst_id = node3_id;
        rel2.next_src_ptr = u64::MAX; // End of list
        store.write_rel(rel2_id, &rel2).unwrap();

        // Traverse linked list
        let node = store.read_node(node1_id).unwrap();
        assert_eq!(node.first_rel_ptr, 100);

        let first_rel = store.read_rel(rel1_id).unwrap();
        assert_eq!(first_rel.dst_id, node2_id);
        assert_eq!(first_rel.next_src_ptr, rel2_id);

        let second_rel = store.read_rel(rel2_id).unwrap();
        assert_eq!(second_rel.dst_id, node3_id);
        assert_eq!(second_rel.next_src_ptr, u64::MAX);
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Create store and write data
        {
            let store = RecordStore::new(&path).unwrap();
            let node_id = store.allocate_node_id();

            let mut record = NodeRecord::default();
            record.add_label(42);
            record.prop_ptr = 999;

            store.write_node(node_id, &record).unwrap();
            store.flush().unwrap();
        }

        // Reopen and verify
        {
            let store = RecordStore::new(&path).unwrap();
            let record = store.read_node(0).unwrap();

            assert!(record.has_label(42));
            assert_eq!(record.prop_ptr, 999);
        }
    }

    #[test]
    fn test_rel_deleted_flag() {
        let mut record = RelationshipRecord::default();
        assert!(!record.is_deleted());

        record.set_deleted();
        assert!(record.is_deleted());
    }

    #[test]
    fn test_multiple_labels_on_node() {
        let (store, _dir) = create_test_store();

        let node_id = store.allocate_node_id();
        let mut record = NodeRecord::default();

        // Add multiple labels
        for label_id in 0..10 {
            record.add_label(label_id);
        }

        store.write_node(node_id, &record).unwrap();
        let read_record = store.read_node(node_id).unwrap();

        for label_id in 0..10 {
            assert!(read_record.has_label(label_id));
        }
    }

    #[test]
    fn test_relationship_persistence() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        {
            let store = RecordStore::new(&path).unwrap();
            let rel_id = store.allocate_rel_id();

            let mut record = RelationshipRecord::default();
            record.src_id = 100;
            record.dst_id = 200;
            record.type_id = 5;
            record.next_src_ptr = 999;

            store.write_rel(rel_id, &record).unwrap();
            store.flush().unwrap();
        }

        {
            let store = RecordStore::new(&path).unwrap();
            let record = store.read_rel(0).unwrap();

            assert_eq!(record.src_id, 100);
            assert_eq!(record.dst_id, 200);
            assert_eq!(record.type_id, 5);
            assert_eq!(record.next_src_ptr, 999);
        }
    }

    #[test]
    fn test_default_pointers() {
        let node = NodeRecord::default();
        assert_eq!(node.first_rel_ptr, u64::MAX);
        assert_eq!(node.prop_ptr, u64::MAX);

        let rel = RelationshipRecord::default();
        assert_eq!(rel.next_src_ptr, u64::MAX);
        assert_eq!(rel.next_dst_ptr, u64::MAX);
        assert_eq!(rel.prop_ptr, u64::MAX);
    }

    #[test]
    fn test_stats_reporting() {
        let (store, _dir) = create_test_store();

        for _ in 0..10 {
            store.allocate_node_id();
        }

        for _ in 0..5 {
            store.allocate_rel_id();
        }

        let stats = store.stats();
        assert_eq!(stats.node_count, 10);
        assert_eq!(stats.rel_count, 5);
        assert_eq!(stats.nodes_file_size, 1024 * 1024);
        assert_eq!(stats.rels_file_size, 1024 * 1024);
    }

    #[test]
    fn test_boundary_label_ids() {
        let mut record = NodeRecord::default();

        // Test boundary cases
        record.add_label(0); // First valid
        record.add_label(63); // Last valid
        record.add_label(64); // Invalid (out of range)

        assert!(record.has_label(0));
        assert!(record.has_label(63));
        assert!(!record.has_label(64));

        // Test removal
        record.remove_label(63);
        assert!(!record.has_label(63));

        record.remove_label(64); // Should not panic
    }

    #[test]
    fn test_large_graph_simulation() {
        let (store, _dir) = create_test_store();

        // Simulate small graph: 100 nodes, 200 relationships
        for i in 0..100 {
            let node_id = store.allocate_node_id();
            let mut record = NodeRecord::default();
            record.add_label((i % 5) as u32); // 5 different labels
            store.write_node(node_id, &record).unwrap();
        }

        for i in 0..200 {
            let rel_id = store.allocate_rel_id();
            let mut record = RelationshipRecord::default();
            record.src_id = (i % 100) as u64;
            record.dst_id = ((i + 1) % 100) as u64;
            record.type_id = (i % 3) as u32; // 3 relationship types
            store.write_rel(rel_id, &record).unwrap();
        }

        let stats = store.stats();
        assert_eq!(stats.node_count, 100);
        assert_eq!(stats.rel_count, 200);
    }

    #[test]
    fn test_concurrent_writes() {
        use std::sync::Arc;
        use std::thread;

        let dir = TempDir::new().unwrap();
        let store = Arc::new(RecordStore::new(dir.path()).unwrap());

        let mut handles = vec![];

        // Spawn threads writing nodes
        for _ in 0..10 {
            let s = store.clone();
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let node_id = s.allocate_node_id();
                    let record = NodeRecord::default();
                    s.write_node(node_id, &record).unwrap();
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let stats = store.stats();
        assert_eq!(stats.node_count, 1000);
    }
}
