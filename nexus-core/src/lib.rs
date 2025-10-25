//! Nexus Core - Property Graph Database Engine
//!
//! This crate provides the core graph database engine for Nexus, implementing:
//! - Property graph model (nodes with labels, edges with types, properties)
//! - Neo4j-inspired record stores (nodes.store, rels.store, props.store)
//! - Page cache with eviction policies (clock/2Q/TinyLFU)
//! - Write-ahead log (WAL) with MVCC by epoch
//! - Cypher subset executor (pattern matching, expand, filter, project)
//! - Multi-index subsystem (label bitmap, B-tree, full-text, KNN)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │           Cypher Executor                    │
//! │   (Pattern Match, Expand, Filter, Project)  │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │          Transaction Layer                   │
//! │        (MVCC, Locking, Isolation)           │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │            Index Layer                       │
//! │  (Label Bitmap, B-tree, Full-text, KNN)     │
//! └──────────────┬──────────────────────────────┘
//!                │
//! ┌──────────────┴──────────────────────────────┐
//! │           Storage Layer                      │
//! │  (Record Stores, Page Cache, WAL, Catalog)  │
//! └─────────────────────────────────────────────┘
//! ```

#![allow(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Allow during initial scaffolding

pub mod catalog;
pub mod error;
pub mod executor;
// pub mod graph; // Temporarily commented out due to storage dependencies
pub mod graph_construction;
pub mod graph_correlation;
pub mod graph_simple;
pub mod index;
pub mod page_cache;
pub mod retry;
pub mod storage;
pub mod transaction;
pub mod wal;

pub use error::{Error, Result};
// pub use graph::{Edge, EdgeId, Graph, GraphStats, Node, NodeId};
pub use graph_construction::{
    CircularLayout, ConnectedComponents, ForceDirectedLayout, GraphLayout, GridLayout,
    HierarchicalLayout, KMeansClustering, LayoutDirection, LayoutEdge, LayoutNode, Point2D,
};
pub use graph_correlation::NodeType;
pub use graph_simple::{
    Edge as SimpleEdge, EdgeId as SimpleEdgeId, Graph as SimpleGraph,
    GraphStats as SimpleGraphStats, Node as SimpleNode, NodeId as SimpleNodeId, PropertyValue,
};

/// Graph database engine
pub struct Engine {
    /// Storage catalog for label/type/key mappings
    pub catalog: catalog::Catalog,
    /// Record stores for nodes and relationships
    pub storage: storage::RecordStore,
    /// Page cache for memory management
    pub page_cache: page_cache::PageCache,
    /// Write-ahead log for durability
    pub wal: wal::Wal,
    /// Transaction manager for MVCC
    pub transaction_manager: transaction::TransactionManager,
    /// Index subsystem
    pub indexes: index::IndexManager,
    /// Query executor
    pub executor: executor::Executor,
}

impl Engine {
    /// Create a new engine instance with all components
    pub fn new() -> Result<Self> {
        // Create temporary directory for data
        let temp_dir = tempfile::tempdir()?;
        let data_dir = temp_dir.path();
        
        // Initialize catalog
        let catalog = catalog::Catalog::new(data_dir.join("catalog.mdb"))?;
        
        // Initialize record stores
        let storage = storage::RecordStore::new(data_dir)?;
        
        // Initialize page cache
        let page_cache = page_cache::PageCache::new(1024)?; // 1024 pages = 8MB
        
        // Initialize WAL
        let wal = wal::Wal::new(data_dir.join("wal.log"))?;
        
        // Initialize transaction manager
        let transaction_manager = transaction::TransactionManager::new()?;
        
        // Initialize index manager
        let indexes = index::IndexManager::new(data_dir.join("indexes"))?;
        
        // Initialize executor
        let executor = executor::Executor::new(
            &catalog,
            &storage,
            &indexes.label_index,
            &indexes.knn_index,
        )?;
        
        Ok(Engine {
            catalog,
            storage,
            page_cache,
            wal,
            transaction_manager,
            indexes,
            executor,
        })
    }
    
    /// Create a new engine with default configuration
    pub fn new_default() -> Result<Self> {
        Self::new()
    }
    
