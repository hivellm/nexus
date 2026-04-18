//! [`Executor`] struct and its foundational methods: constructors, accessor
//! shims over [`ExecutorShared`], and row-lock helpers. Operator-execution
//! methods live in sibling modules (`operators/*`, `eval/*`) — each of those
//! adds its own `impl Executor { … }` block against this same type.

use super::shared::ExecutorShared;
use super::types::ExecutorConfig;
use crate::Result;
use crate::catalog::Catalog;
use crate::database::DatabaseManager;
use crate::index::{KnnIndex, LabelIndex};
use crate::relationship::{RelationshipPropertyIndex, RelationshipStorageManager};
use crate::storage::{
    RecordStore,
    row_lock::{RowLockGuard, RowLockManager},
};
use crate::udf::UdfRegistry;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Query executor.
///
/// Cloneable for concurrent execution — each clone shares the same
/// underlying data through [`ExecutorShared`]. The per-clone state
/// (query count, property-access stats) is kept distinct per clone.
pub struct Executor {
    /// Shared state (catalog, store, indexes)
    pub(super) shared: ExecutorShared,
    /// Query execution counter for lazy cache warming
    pub(super) query_count: std::sync::atomic::AtomicUsize,
    /// Property access statistics for automatic indexing
    pub(super) property_access_stats: Arc<RwLock<HashMap<String, usize>>>,
    /// Executor configuration for controlling execution behavior
    pub(super) config: ExecutorConfig,
    // JIT and parallel execution hooks are gated behind ExecutorConfig flags;
    // they remain inert until the core optimiser stabilises.
    /// Phase 8: Relationship processing optimizations enabled
    pub(super) enable_relationship_optimizations: bool,
}

impl Clone for Executor {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            query_count: std::sync::atomic::AtomicUsize::new(
                self.query_count.load(std::sync::atomic::Ordering::Relaxed),
            ),
            property_access_stats: self.property_access_stats.clone(),
            config: self.config.clone(),
            enable_relationship_optimizations: self.enable_relationship_optimizations,
        }
    }
}

impl Executor {
    /// Create a new executor with default configuration
    pub fn new(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
    ) -> Result<Self> {
        Self::new_with_config(
            catalog,
            store,
            label_index,
            knn_index,
            ExecutorConfig::default(),
        )
    }

    /// Create a new executor with custom configuration
    pub fn new_with_config(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
        config: ExecutorConfig,
    ) -> Result<Self> {
        Ok(Self {
            shared: ExecutorShared::new(catalog, store, label_index, knn_index)?,
            query_count: std::sync::atomic::AtomicUsize::new(0),
            property_access_stats: Arc::new(RwLock::new(HashMap::new())),
            config,
            enable_relationship_optimizations: true, // Phase 8: Enable by default
        })
    }

    /// Create a new executor with custom UDF registry
    pub fn with_udf_registry(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
        udf_registry: UdfRegistry,
    ) -> Result<Self> {
        Self::with_udf_registry_and_config(
            catalog,
            store,
            label_index,
            knn_index,
            udf_registry,
            ExecutorConfig::default(),
        )
    }

    /// Create a new executor with custom UDF registry and configuration
    pub fn with_udf_registry_and_config(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
        udf_registry: UdfRegistry,
        config: ExecutorConfig,
    ) -> Result<Self> {
        Ok(Self {
            shared: ExecutorShared::with_udf_registry(
                catalog,
                store,
                label_index,
                knn_index,
                udf_registry,
            )?,
            query_count: std::sync::atomic::AtomicUsize::new(0),
            property_access_stats: Arc::new(RwLock::new(HashMap::new())),
            config,
            enable_relationship_optimizations: true, // Phase 8: Enable by default
        })
    }

    /// Get reference to UDF registry
    pub fn udf_registry(&self) -> &UdfRegistry {
        &self.shared.udf_registry
    }

    /// Get mutable reference to UDF registry (creates new Arc if needed)
    pub fn udf_registry_mut(&mut self) -> &mut UdfRegistry {
        // Arc::make_mut clones if there are other strong references; the
        // caller must treat this path as read-write only when the registry
        // is uniquely owned.
        Arc::get_mut(&mut self.shared.udf_registry)
            .expect("UDF registry should be uniquely owned for mutation")
    }

    /// Set the database manager for multi-database support
    pub fn set_database_manager(
        &self,
        manager: Arc<parking_lot::RwLock<DatabaseManager>>,
    ) -> std::result::Result<(), Arc<parking_lot::RwLock<DatabaseManager>>> {
        self.shared.set_database_manager(manager)
    }

