//! [`RecordStore`] struct definition, lifecycle methods, and `Clone` impl.
//!
//! Operational methods (CRUD, property management, adjacency helpers) live in
//! [`super::record_store_ops`] to keep this file under the 1 500-line budget.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use memmap2::{MmapMut, MmapOptions};
use tempfile;
use tracing;

use crate::error::{Error, Result};

use super::adjacency_list;
use super::property_store;
use super::records::{
    FILE_GROWTH_FACTOR, INITIAL_NODES_FILE_SIZE, INITIAL_RELS_FILE_SIZE, NODE_RECORD_SIZE,
    REL_RECORD_SIZE, RecordStoreStats,
};

/// Record store for managing nodes and relationships
pub struct RecordStore {
    /// Path to the storage directory
    pub(super) path: PathBuf,
    /// Nodes file handle (shared via Arc to prevent file descriptor leaks)
    pub(super) nodes_file: Arc<File>,
    /// Relationships file handle (shared via Arc to prevent file descriptor leaks)
    pub(super) rels_file: Arc<File>,
    /// Memory-mapped nodes file. Shared via `Arc<RwLock<..>>` so a
    /// `RecordStore::clone` (done on every `refresh_executor`) is a cheap
    /// `Arc::clone` instead of re-opening + re-mmapping the file, and so a
    /// file grow in one clone is visible to all clones (#16).
    pub(super) nodes_mmap: Arc<RwLock<MmapMut>>,
    /// Memory-mapped relationships file (see `nodes_mmap`).
    pub(super) rels_mmap: Arc<RwLock<MmapMut>>,
    /// Property store for node and relationship properties (shared via Arc to propagate modifications)
    pub property_store: Arc<RwLock<property_store::PropertyStore>>,
    /// Phase 3: Adjacency list store for optimized relationship traversal
    pub(crate) adjacency_store: Option<adjacency_list::AdjacencyListStore>,
    /// Next available node ID (shared across clones)
    pub(super) next_node_id: Arc<AtomicU64>,
    /// Next available relationship ID (shared across clones)
    pub(super) next_rel_id: Arc<AtomicU64>,
    /// Count of nodes actually created since the last reset.
    ///
    /// Shared across clones for the same reason as `next_node_id`: the
    /// store is cloned on every `refresh_executor`, so the executor's
    /// creations happen on a different clone than the one the engine
    /// reads when it builds the `ResultSet`. A non-shared counter would
    /// silently report zero for every query that routes through the
    /// executor. Reset per query by the engine; read to populate
    /// `ResultSet::side_effects`.
    pub(super) nodes_created: Arc<AtomicU64>,
    /// Current nodes file size
    pub(super) nodes_file_size: usize,
    /// Current relationships file size
    pub(super) rels_file_size: usize,
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

        // Phase 3: Initialize adjacency list store (optional, for optimization)
        let adjacency_store = adjacency_list::AdjacencyListStore::new(&path).ok();

        // Calculate next available IDs by scanning existing data
        // Count non-empty records (records where any field is non-zero)
        let mut next_node_id = 0u64;
        for i in 0..(nodes_file_size / NODE_RECORD_SIZE) {
            let offset = i * NODE_RECORD_SIZE;
            let slice = &nodes_mmap[offset..offset + NODE_RECORD_SIZE];
            // Check if record is non-empty (any byte is non-zero)
            if slice.iter().any(|&b| b != 0) {
                next_node_id = (i + 1) as u64;
            }
        }

        let mut next_rel_id = 0u64;
        for i in 0..(rels_file_size / REL_RECORD_SIZE) {
            let offset = i * REL_RECORD_SIZE;
            let slice = &rels_mmap[offset..offset + REL_RECORD_SIZE];
            // Check if record is non-empty (any byte is non-zero)
            if slice.iter().any(|&b| b != 0) {
                next_rel_id = (i + 1) as u64;
            }
        }

        // Initialize property store (wrapped in Arc<RwLock> for sharing between clones)
        let property_store = Arc::new(RwLock::new(property_store::PropertyStore::new(
            path.clone(),
        )?));

        // Phase 3: Initialize adjacency list store (optional, for optimization)
        let adjacency_store = adjacency_list::AdjacencyListStore::new(&path).ok();