    /// Get engine statistics
    pub fn stats(&self) -> Result<EngineStats> {
        Ok(EngineStats {
            nodes: self.storage.node_count(),
            relationships: self.storage.relationship_count(),
            labels: self.catalog.label_count(),
            rel_types: self.catalog.rel_type_count(),
            page_cache_hits: self.page_cache.hit_count(),
            page_cache_misses: self.page_cache.miss_count(),
            wal_entries: self.wal.entry_count(),
            active_transactions: self.transaction_manager.active_count(),
        })
    }
    
    /// Execute a Cypher query
    pub fn execute_cypher(&mut self, query: &str) -> Result<executor::ResultSet> {
        let query_obj = executor::Query {
            cypher: query.to_string(),
            params: std::collections::HashMap::new(),
        };
        self.executor.execute(&query_obj)
    }
    
    /// Create a new node
    pub fn create_node(&mut self, labels: Vec<String>, properties: serde_json::Value) -> Result<u64> {
        let mut tx = self.transaction_manager.begin_write()?;
        let node_id = self.storage.create_node(&mut tx, labels, properties)?;
        self.transaction_manager.commit(&mut tx)?;
        Ok(node_id)
    }
    
    /// Create a new relationship
    pub fn create_relationship(
        &mut self,
        from: u64,
        to: u64,
        rel_type: String,
        properties: serde_json::Value,
    ) -> Result<u64> {
        let mut tx = self.transaction_manager.begin_write()?;
        let rel_id = self.storage.create_relationship(&mut tx, from, to, rel_type, properties)?;
        self.transaction_manager.commit(&mut tx)?;
        Ok(rel_id)
    }
    
    /// Get node by ID
    pub fn get_node(&mut self, id: u64) -> Result<Option<storage::NodeRecord>> {
        let tx = self.transaction_manager.begin_read()?;
        self.storage.get_node(&tx, id)
    }
    
    /// Get relationship by ID
    pub fn get_relationship(&mut self, id: u64) -> Result<Option<storage::RelationshipRecord>> {
        let tx = self.transaction_manager.begin_read()?;
        self.storage.get_relationship(&tx, id)
    }
    
    /// Perform KNN search
    pub fn knn_search(
        &self,
        label: &str,
        vector: &[f32],
        k: usize,
    ) -> Result<Vec<(u64, f32)>> {
        self.indexes.knn_search(label, vector, k)
    }
    