    /// Get a clone of the internal store (for syncing changes back to engine)
    pub fn get_store(&self) -> RecordStore {
        self.shared.store.read().clone()
    }

    /// Get reference to shared state (for internal use)
    pub(crate) fn shared(&self) -> &ExecutorShared {
        &self.shared
    }

    /// Phase 8: Get relationship storage manager (for synchronization)
    pub(crate) fn relationship_storage(
        &self,
    ) -> Option<&Arc<parking_lot::RwLock<RelationshipStorageManager>>> {
        self.shared.relationship_storage.as_ref()
    }

    /// Phase 8: Get relationship property index (for synchronization)
    pub(crate) fn relationship_property_index(
        &self,
    ) -> Option<&Arc<parking_lot::RwLock<RelationshipPropertyIndex>>> {
        self.shared.relationship_property_index.as_ref()
    }

    /// Get reference to catalog (for internal use).
    /// Catalog is thread-safe via LMDB transactions, so no lock needed.
    pub(super) fn catalog(&self) -> &Catalog {
        &self.shared.catalog
    }

    /// Read lock on store (guard derefs to `&RecordStore`).
    pub(super) fn store(&self) -> parking_lot::RwLockReadGuard<'_, RecordStore> {
        self.shared.store.read()
    }

    /// Write lock on store.
    pub(super) fn store_mut(&self) -> parking_lot::RwLockWriteGuard<'_, RecordStore> {
        self.shared.store.write()
    }

    /// Read lock on label_index (guard derefs to `&LabelIndex`).
    pub(super) fn label_index(&self) -> parking_lot::RwLockReadGuard<'_, LabelIndex> {
        self.shared.label_index.read()
    }

    /// Write lock on label_index.
    pub(super) fn label_index_mut(&self) -> parking_lot::RwLockWriteGuard<'_, LabelIndex> {
        self.shared.label_index.write()
    }

    /// Read lock on knn_index (guard derefs to `&KnnIndex`).
    pub(super) fn knn_index(&self) -> parking_lot::RwLockReadGuard<'_, KnnIndex> {
        self.shared.knn_index.read()
    }

    /// Write lock on knn_index.
    pub(super) fn knn_index_mut(&self) -> parking_lot::RwLockWriteGuard<'_, KnnIndex> {
        self.shared.knn_index.write()
    }

    /// Row lock manager shared across operations.
    pub(super) fn row_lock_manager(&self) -> &RowLockManager {
        &self.shared.row_lock_manager
    }

    /// Shared transaction manager (reused across operations).
    pub(super) fn transaction_manager(
        &self,
    ) -> &Arc<parking_lot::Mutex<crate::transaction::TransactionManager>> {
        &self.shared.transaction_manager
    }

    /// Generate a transaction ID for row locking.
    ///
    /// Uses a thread-id hash so that concurrent readers/writers produce
    /// distinct ids without needing a global counter.
    pub(super) fn generate_tx_id(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let thread_id = std::thread::current().id();
        let mut hasher = DefaultHasher::new();
        thread_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Acquire row locks for the two endpoints of a relationship creation.
    /// When `source_id == target_id` only a single guard is returned.
    pub(super) fn acquire_relationship_locks(
        &self,
        source_id: u64,
        target_id: u64,
    ) -> Result<(RowLockGuard, Option<RowLockGuard>)> {
        use crate::storage::row_lock::ResourceId;

        let tx_id = self.generate_tx_id();
        let lock_manager = self.row_lock_manager();

        let source_lock = lock_manager.acquire_write(tx_id, ResourceId::node(source_id))?;

        let target_lock = if source_id != target_id {
            Some(lock_manager.acquire_write(tx_id, ResourceId::node(target_id))?)
        } else {
            None
        };

        Ok((source_lock, target_lock))
    }

    /// Acquire a row lock for a single node (UPDATE path).
    pub(super) fn acquire_node_lock(&self, node_id: u64) -> Result<RowLockGuard> {
        use crate::storage::row_lock::ResourceId;

        let tx_id = self.generate_tx_id();
        let lock_manager = self.row_lock_manager();

        lock_manager.acquire_write(tx_id, ResourceId::node(node_id))
    }

    /// Acquire a row lock for a relationship (UPDATE/DELETE path).
    pub(super) fn acquire_relationship_lock(&self, rel_id: u64) -> Result<RowLockGuard> {
        use crate::storage::row_lock::ResourceId;

        let tx_id = self.generate_tx_id();
        let lock_manager = self.row_lock_manager();

        lock_manager.acquire_write(tx_id, ResourceId::relationship(rel_id))
    }
}
