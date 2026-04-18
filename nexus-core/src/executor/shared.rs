//! [`ExecutorShared`] — thread-safe state shared between executor clones.
//! Holds the catalog, record store, indexes, caches, lock manager, and
//! auxiliary optimisers. Fields are `pub(super)` so the surrounding
//! `executor` module can read them directly without going through accessor
//! shims that would bloat the call graph.

use crate::Result;
use crate::catalog::Catalog;
use crate::database::DatabaseManager;
use crate::geospatial::rtree::RTreeIndex as SpatialIndex;
use crate::index::{KnnIndex, LabelIndex};
use crate::query_cache::{IntelligentQueryCache, QueryCacheConfig};
use crate::relationship::{
    AdvancedTraversalEngine, RelationshipPropertyIndex, RelationshipStorageManager,
};
use crate::storage::{RecordStore, row_lock::RowLockManager};
use crate::udf::UdfRegistry;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Shared executor state for concurrent execution.
///
/// Contains all components that can be safely shared across threads.
#[derive(Clone)]
pub struct ExecutorShared {
    /// Catalog for label/type lookups (thread-safe via LMDB transactions)
    pub(super) catalog: Catalog,
    /// Record store for data access (thread-safe via transactions)
    pub(super) store: Arc<RwLock<RecordStore>>,
    /// Label index for fast label scans (needs RwLock for concurrent access)
    pub(super) label_index: Arc<RwLock<LabelIndex>>,
    /// KNN index for vector operations (needs RwLock for concurrent access)
    pub(super) knn_index: Arc<RwLock<KnnIndex>>,
    /// UDF registry for user-defined functions (immutable, can be shared)
    pub(super) udf_registry: Arc<UdfRegistry>,
    /// Spatial indexes (`label.property` -> `RTreeIndex`)
    pub(super) spatial_indexes: Arc<parking_lot::RwLock<HashMap<String, SpatialIndex>>>,
    /// Multi-layer cache system for performance optimization
    pub(super) cache: Option<Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>>,
    /// Intelligent query cache for Cypher query results
    pub(super) query_cache: Option<Arc<RwLock<IntelligentQueryCache>>>,
    /// Row-level lock manager for fine-grained concurrency control
    pub(super) row_lock_manager: Arc<RowLockManager>,
    /// Phase 8.1: Specialized relationship storage manager
    pub(super) relationship_storage: Option<Arc<parking_lot::RwLock<RelationshipStorageManager>>>,
    /// Phase 8.2: Advanced traversal engine for optimized relationship queries
    pub(super) traversal_engine: Option<Arc<AdvancedTraversalEngine>>,
    /// Phase 8.3: Relationship property index for fast property-based queries
    pub(super) relationship_property_index:
        Option<Arc<parking_lot::RwLock<RelationshipPropertyIndex>>>,
    /// Shared transaction manager for write operations (avoids creating new manager per operation)
    pub(super) transaction_manager: Arc<parking_lot::Mutex<crate::transaction::TransactionManager>>,
    /// Database manager for multi-database support (optional for backward compatibility)
    pub(super) database_manager: std::sync::OnceLock<Arc<parking_lot::RwLock<DatabaseManager>>>,
}

impl ExecutorShared {
    /// Create new shared executor state
    pub fn new(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
    ) -> Result<Self> {
        // Phase 8: Initialize relationship optimizations
        let relationship_storage =
            Arc::new(parking_lot::RwLock::new(RelationshipStorageManager::new()));
        let traversal_engine = Arc::new(AdvancedTraversalEngine::new(relationship_storage.clone()));
        let relationship_property_index =
            Arc::new(parking_lot::RwLock::new(RelationshipPropertyIndex::new()));

        // Create shared transaction manager (reused across operations)
        let transaction_manager = Arc::new(parking_lot::Mutex::new(
            crate::transaction::TransactionManager::new()?,
        ));

        Ok(Self {
            catalog: catalog.clone(),
            store: Arc::new(RwLock::new(store.clone())),
            label_index: Arc::new(RwLock::new(label_index.clone())),
            knn_index: Arc::new(RwLock::new(knn_index.clone())),
            udf_registry: Arc::new(UdfRegistry::new()),
            spatial_indexes: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            cache: None,
            query_cache: None,
            row_lock_manager: Arc::new(RowLockManager::default()),
            relationship_storage: Some(relationship_storage),
            traversal_engine: Some(traversal_engine),
            relationship_property_index: Some(relationship_property_index),
            transaction_manager,
            database_manager: std::sync::OnceLock::new(),
        })
    }

    /// Set the database manager for multi-database support
    pub fn set_database_manager(
        &self,
        manager: Arc<parking_lot::RwLock<DatabaseManager>>,
    ) -> std::result::Result<(), Arc<parking_lot::RwLock<DatabaseManager>>> {
        self.database_manager.set(manager)
    }

    /// Get the database manager
    pub fn database_manager(&self) -> Option<&Arc<parking_lot::RwLock<DatabaseManager>>> {
        self.database_manager.get()
    }

    /// Set the cache system for the executor
    pub fn set_cache(&mut self, cache: Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>) {
        self.cache = Some(cache);
    }

    /// Set the intelligent query cache for the executor
    pub fn set_query_cache(&mut self, query_cache: Arc<RwLock<IntelligentQueryCache>>) {
        self.query_cache = Some(query_cache);
    }

    /// Enable intelligent query caching with default configuration
    pub fn enable_query_cache(&mut self) -> Result<()> {
        let cache = Arc::new(RwLock::new(IntelligentQueryCache::new_default()));
        self.set_query_cache(cache);
        Ok(())
    }

    /// Enable intelligent query caching with custom configuration
    pub fn enable_query_cache_with_config(&mut self, config: QueryCacheConfig) -> Result<()> {
        let cache = Arc::new(RwLock::new(IntelligentQueryCache::new(config)));
        self.set_query_cache(cache);
        Ok(())
    }

    /// Create shared state with custom UDF registry
    pub fn with_udf_registry(
        catalog: &Catalog,
        store: &RecordStore,
        label_index: &LabelIndex,
        knn_index: &KnnIndex,
        udf_registry: UdfRegistry,
    ) -> Result<Self> {
        // Phase 8: Initialize relationship optimizations
        let relationship_storage =
            Arc::new(parking_lot::RwLock::new(RelationshipStorageManager::new()));
        let traversal_engine = Arc::new(AdvancedTraversalEngine::new(relationship_storage.clone()));
        let relationship_property_index =
            Arc::new(parking_lot::RwLock::new(RelationshipPropertyIndex::new()));

        // Create shared transaction manager (reused across operations)
        let transaction_manager = Arc::new(parking_lot::Mutex::new(
            crate::transaction::TransactionManager::new()?,
        ));

        Ok(Self {
            catalog: catalog.clone(),
            store: Arc::new(RwLock::new(store.clone())),
            label_index: Arc::new(RwLock::new(label_index.clone())),
            knn_index: Arc::new(RwLock::new(knn_index.clone())),
            udf_registry: Arc::new(udf_registry),
            spatial_indexes: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            cache: None,
            query_cache: None,
            row_lock_manager: Arc::new(RowLockManager::default()),
            relationship_storage: Some(relationship_storage),
            traversal_engine: Some(traversal_engine),
            relationship_property_index: Some(relationship_property_index),
            transaction_manager,
            database_manager: std::sync::OnceLock::new(),
        })
    }
}
