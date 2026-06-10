//! Index layer - Multi-index subsystem for fast queries
//!
//! Implements multiple index types for different query patterns:
//! - Label index: label_id → bitmap of node_ids (roaring)
//! - Property index: (label_id, key_id) → (value → set(node_id)) (B-tree)
//! - Full-text index: Tantivy per label/key
//! - KNN index: Simple cosine similarity for MVP

use crate::Result;

pub mod btree;
pub mod composite_btree;
pub mod dist;
pub mod fulltext;
pub mod fulltext_analyzer;
pub mod fulltext_registry;
pub mod fulltext_writer;
pub mod knn_index;
pub mod label_index;
pub mod pending_updates;
pub mod property_index;
pub mod rtree;

// Re-export everything that was previously reachable at `crate::index::*`
pub use dist::{DEFAULT_VECTORIZER_DIMENSION, DistSimdCosine, DistSimdL2};
pub use knn_index::{KnnConfig, KnnIndex, KnnIndexStats};
pub use label_index::{LabelIndex, LabelIndexStats};
pub use property_index::{PropertyIndex, PropertyIndexStats, PropertyValue};

/// Index manager that coordinates all index types
#[derive(Clone)]
pub struct IndexManager {
    /// Label index for fast label-based queries
    pub label_index: LabelIndex,
    /// KNN index for vector similarity search
    pub knn_index: KnnIndex,
    /// Property index for property-based queries
    pub property_index: PropertyIndex,
    /// Composite B-tree indexes keyed by (label, property list) tuple
    /// (phase6_opencypher-advanced-types §3). Registered via DDL; the
    /// planner consults the registry before falling back to a label
    /// scan + residual filter.
    pub composite_btree: composite_btree::CompositeBtreeRegistry,
    /// Named full-text search indexes (phase6_opencypher-fulltext-search).
    /// Backed by Tantivy through `fulltext::FullTextIndex`.
    pub fulltext: fulltext_registry::FullTextRegistry,
    /// Packed-Hilbert R-tree registry
    /// (phase6_rtree-index-core §7.1). Replaces the grid-backed
    /// `crate::geospatial::rtree::RTreeIndex` for the spatial
    /// query path. Registered via `CREATE SPATIAL INDEX` (and the
    /// `USING RTREE` alias from §7.5); WAL replay routes through
    /// `RTreeRegistry::apply_wal_entry`.
    pub rtree: std::sync::Arc<rtree::RTreeRegistry>,
}

impl IndexManager {
    /// Create a new index manager
    pub fn new<P: AsRef<std::path::Path>>(index_dir: P) -> Result<Self> {
        let index_dir = index_dir.as_ref();
        std::fs::create_dir_all(index_dir)?;

        let fulltext = fulltext_registry::FullTextRegistry::new();
        fulltext.set_base_dir(index_dir.join("fulltext"));
        // phase6_fulltext-wal-integration §2.2 — pull every
        // catalogued index back into memory from its on-disk
        // `_meta.json` sidecar so the registry survives process
        // restarts without requiring a WAL replay first.
        let loaded = fulltext.load_from_disk().unwrap_or(0);
        if loaded > 0 {
            tracing::info!("FTS: restored {loaded} index(es) from on-disk catalogue");
        }
        // phase6_fulltext-async-writer — async writers are opt-in.
        // Callers that want the high-throughput background commit
        // path invoke `engine.indexes().fulltext.enable_async_writers()`
        // explicitly at boot; defaulting to ON would break every
        // "add document, query back in the same test" assertion by
        // introducing `refresh_ms` commit lag on the hot read path.
        Ok(Self {
            label_index: LabelIndex::new(),
            knn_index: KnnIndex::new(DEFAULT_VECTORIZER_DIMENSION)?,
            property_index: PropertyIndex::new(),
            composite_btree: composite_btree::CompositeBtreeRegistry::new(),
            fulltext,
            rtree: std::sync::Arc::new(rtree::RTreeRegistry::new()),
        })
    }

    /// Perform KNN search
    pub fn knn_search(&self, _label: &str, vector: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        self.knn_index.search_knn(vector, k)
    }

    /// Add a node to the label index
    pub fn add_node_to_label(&self, node_id: u64, label_id: u32) -> Result<()> {
        self.label_index.add_node(node_id, &[label_id])
    }

    /// Remove a node from the label index
    pub fn remove_node_from_label(&self, node_id: u64, _label_id: u32) -> Result<()> {
        self.label_index.remove_node(node_id)
    }

    /// Health check for the index manager
    pub fn health_check(&self) -> Result<()> {
        // Check label index
        self.label_index.health_check()?;

        // Check KNN index
        // KNN index health check is already implemented

        // Check property index
        self.property_index.health_check()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_manager_creation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = IndexManager::new(temp_dir.path()).unwrap();

        // Test that all components are initialized
        assert_eq!(manager.label_index.get_stats().total_nodes, 0);
        assert_eq!(manager.knn_index.dimension(), 128);
        assert_eq!(manager.property_index.get_stats().total_entries, 0);
    }

    #[test]
    fn test_index_manager_knn_search() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = IndexManager::new(temp_dir.path()).unwrap();

        // Add some vectors
        let embedding1 = vec![1.0; 128];
        let embedding2 = vec![0.0; 128];
        manager.knn_index.add_vector(1, embedding1).unwrap();
        manager.knn_index.add_vector(2, embedding2).unwrap();

        // Search
        let query = vec![1.0; 128];
        let results = manager.knn_search("test", &query, 2).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_index_manager_label_operations() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = IndexManager::new(temp_dir.path()).unwrap();

        // Add node to label
        manager.add_node_to_label(1, 0).unwrap();
        manager.add_node_to_label(2, 0).unwrap();

        let nodes = manager.label_index.get_nodes(0).unwrap();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains(1));
        assert!(nodes.contains(2));

        // Remove node from label
        manager.remove_node_from_label(1, 0).unwrap();
        let nodes = manager.label_index.get_nodes(0).unwrap();
        assert_eq!(nodes.len(), 1);
        assert!(nodes.contains(2));
    }

    #[test]
    fn test_index_manager_health_check() {
        let temp_dir = tempfile::tempdir().unwrap();
        let manager = IndexManager::new(temp_dir.path()).unwrap();

        // Health check should pass for empty manager
        manager.health_check().unwrap();
    }
}