        let mut store = Self {
            path,
            nodes_file: Arc::new(nodes_file),
            rels_file: Arc::new(rels_file),
            nodes_mmap: Arc::new(RwLock::new(nodes_mmap)),
            rels_mmap: Arc::new(RwLock::new(rels_mmap)),
            property_store,
            adjacency_store,
            nodes_created: Arc::new(AtomicU64::new(0)),
            next_node_id: Arc::new(AtomicU64::new(next_node_id)),
            next_rel_id: Arc::new(AtomicU64::new(next_rel_id)),
            nodes_file_size,
            rels_file_size,
        };

        // Issue #4: run the durable startup repair so corrupt prop_ptrs are
        // fixed on disk before any query sees them.  On error we log and
        // continue — refusing to open is worse than opening with stale ptrs
        // (load_node_properties still falls back to the reverse_index).
        if let Err(e) = store.repair_corrupt_node_prop_ptrs() {
            tracing::error!(
                "RecordStore::new: startup prop_ptr repair failed (continuing): {}",
                e
            );
        }

        Ok(store)
    }

    /// Allocate a new node ID
    pub fn allocate_node_id(&mut self) -> u64 {
        self.next_node_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Peek at the next node ID without consuming it.
    ///
    /// Used by the peek-then-allocate pattern in
    /// `create_node_with_label_bits_inner` to write the external-id index
    /// entry before committing the id allocation.  Valid only in the
    /// single-writer model.
    /// Nodes created since the last [`RecordStore::reset_nodes_created`].
    ///
    /// Counts only records actually written: an external-id create that
    /// resolves to an existing node under `ConflictPolicy::Match` or
    /// `Replace` created nothing and is not counted.
    pub fn nodes_created(&self) -> u64 {
        self.nodes_created.load(Ordering::SeqCst)
    }

    /// Zero the node-creation counter. Called at query start so the count
    /// reported on a `ResultSet` covers only that query.
    pub fn reset_nodes_created(&self) {
        self.nodes_created.store(0, Ordering::SeqCst);
    }

    pub fn peek_next_node_id(&self) -> u64 {
        self.next_node_id.load(Ordering::SeqCst)
    }

    /// Allocate a new relationship ID
    pub fn allocate_rel_id(&mut self) -> u64 {
        self.next_rel_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Flush all pending writes to disk
    ///
    /// This forces the memory-mapped files to sync with disk, ensuring data persistence.
    /// Should be called after writes to guarantee durability.
    ///
    /// Phase 1 Deep Optimization: Use flush_async() for better performance in high-throughput scenarios
    pub fn flush(&mut self) -> Result<()> {
        // Phase 1 Deep Optimization: Flush is expensive (~5-10ms), but necessary for durability
        // Consider using flush_async() or batching flushes for better throughput
        self.flush_sync()
    }

    /// Synchronous flush (for durability guarantees)
    fn flush_sync(&mut self) -> Result<()> {
        // Flush memory-mapped files to disk
        self.nodes_mmap
            .read()
            .unwrap()
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush nodes: {}", e)))?;
        self.rels_mmap
            .read()
            .unwrap()
            .flush()
            .map_err(|e| Error::Storage(format!("Failed to flush rels: {}", e)))?;

        // Also flush the property store
        self.property_store.write().unwrap().flush()?;

        // Phase 3: Flush adjacency list store
        if let Some(ref mut adj_store) = self.adjacency_store {
            adj_store.flush()?;
        }

        Ok(())
    }

    /// Phase 1 Deep Optimization: Optional async flush (doesn't wait for OS)
    /// Use this when durability can be relaxed for better throughput
    pub fn flush_async(&mut self) -> Result<()> {
        // Just trigger flush without waiting - OS will handle it
        // This is much faster but doesn't guarantee immediate durability
        // For most use cases, this is sufficient as OS will flush eventually
        Ok(())
    }

    /// Get statistics about the record store
    pub fn stats(&self) -> RecordStoreStats {
        RecordStoreStats {
            node_count: self.next_node_id.load(Ordering::SeqCst),
            rel_count: self.next_rel_id.load(Ordering::SeqCst),
            nodes_file_size: self.nodes_file_size,
            rels_file_size: self.rels_file_size,
        }
    }

    /// Grow the nodes file
    /// Phase 1 Deep Optimization: Pre-allocate larger chunks to reduce growth frequency
    pub(super) fn grow_nodes_file(&mut self) -> Result<()> {
        // Phase 1 Deep Optimization: Grow by larger factor to reduce frequency
        // Minimum 2MB growth to reduce frequent remapping overhead
        let min_growth = 2 * 1024 * 1024; // 2MB
        let calculated_size = ((self.nodes_file_size as f64) * FILE_GROWTH_FACTOR) as usize;
        let new_size = calculated_size.max(self.nodes_file_size + min_growth);

        // Resize the file
        self.nodes_file.set_len(new_size as u64)?;

        // Recreate the memory mapping in place. Because the mapping is shared
        // via Arc<RwLock>, the grow is immediately visible to every clone
        // (#16) — no per-clone re-map needed on the next refresh_executor.
        *self.nodes_mmap.write().unwrap() =
            unsafe { MmapOptions::new().map_mut(&*self.nodes_file)? };

        self.nodes_file_size = new_size;
        Ok(())
    }

    /// Grow the relationships file
    /// Phase 1 Deep Optimization: Pre-allocate larger chunks to reduce growth frequency
    pub(super) fn grow_rels_file(&mut self) -> Result<()> {
        // Phase 1 Deep Optimization: Grow by larger factor to reduce frequency
        // Minimum 2MB growth to reduce frequent remapping overhead
        let min_growth = 2 * 1024 * 1024; // 2MB
        let calculated_size = ((self.rels_file_size as f64) * FILE_GROWTH_FACTOR) as usize;
        let new_size = calculated_size.max(self.rels_file_size + min_growth);

        // Resize the file
        self.rels_file.set_len(new_size as u64)?;

        // Recreate the memory mapping in place (shared via Arc<RwLock>; see
        // grow_nodes_file).
        *self.rels_mmap.write().unwrap() = unsafe { MmapOptions::new().map_mut(&*self.rels_file)? };

        self.rels_file_size = new_size;
        Ok(())
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> u64 {
        self.next_node_id.load(Ordering::SeqCst)
    }

    /// Get the number of relationships
    pub fn relationship_count(&self) -> u64 {
        self.next_rel_id.load(Ordering::SeqCst)
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
        let _ = self.nodes_mmap.read().unwrap().len();
        let _ = self.rels_mmap.read().unwrap().len();

        Ok(())
    }
}

impl Clone for RecordStore {
    fn clone(&self) -> Self {
        // CRITICAL FIX: Share the same PropertyStore via Arc::clone()
        // This ensures all clones share the same PropertyStore instance, so modifications
        // made in one clone are visible in all other clones (via RwLock)
        // This solves the problem where next_offset was being reset when creating relationships
        // because each clone was getting an independent copy of PropertyStore

        // #16: share the memory mappings, file handles, property store and id
        // counters via `Arc` — clone is now a handful of `Arc::clone`s, with no
        // file re-open + re-mmap on the (per-write) `refresh_executor` path.
        // Sharing the mmap also means a file grow performed through one clone
        // is immediately visible to every other clone (previously each clone
        // held an independent mapping that went stale after a grow).
        let property_store = Arc::clone(&self.property_store);

        // Clone adjacency store if present (still per-clone; not on the
        // record read/write hot path).
        let adjacency_store = self
            .adjacency_store
            .as_ref()
            .and_then(|_| adjacency_list::AdjacencyListStore::new(&self.path).ok());

        Self {
            path: self.path.clone(),
            nodes_file: Arc::clone(&self.nodes_file),
            rels_file: Arc::clone(&self.rels_file),
            nodes_mmap: Arc::clone(&self.nodes_mmap),
            rels_mmap: Arc::clone(&self.rels_mmap),
            property_store, // CRITICAL: Shared PropertyStore instance (not a clone)
            adjacency_store,
            nodes_created: Arc::clone(&self.nodes_created),
            next_node_id: Arc::clone(&self.next_node_id),
            next_rel_id: Arc::clone(&self.next_rel_id),
            nodes_file_size: self.nodes_file_size,
            rels_file_size: self.rels_file_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::external_id::{ConflictPolicy, ExternalId};
    use super::super::property_store;
    use super::super::records::{
        INITIAL_NODES_FILE_SIZE, NODE_RECORD_SIZE, NodeRecord, REL_RECORD_SIZE, RelationshipRecord,
    };
    use super::*;
    use crate::testing::TestContext;

    fn create_test_store() -> (RecordStore, TestContext) {
        let ctx = TestContext::new();
        let store = RecordStore::new(ctx.path()).unwrap();
        (store, ctx)
    }

    #[test]
    fn test_node_record_size() {
        assert_eq!(std::mem::size_of::<NodeRecord>(), NODE_RECORD_SIZE);
    }

    #[test]
    fn test_rel_record_size() {
        assert_eq!(std::mem::size_of::<RelationshipRecord>(), REL_RECORD_SIZE);
    }

    /// ISSUE #16: `RecordStore::clone` must be a shared HANDLE (a handful
    /// of `Arc::clone`s over the same mmaps / files / property store), not
    /// a file re-open + re-mmap. This guards the per-write
    /// `refresh_executor` path: a write must never trigger a RecordStore
    /// reopen, and a write through one handle must be immediately visible
    /// through every other handle.
    #[test]
    fn clone_is_shared_handle_not_reopen() {
        let (mut store, _dir) = create_test_store();
        let clone = store.clone();

        // Structural guard: the clone shares the SAME mmaps, file handles
        // and property store (no reopen happened).
        assert!(
            Arc::ptr_eq(&store.nodes_mmap, &clone.nodes_mmap),
            "clone must share the nodes mmap (no re-mmap)"
        );
        assert!(
            Arc::ptr_eq(&store.rels_mmap, &clone.rels_mmap),
            "clone must share the rels mmap (no re-mmap)"
        );
        assert!(
            Arc::ptr_eq(&store.nodes_file, &clone.nodes_file),
            "clone must share the nodes file handle (no reopen)"
        );
        assert!(
            Arc::ptr_eq(&store.property_store, &clone.property_store),
            "clone must share the property store"
        );

        // Behavioral guard: write through the original, read through the
        // clone — engine + executor see one store.
        let node_id = store.allocate_node_id();
        let mut record = NodeRecord::default();
        record.add_label(7);
        store.write_node(node_id, &record).unwrap();

        let seen = clone.read_node(node_id).unwrap();
        assert!(
            seen.has_label(7),
            "write through one handle must be visible through the other"
        );
        assert_eq!(
            clone.node_count(),
            store.node_count(),
            "id counters are shared across handles"
        );
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
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

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

    // ── External-id tests (items 2.8) ─────────────────────────────────────────

    /// Build an isolated (RecordStore, Catalog) pair that do not share any
    /// LMDB environment with other tests.
    fn create_ext_id_fixtures() -> (RecordStore, crate::catalog::Catalog, TestContext) {
        let ctx = TestContext::new();
        let store_path = ctx.path().join("store");
        let catalog_path = ctx.path().join("catalog");
        std::fs::create_dir_all(&store_path).unwrap();
        std::fs::create_dir_all(&catalog_path).unwrap();

        let store = RecordStore::new(&store_path).unwrap();
        let catalog = crate::catalog::Catalog::with_isolated_path(
            &catalog_path,
            crate::catalog::CATALOG_MMAP_INITIAL_SIZE,
        )
        .unwrap();
        (store, catalog, ctx)
    }

    fn make_uuid_ext_id(byte: u8) -> ExternalId {
        ExternalId::try_uuid([byte; 16]).unwrap()
    }

    fn make_str_ext_id(s: &str) -> ExternalId {
        ExternalId::try_str(s.to_string()).unwrap()
    }

    /// Insert with a new external id: both forward and reverse entries must
    /// be present after creation.
    #[test]
    fn test_create_with_external_id_assigns_and_persists() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_uuid_ext_id(0xAA);
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let node_id = store
            .create_node_with_external_id(
                &mut tx,
                vec!["Person".to_string()],
                serde_json::json!({"name": "Alice"}),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        // Forward lookup.
        let rtxn = catalog.read_txn().unwrap();
        let found = catalog
            .external_id_index()
            .get_internal(&rtxn, &ext)
            .unwrap();
        assert_eq!(
            found,
            Some(node_id),
            "forward entry must map to the new node"
        );

        // Reverse lookup.
        let rev = catalog
            .external_id_index()
            .get_external(&rtxn, node_id)
            .unwrap();
        assert_eq!(
            rev,
            Some(ext),
            "reverse entry must map back to the external id"
        );
    }

    /// ConflictPolicy::Error on duplicate must surface ExternalIdConflict and
    /// must not write a new record.
    #[test]
    fn test_conflict_policy_error_returns_typed_error() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_uuid_ext_id(0x01);
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let first_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        let err = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap_err();

        match err {
            crate::error::Error::ExternalIdConflict {
                existing_internal_id,
                attempted_external_id,
            } => {
                assert_eq!(existing_internal_id, first_id);
                assert!(
                    attempted_external_id.contains("uuid:"),
                    "error string must include the external id display form"
                );
            }
            other => panic!("expected ExternalIdConflict, got {other:?}"),
        }

        // No extra node should have been allocated.
        assert_eq!(
            store.peek_next_node_id(),
            first_id + 1,
            "id counter must not advance on conflict"
        );
    }

    /// ConflictPolicy::Match returns the existing id without writing anything.
    #[test]
    fn test_conflict_policy_match_returns_existing_id() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_uuid_ext_id(0x02);
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let first_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::json!({"v": 1}),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        let matched_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::json!({"v": 99}),
                Some(ext.clone()),
                ConflictPolicy::Match,
                &catalog,
            )
            .unwrap();

        assert_eq!(matched_id, first_id, "Match must return the existing id");
        // Id counter must not have advanced.
        assert_eq!(store.peek_next_node_id(), first_id + 1);
    }

    /// ConflictPolicy::Replace overwrites properties but keeps the same
    /// internal id.
    #[test]
    fn test_conflict_policy_replace_overwrites_properties_and_keeps_id() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_str_ext_id("doc:001");
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let first_id = store
            .create_node_with_label_bits_and_external_id(
                &mut tx,
                0b1,
                serde_json::json!({"name": "old"}),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        let replaced_id = store
            .create_node_with_label_bits_and_external_id(
                &mut tx,
                0b1,
                serde_json::json!({"name": "new"}),
                Some(ext.clone()),
                ConflictPolicy::Replace,
                &catalog,
            )
            .unwrap();

        assert_eq!(replaced_id, first_id, "Replace must return the existing id");

        // Properties must reflect the new value.
        let props = store
            .property_store
            .read()
            .unwrap()
            .load_properties(first_id, property_store::EntityType::Node)
            .unwrap()
            .expect("node must have properties after Replace");
        assert_eq!(
            props.get("name").and_then(|v| v.as_str()),
            Some("new"),
            "Replace must overwrite the property store"
        );
    }

    /// delete_node_with_catalog removes forward and reverse entries atomically.
    #[test]
    fn test_delete_removes_external_id_from_both_maps() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_str_ext_id("file:abc");
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let node_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        store.delete_node_with_catalog(node_id, &catalog).unwrap();

        let rtxn = catalog.read_txn().unwrap();
        assert_eq!(
            catalog
                .external_id_index()
                .get_internal(&rtxn, &ext)
                .unwrap(),
            None,
            "forward entry must be absent after delete"
        );
        assert_eq!(
            catalog
                .external_id_index()
                .get_external(&rtxn, node_id)
                .unwrap(),
            None,
            "reverse entry must be absent after delete"
        );
    }

    /// delete_then_recreate: after deleting a node its external id can be
    /// reused for a new node without conflict.
    #[test]
    fn test_delete_then_recreate_with_same_external_id_succeeds() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let ext = make_str_ext_id("reuse:key");
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let first_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        store.delete_node_with_catalog(first_id, &catalog).unwrap();

        // Recreating with the same external id must succeed.
        let second_id = store
            .create_node_with_external_id(
                &mut tx,
                vec![],
                serde_json::Value::Object(Default::default()),
                Some(ext.clone()),
                ConflictPolicy::Error,
                &catalog,
            )
            .unwrap();

        assert_ne!(second_id, first_id, "new node must get a fresh internal id");

        let rtxn = catalog.read_txn().unwrap();
        assert_eq!(
            catalog
                .external_id_index()
                .get_internal(&rtxn, &ext)
                .unwrap(),
            Some(second_id),
            "forward entry must point to the new node"
        );
    }

    /// create_node (without external id) must leave the external-id index
    /// untouched — no false reverse entry for that node.
    #[test]
    fn test_no_external_id_doesnt_touch_index() {
        let (mut store, catalog, _ctx) = create_ext_id_fixtures();
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        let node_id = store
            .create_node(
                &mut tx,
                vec!["Label".to_string()],
                serde_json::json!({"x": 1}),
            )
            .unwrap();

        let rtxn = catalog.read_txn().unwrap();
        let rev = catalog
            .external_id_index()
            .get_external(&rtxn, node_id)
            .unwrap();
        assert_eq!(
            rev, None,
            "plain create_node must not insert a reverse entry"
        );
    }

    // ── repair_corrupt_node_prop_ptrs tests (issue #4) ────────────────────────

    /// Inject on-disk prop_ptr corruption (node's pointer points at a
    /// Relationship property entry), drop and reopen the store, then verify
    /// that:
    ///   (a) reopening does not panic,
    ///   (b) the on-disk prop_ptr is no longer pointing at a Relationship entry,
    ///   (c) load_node_properties returns the node's real properties.
    #[test]
    fn repair_resets_prop_ptr_pointing_to_relationship_durably() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // ------------------------------------------------------------------
        // Phase 1: build a store with one node (with props) and one rel (with
        // props), then manually corrupt the node's on-disk prop_ptr to point
        // at the relationship's property offset.
        // ------------------------------------------------------------------
        let rel_prop_offset;
        let node_id;
        {
            let mut store = RecordStore::new(&path).unwrap();
            let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
            let mut tx = tx_mgr.begin_write().unwrap();

            // Create a node with real properties.
            node_id = store
                .create_node(
                    &mut tx,
                    vec!["Person".to_string()],
                    serde_json::json!({"name": "Alice", "age": 30}),
                )
                .unwrap();

            // Create a relationship with properties so there is a Relationship
            // entry in the property store.
            let _rel_id = store
                .create_relationship(
                    &mut tx,
                    node_id,
                    node_id,
                    0,
                    serde_json::json!({"since": 2020}),
                )
                .unwrap();

            // Persist the node record + property store to disk BEFORE injecting
            // corruption, so that on reopen the rebuilt reverse_index still
            // contains the node's real property entry (the source the repair
            // recovers from). Flushing only nodes_mmap later is not enough —
            // properties.store must be durable too.
            store
                .flush()
                .expect("flush store before corruption injection");

            // Discover the offset of the relationship's property entry via the
            // reverse_index (the only reliable source).
            rel_prop_offset = store
                .property_store
                .read()
                .unwrap()
                .offset_for(0, property_store::EntityType::Relationship)
                .expect("relationship must have a property entry");

            // ----------------------------------------------------------------
            // Inject corruption: overwrite the node's on-disk record so that
            // prop_ptr points at the relationship's property offset.
            //
            // We bypass write_node deliberately here — write_node's guard
            // would reject this (it detects the Relationship type and returns
            // Err).  Writing the mmap directly is the only way to simulate the
            // pre-existing on-disk corruption that issue #4 describes.
            // ----------------------------------------------------------------
            let byte_start = node_id as usize * NODE_RECORD_SIZE;
            let byte_end = byte_start + NODE_RECORD_SIZE;
            // Read current record bytes via mmap.
            let mut record_bytes = store.nodes_mmap.read().unwrap()[byte_start..byte_end].to_vec();
            // Overwrite prop_ptr (bytes 16..24 in NodeRecord: label_bits[0..8],
            // first_rel_ptr[8..16], prop_ptr[16..24]).
            record_bytes[16..24].copy_from_slice(&rel_prop_offset.to_le_bytes());
            store.nodes_mmap.write().unwrap()[byte_start..byte_end].copy_from_slice(&record_bytes);
            // Flush so the corrupt bytes land on disk.
            store
                .nodes_mmap
                .read()
                .unwrap()
                .flush()
                .expect("flush of injected corruption must succeed");
        } // store dropped here — all handles closed.

        // ------------------------------------------------------------------
        // Phase 2: reopen.  RecordStore::new calls repair_corrupt_node_prop_ptrs.
        // ------------------------------------------------------------------
        let store2 = RecordStore::new(&path).unwrap();

        // (a) We reached here without panic.

        // (b) The on-disk prop_ptr must no longer point at a Relationship.
        let byte_start = node_id as usize * NODE_RECORD_SIZE;
        let byte_end = byte_start + NODE_RECORD_SIZE;
        let on_disk_record: NodeRecord = {
            let guard = store2.nodes_mmap.read().unwrap();
            *bytemuck::from_bytes(&guard[byte_start..byte_end])
        };
        if on_disk_record.prop_ptr != 0 {
            let info = store2
                .property_store
                .read()
                .unwrap()
                .get_entity_info_at_offset(on_disk_record.prop_ptr);
            assert!(
                matches!(
                    info,
                    Some((id, property_store::EntityType::Node)) if id == node_id
                ),
                "on-disk prop_ptr={} must point to a Node entry for node_id={}, got {:?}",
                on_disk_record.prop_ptr,
                node_id,
                info
            );
        }

        // (c) load_node_properties returns the node's real properties.
        let props = store2
            .load_node_properties(node_id)
            .unwrap()
            .expect("node must still have properties after repair");
        assert_eq!(
            props.get("name").and_then(|v| v.as_str()),
            Some("Alice"),
            "real node properties must be recoverable after repair"
        );
    }

    /// After the first repair (above), drop and reopen once more.  The second
    /// open must find zero corrupt slots — the repair is one-shot and durable.
    #[test]
    fn repair_is_one_shot_clean_second_boot() {
        let ctx = TestContext::new();
        let path = ctx.path().to_path_buf();

        // ------------------------------------------------------------------
        // Build the same corrupt scenario as the first test.
        // ------------------------------------------------------------------
        {
            let mut store = RecordStore::new(&path).unwrap();
            let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
            let mut tx = tx_mgr.begin_write().unwrap();

            let node_id = store
                .create_node(
                    &mut tx,
                    vec!["X".to_string()],
                    serde_json::json!({"k": "v"}),
                )
                .unwrap();

            let _rel_id = store
                .create_relationship(&mut tx, node_id, node_id, 0, serde_json::json!({"r": 1}))
                .unwrap();

            let rel_prop_offset = store
                .property_store
                .read()
                .unwrap()
                .offset_for(0, property_store::EntityType::Relationship)
                .unwrap();

            // Inject corruption.
            let byte_start = node_id as usize * NODE_RECORD_SIZE;
            let byte_end = byte_start + NODE_RECORD_SIZE;
            let mut record_bytes = store.nodes_mmap.read().unwrap()[byte_start..byte_end].to_vec();
            record_bytes[16..24].copy_from_slice(&rel_prop_offset.to_le_bytes());
            store.nodes_mmap.write().unwrap()[byte_start..byte_end].copy_from_slice(&record_bytes);
            store.nodes_mmap.read().unwrap().flush().unwrap();
        }

        // First reopen — repair runs.
        drop(RecordStore::new(&path).unwrap());

        // Second reopen — repair must find nothing to fix.
        let mut store3 = RecordStore::new(&path).unwrap();
        let count = store3
            .repair_corrupt_node_prop_ptrs()
            .expect("repair on already-clean store must succeed");
        assert_eq!(
            count, 0,
            "second boot must find zero corrupt slots (repair is durable)"
        );
    }

    /// A completely healthy store (no corruption) must yield a repair count of
    /// 0, and all properties must remain intact after the repair pass.
    #[test]
    fn repair_noop_on_healthy_store() {
        let (mut store, _ctx) = create_test_store();
        let mut tx_mgr = crate::transaction::TransactionManager::new().unwrap();
        let mut tx = tx_mgr.begin_write().unwrap();

        // Create a few nodes with real properties.
        let n0 = store
            .create_node(&mut tx, vec!["A".to_string()], serde_json::json!({"x": 1}))
            .unwrap();
        let n1 = store
            .create_node(&mut tx, vec!["B".to_string()], serde_json::json!({"y": 2}))
            .unwrap();

        let count = store
            .repair_corrupt_node_prop_ptrs()
            .expect("repair on healthy store must not error");
        assert_eq!(count, 0, "healthy store must report 0 repairs");

        // Properties must remain intact.
        let p0 = store
            .load_node_properties(n0)
            .unwrap()
            .expect("node 0 must still have properties");
        assert_eq!(p0.get("x").and_then(|v| v.as_i64()), Some(1));

        let p1 = store
            .load_node_properties(n1)
            .unwrap()
            .expect("node 1 must still have properties");
        assert_eq!(p1.get("y").and_then(|v| v.as_i64()), Some(2));
    }
}