    /// Health check
    pub fn health_check(&self) -> Result<HealthStatus> {
        let mut status = HealthStatus {
            overall: HealthState::Healthy,
            components: std::collections::HashMap::new(),
        };
        
        // Check catalog
        match self.catalog.health_check() {
            Ok(_) => {
                status.components.insert("catalog".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status.components.insert("catalog".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }
        
        // Check storage
        match self.storage.health_check() {
            Ok(_) => {
                status.components.insert("storage".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status.components.insert("storage".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }
        
        // Check page cache
        match self.page_cache.health_check() {
            Ok(_) => {
                status.components.insert("page_cache".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status.components.insert("page_cache".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }
        
        // Check WAL
        match self.wal.health_check() {
            Ok(_) => {
                status.components.insert("wal".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status.components.insert("wal".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }
        
        // Check indexes
        match self.indexes.health_check() {
            Ok(_) => {
                status.components.insert("indexes".to_string(), HealthState::Healthy);
            }
            Err(_) => {
                status.components.insert("indexes".to_string(), HealthState::Unhealthy);
                status.overall = HealthState::Unhealthy;
            }
        }
        
        Ok(status)
    }
}

/// Engine statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineStats {
    pub nodes: u64,
    pub relationships: u64,
    pub labels: u64,
    pub rel_types: u64,
    pub page_cache_hits: u64,
    pub page_cache_misses: u64,
    pub wal_entries: u64,
    pub active_transactions: u64,
}

/// Health status
#[derive(Debug, Clone, serde::Serialize)]
pub struct HealthStatus {
    pub overall: HealthState,
    pub components: std::collections::HashMap<String, HealthState>,
}

/// Health state
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum HealthState {
    Healthy,
    Unhealthy,
    Degraded,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new().expect("Failed to create default engine")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_storage() {
        let err = Error::storage("test error");
        assert!(matches!(err, Error::Storage(_)));
        assert_eq!(err.to_string(), "Storage error: test error");
    }

    #[test]
    fn test_error_page_cache() {
        let err = Error::page_cache("cache full");
        assert!(matches!(err, Error::PageCache(_)));
    }

    #[test]
    fn test_error_wal() {
        let err = Error::wal("checkpoint failed");
        assert!(matches!(err, Error::Wal(_)));
    }

    #[test]
    fn test_error_catalog() {
        let err = Error::catalog("catalog error");
        assert!(matches!(err, Error::Catalog(_)));
        assert!(err.to_string().contains("catalog error"));
    }

    #[test]
    fn test_error_transaction() {
        let err = Error::transaction("tx failed");
        assert!(matches!(err, Error::Transaction(_)));
        assert!(err.to_string().contains("tx failed"));
    }

    #[test]
    fn test_error_index() {
        let err = Error::index("index error");
        assert!(matches!(err, Error::Index(_)));
        assert!(err.to_string().contains("index error"));
    }

    #[test]
    fn test_error_executor() {
        let err = Error::executor("exec error");
        assert!(matches!(err, Error::Executor(_)));
        assert!(err.to_string().contains("exec error"));
    }

    #[test]
    fn test_error_internal() {
        let err = Error::internal("internal error");
        assert!(matches!(err, Error::Internal(_)));
        assert!(err.to_string().contains("internal error"));
    }

    #[test]
    fn test_error_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn test_node_type_export() {
        // Test that NodeType is properly exported from the main library
        use crate::NodeType;

        let function = NodeType::Function;
        let module = NodeType::Module;
        let class = NodeType::Class;
        let variable = NodeType::Variable;
        let api = NodeType::API;

        // Test that all variants are accessible
        assert_eq!(format!("{:?}", function), "Function");
        assert_eq!(format!("{:?}", module), "Module");
        assert_eq!(format!("{:?}", class), "Class");
        assert_eq!(format!("{:?}", variable), "Variable");
        assert_eq!(format!("{:?}", api), "API");

        // Test serialization
        let json = serde_json::to_string(&api).unwrap();
        assert!(json.contains("API"));

        let deserialized: NodeType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, NodeType::API);
    }

    #[test]
    fn test_error_database() {
        let db_err = heed::Error::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "db file not found",
        ));
        let err: Error = db_err.into();
        assert!(matches!(err, Error::Database(_)));
    }

    #[test]
    fn test_error_not_found() {
        let err = Error::NotFound("node 123".to_string());
        assert!(matches!(err, Error::NotFound(_)));
        assert!(err.to_string().contains("node 123"));
    }

    #[test]
    fn test_error_invalid_id() {
        let err = Error::InvalidId("invalid node id".to_string());
        assert!(matches!(err, Error::InvalidId(_)));
        assert!(err.to_string().contains("invalid node id"));
    }

    #[test]
    fn test_error_constraint_violation() {
        let err = Error::ConstraintViolation("unique constraint violated".to_string());
        assert!(matches!(err, Error::ConstraintViolation(_)));
        assert!(err.to_string().contains("unique constraint violated"));
    }

    #[test]
    fn test_error_type_mismatch() {
        let err = Error::TypeMismatch {
            expected: "String".to_string(),
            actual: "Int64".to_string(),
        };
        assert!(matches!(err, Error::TypeMismatch { .. }));
        assert!(err.to_string().contains("String"));
        assert!(err.to_string().contains("Int64"));
    }

    #[test]
    fn test_error_cypher_syntax() {
        let err = Error::CypherSyntax("unexpected token".to_string());
        assert!(matches!(err, Error::CypherSyntax(_)));
        assert!(err.to_string().contains("unexpected token"));
    }

    #[test]
    fn test_error_debug() {
        let err = Error::Storage("test".to_string());
        let debug = format!("{:?}", err);
        assert!(debug.contains("Storage"));
    }
}
