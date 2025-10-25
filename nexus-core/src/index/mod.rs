//! Index layer - Multi-index subsystem for fast queries
//!
//! Implements multiple index types for different query patterns:
//! - Label index: label_id → bitmap of node_ids (roaring)
//! - Property index: (label_id, key_id) → (value → set(node_id)) (B-tree)
//! - Full-text index: Tantivy per label/key
//! - KNN index: Simple cosine similarity for MVP

use crate::{Error, Result};
use hnsw_rs::prelude::*;
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Type alias for property index trees
type PropertyIndexTree = BTreeMap<PropertyValue, RoaringBitmap>;

/// Label bitmap index using roaring bitmaps
///
/// Maps label_id → bitmap of node_ids for fast label-based queries.
/// Uses RoaringBitmap for efficient compression and operations.
pub struct LabelIndex {
    /// Mapping from label_id to bitmap of node_ids
    label_bitmaps: Arc<RwLock<HashMap<u32, RoaringBitmap>>>,
    /// Statistics
    stats: Arc<RwLock<LabelIndexStats>>,
}

/// Statistics for label index
#[derive(Debug, Clone, Default)]
pub struct LabelIndexStats {
    /// Total number of nodes indexed
    pub total_nodes: u64,
    /// Number of unique labels
    pub label_count: u32,
    /// Average nodes per label
    pub avg_nodes_per_label: f64,
}

impl LabelIndex {
    /// Create a new label index
    pub fn new() -> Self {
        Self {
            label_bitmaps: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(LabelIndexStats::default())),
        }
    }

    /// Add a node with given labels
    pub fn add_node(&self, node_id: u64, label_ids: &[u32]) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();
        let mut stats = self.stats.write();

        for &label_id in label_ids {
            bitmaps.entry(label_id).or_default().insert(node_id as u32);
        }

        stats.total_nodes += 1;
        stats.label_count = bitmaps.len() as u32;
        stats.avg_nodes_per_label = if stats.label_count > 0 {
            stats.total_nodes as f64 / stats.label_count as f64
        } else {
            0.0
        };

        Ok(())
    }

    /// Remove a node from all labels
    pub fn remove_node(&self, node_id: u64) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();
        let mut stats = self.stats.write();

        for bitmap in bitmaps.values_mut() {
            bitmap.remove(node_id as u32);
        }

        stats.total_nodes = stats.total_nodes.saturating_sub(1);
        stats.label_count = bitmaps.len() as u32;
        stats.avg_nodes_per_label = if stats.label_count > 0 {
            stats.total_nodes as f64 / stats.label_count as f64
        } else {
            0.0
        };

        Ok(())
    }

    /// Get all nodes with a specific label
    pub fn get_nodes(&self, label_id: u32) -> Result<RoaringBitmap> {
        let bitmaps = self.label_bitmaps.read();
        Ok(bitmaps.get(&label_id).cloned().unwrap_or_default())
    }

    /// Get nodes that have ALL specified labels (intersection)
    pub fn get_nodes_with_labels(&self, label_ids: &[u32]) -> Result<RoaringBitmap> {
        if label_ids.is_empty() {
            return Ok(RoaringBitmap::new());
        }

        let bitmaps = self.label_bitmaps.read();
        let mut result = bitmaps.get(&label_ids[0]).cloned().unwrap_or_default();

        for &label_id in &label_ids[1..] {
            if let Some(bitmap) = bitmaps.get(&label_id) {
                result &= bitmap;
            } else {
                return Ok(RoaringBitmap::new());
            }
        }

        Ok(result)
    }

    /// Get nodes that have ANY of the specified labels (union)
    pub fn get_nodes_with_any_labels(&self, label_ids: &[u32]) -> Result<RoaringBitmap> {
        let bitmaps = self.label_bitmaps.read();
        let mut result = RoaringBitmap::new();

        for &label_id in label_ids {
            if let Some(bitmap) = bitmaps.get(&label_id) {
                result |= bitmap;
            }
        }

        Ok(result)
    }

    /// Estimate cardinality for a label
    pub fn estimate_cardinality(&self, label_id: u32) -> u64 {
        let bitmaps = self.label_bitmaps.read();
        bitmaps.get(&label_id).map(|b| b.len()).unwrap_or(0)
    }

    /// Get statistics
    pub fn get_stats(&self) -> LabelIndexStats {
        self.stats.read().clone()
    }

    /// Check if a label exists
    pub fn has_label(&self, label_id: u32) -> bool {
        let bitmaps = self.label_bitmaps.read();
        bitmaps.contains_key(&label_id)
    }

    /// Get all label IDs
    pub fn get_all_labels(&self) -> Vec<u32> {
        let bitmaps = self.label_bitmaps.read();
        bitmaps.keys().copied().collect()
    }

    /// Clear all data
    pub fn clear(&mut self) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();
        bitmaps.clear();

        let mut stats = self.stats.write();
        *stats = LabelIndexStats::default();

        Ok(())
    }
}

impl Default for LabelIndex {
    fn default() -> Self {
        Self::new()
    }
}

/// KNN vector index using HNSW (Hierarchical Navigable Small World)
///
/// Maps node_id → embedding for fast similarity search.
/// Uses HNSW algorithm for sub-linear search complexity.
pub struct KnnIndex {
    /// HNSW index for fast KNN search
    hnsw: Arc<RwLock<Hnsw<'static, f32, DistCosine>>>,
    /// Mapping from node_id to vector index in HNSW
    node_to_index: Arc<RwLock<HashMap<u64, usize>>>,
    /// Mapping from vector index to node_id
    index_to_node: Arc<RwLock<HashMap<usize, u64>>>,
    /// Vector dimension
    dimension: usize,
    /// Statistics
    stats: Arc<RwLock<KnnIndexStats>>,
    /// Next available index
    next_index: Arc<RwLock<usize>>,
}

/// Statistics for KNN index
#[derive(Debug, Clone, Default)]
pub struct KnnIndexStats {
    /// Total number of vectors indexed
    pub total_vectors: u64,
    /// Vector dimension
    pub dimension: usize,
    /// Average search time in microseconds
    pub avg_search_time_us: f64,
}

impl KnnIndex {
    /// Create a new KNN index
    ///
    /// # Arguments
    /// * `dimension` - Vector dimension (must be > 0 and <= 4096)
    ///
    /// # Errors
    /// Returns an error if dimension is invalid
    pub fn new(dimension: usize) -> Result<Self> {
        if dimension == 0 || dimension > 4096 {
            return Err(Error::InvalidId(format!(
                "Invalid vector dimension: {}",
                dimension
            )));
        }

        // Create HNSW index with cosine distance
        // Parameters: max_nb_connection, max_elements, max_layer, ef_construction, distance_function
        let hnsw = Hnsw::new(16, 10000, 16, 200, DistCosine);

        Ok(Self {
            hnsw: Arc::new(RwLock::new(hnsw)),
            node_to_index: Arc::new(RwLock::new(HashMap::new())),
            index_to_node: Arc::new(RwLock::new(HashMap::new())),
            dimension,
            stats: Arc::new(RwLock::new(KnnIndexStats {
                total_vectors: 0,
                dimension,
                avg_search_time_us: 0.0,
            })),
            next_index: Arc::new(RwLock::new(0)),
        })
    }

    /// Create a new KNN index with default parameters
    pub fn new_default(dimension: usize) -> Result<Self> {
        Self::new(dimension)
    }

    /// Add a vector for a node
    pub fn add_vector(&self, node_id: u64, embedding: Vec<f32>) -> Result<()> {
        if embedding.len() != self.dimension {
            return Err(Error::InvalidId(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.dimension,
                embedding.len()
            )));
        }

        let hnsw = self.hnsw.write();
        let mut node_to_index = self.node_to_index.write();
        let mut index_to_node = self.index_to_node.write();
        let mut next_index = self.next_index.write();

        // Check if node already exists
        if let Some(&_existing_index) = node_to_index.get(&node_id) {
            // Update existing vector - HNSW doesn't support updates, so we'll just add new
            // In a production system, you might want to implement a more sophisticated update mechanism
        }

        // Add new vector to HNSW using insert method
        let vector_index = *next_index;
        hnsw.insert((&embedding, vector_index));

        // Update mappings
        node_to_index.insert(node_id, vector_index);
        index_to_node.insert(vector_index, node_id);
        *next_index += 1;

        // Update statistics
        let mut stats = self.stats.write();
        stats.total_vectors += 1;

        Ok(())
    }

    /// Remove a vector for a node
    pub fn remove_vector(&self, node_id: u64) -> Result<()> {
        let mut node_to_index = self.node_to_index.write();
        let mut index_to_node = self.index_to_node.write();

        if let Some(&vector_index) = node_to_index.get(&node_id) {
            // Remove from mappings
            node_to_index.remove(&node_id);
            index_to_node.remove(&vector_index);

            // Update statistics
            let mut stats = self.stats.write();
            stats.total_vectors = stats.total_vectors.saturating_sub(1);
        }

        Ok(())
    }

    /// Search for k nearest neighbors using cosine similarity
    pub fn search_knn(&self, query: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        if query.len() != self.dimension {
            return Err(Error::InvalidId(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let start_time = std::time::Instant::now();

        let hnsw = self.hnsw.read();
        let index_to_node = self.index_to_node.read();

        // Search using HNSW - using search method with ef parameter
        let search_results = hnsw.search(query, k, 50);

        let mut results = Vec::new();
        for neighbour in search_results {
            if let Some(&node_id) = index_to_node.get(&neighbour.d_id) {
                // Convert distance to similarity (1 - distance for cosine)
                let similarity = 1.0 - neighbour.distance;
                results.push((node_id, similarity));
            }
        }

        // Update search time statistics
        let search_time_us = start_time.elapsed().as_micros() as f64;
        let mut stats = self.stats.write();
        stats.avg_search_time_us = (stats.avg_search_time_us + search_time_us) / 2.0;

        Ok(results)
    }

    /// Search for k nearest neighbors with default k=10
    pub fn search_knn_default(&self, query: &[f32]) -> Result<Vec<(u64, f32)>> {
        self.search_knn(query, 10)
    }

    /// Get statistics
    pub fn get_stats(&self) -> KnnIndexStats {
        self.stats.read().clone()
    }

    /// Get vector dimension
    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Check if a node has a vector
    pub fn has_vector(&self, node_id: u64) -> bool {
        let node_to_index = self.node_to_index.read();
        node_to_index.contains_key(&node_id)
    }

    /// Get all node IDs with vectors
    pub fn get_all_nodes(&self) -> Vec<u64> {
        let node_to_index = self.node_to_index.read();
        node_to_index.keys().copied().collect()
    }

    /// Clear all data
    pub fn clear(&mut self) -> Result<()> {
        let mut hnsw = self.hnsw.write();
        let mut node_to_index = self.node_to_index.write();
        let mut index_to_node = self.index_to_node.write();
        let mut next_index = self.next_index.write();

        // Create new HNSW index
        *hnsw = Hnsw::new(16, 10000, 16, 200, DistCosine);

        // Clear mappings
        node_to_index.clear();
        index_to_node.clear();
        *next_index = 0;

        // Reset statistics
        let mut stats = self.stats.write();
        stats.total_vectors = 0;

        Ok(())
    }

    /// Normalize a vector to unit length
    pub fn normalize_vector(&self, vector: &mut [f32]) {
        let norm: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in vector.iter_mut() {
                *x /= norm;
            }
        }
    }
}

impl Default for KnnIndex {
    fn default() -> Self {
        Self::new(128).expect("Failed to create default KNN index")
    }
}

/// Property B-tree index for range queries and unique constraints
///
/// Maps (label_id, key_id, value) → set of node_ids for fast property-based queries.
/// Uses BTreeMap for efficient range queries and ordered iteration.
pub struct PropertyIndex {
    /// Mapping from (label_id, key_id) to value → set of node_ids
    property_trees: Arc<RwLock<HashMap<(u32, u32), PropertyIndexTree>>>,
    /// Statistics
    stats: Arc<RwLock<PropertyIndexStats>>,
}

/// Property value for indexing
#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    /// String property value
    String(String),
    /// Integer property value
    Integer(i64),
    /// Floating point property value
    Float(f64),
    /// Boolean property value
    Boolean(bool),
    /// Null property value
    Null,
}

impl Eq for PropertyValue {}

impl PartialOrd for PropertyValue {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropertyValue {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (PropertyValue::String(a), PropertyValue::String(b)) => a.cmp(b),
            (PropertyValue::Integer(a), PropertyValue::Integer(b)) => a.cmp(b),
            (PropertyValue::Float(a), PropertyValue::Float(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
            (PropertyValue::Boolean(a), PropertyValue::Boolean(b)) => a.cmp(b),
            (PropertyValue::Null, PropertyValue::Null) => std::cmp::Ordering::Equal,
            // Different types are ordered by variant order
            (PropertyValue::String(_), _) => std::cmp::Ordering::Less,
            (PropertyValue::Integer(_), PropertyValue::String(_)) => std::cmp::Ordering::Greater,
            (PropertyValue::Integer(_), _) => std::cmp::Ordering::Less,
            (PropertyValue::Float(_), PropertyValue::String(_) | PropertyValue::Integer(_)) => std::cmp::Ordering::Greater,
            (PropertyValue::Float(_), _) => std::cmp::Ordering::Less,
            (PropertyValue::Boolean(_), PropertyValue::Null) => std::cmp::Ordering::Less,
            (PropertyValue::Boolean(_), _) => std::cmp::Ordering::Greater,
            (PropertyValue::Null, _) => std::cmp::Ordering::Greater,
        }
    }
}

impl std::hash::Hash for PropertyValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            PropertyValue::String(s) => s.hash(state),
            PropertyValue::Integer(i) => i.hash(state),
            PropertyValue::Float(f) => {
                // Convert f64 to bits for hashing
                f.to_bits().hash(state);
            }
            PropertyValue::Boolean(b) => b.hash(state),
            PropertyValue::Null => 0.hash(state),
        }
    }
}

/// Statistics for property index
#[derive(Debug, Clone, Default)]
pub struct PropertyIndexStats {
    /// Total number of property entries indexed
    pub total_entries: u64,
    /// Number of unique (label_id, key_id) combinations
    pub indexed_properties: u32,
    /// Average entries per property
    pub avg_entries_per_property: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
}

impl PropertyIndex {
    /// Create a new property index
    pub fn new() -> Self {
        Self {
            property_trees: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(PropertyIndexStats::default())),
        }
    }

    /// Add a property value for a node
    pub fn add_property(
        &self,
        node_id: u64,
        label_id: u32,
        key_id: u32,
        value: PropertyValue,
    ) -> Result<()> {
        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        let tree = trees
            .entry((label_id, key_id))
            .or_default();
        let bitmap = tree.entry(value).or_default();
        bitmap.insert(node_id as u32);

        stats.total_entries += 1;
        stats.indexed_properties = trees.len() as u32;
        stats.avg_entries_per_property = if stats.indexed_properties > 0 {
            stats.total_entries as f64 / stats.indexed_properties as f64
        } else {
            0.0
        };

        Ok(())
    }

    /// Remove a property value for a node
    pub fn remove_property(
        &self,
        node_id: u64,
        label_id: u32,
        key_id: u32,
        value: PropertyValue,
    ) -> Result<()> {
        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        if let Some(tree) = trees.get_mut(&(label_id, key_id)) {
            if let Some(bitmap) = tree.get_mut(&value) {
                bitmap.remove(node_id as u32);

                // Remove empty entries
                if bitmap.is_empty() {
                    tree.remove(&value);
                }

                stats.total_entries = stats.total_entries.saturating_sub(1);
            }
        }

        Ok(())
    }

    /// Find nodes with exact property value
    pub fn find_exact(
        &self,
        label_id: u32,
        key_id: u32,
        value: PropertyValue,
    ) -> Result<RoaringBitmap> {
        let trees = self.property_trees.read();

        if let Some(tree) = trees.get(&(label_id, key_id)) {
            if let Some(bitmap) = tree.get(&value) {
                return Ok(bitmap.clone());
            }
        }

        Ok(RoaringBitmap::new())
    }

    /// Find nodes with property value in range
    pub fn find_range(
        &self,
        label_id: u32,
        key_id: u32,
        min_value: Option<PropertyValue>,
        max_value: Option<PropertyValue>,
    ) -> Result<RoaringBitmap> {
        let trees = self.property_trees.read();
        let mut result = RoaringBitmap::new();

        if let Some(tree) = trees.get(&(label_id, key_id)) {
            let range = match (min_value, max_value) {
                (Some(min), Some(max)) => tree.range(min..=max),
                (Some(min), None) => tree.range(min..),
                (None, Some(max)) => tree.range(..=max),
                (None, None) => tree.range(..),
            };

            for (_, bitmap) in range {
                result |= bitmap;
            }
        }

        Ok(result)
    }

    /// Find nodes with property value greater than threshold
    pub fn find_greater_than(
        &self,
        label_id: u32,
        key_id: u32,
        threshold: PropertyValue,
    ) -> Result<RoaringBitmap> {
        self.find_range(label_id, key_id, Some(threshold), None)
    }

    /// Find nodes with property value less than threshold
    pub fn find_less_than(
        &self,
        label_id: u32,
        key_id: u32,
        threshold: PropertyValue,
    ) -> Result<RoaringBitmap> {
        self.find_range(label_id, key_id, None, Some(threshold))
    }

    /// Check if a property value exists (for unique constraints)
    pub fn has_value(&self, label_id: u32, key_id: u32, value: PropertyValue) -> Result<bool> {
        let trees = self.property_trees.read();

        if let Some(tree) = trees.get(&(label_id, key_id)) {
            Ok(tree.contains_key(&value))
        } else {
            Ok(false)
        }
    }

    /// Get all unique values for a property
    pub fn get_unique_values(&self, label_id: u32, key_id: u32) -> Result<Vec<PropertyValue>> {
        let trees = self.property_trees.read();

        if let Some(tree) = trees.get(&(label_id, key_id)) {
            Ok(tree.keys().cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }

    /// Get statistics
    pub fn get_stats(&self) -> PropertyIndexStats {
        self.stats.read().clone()
    }

    /// Clear all data
    pub fn clear(&self) -> Result<()> {
        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        trees.clear();
        *stats = PropertyIndexStats::default();

        Ok(())
    }

    /// Get memory usage estimate
    pub fn estimate_memory_usage(&self) -> u64 {
        let trees = self.property_trees.read();
        let mut total_size = 0;

        for tree in trees.values() {
            for bitmap in tree.values() {
                total_size += bitmap.serialized_size() as u64;
            }
        }

        total_size
    }
}

impl Default for PropertyIndex {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_index_creation() {
        let index = LabelIndex::new();
        let stats = index.get_stats();
        assert_eq!(stats.total_nodes, 0);
        assert_eq!(stats.label_count, 0);
    }

    #[test]
    fn test_label_index_add_node() {
        let index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();
        index.add_node(3, &[1, 2]).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_nodes, 3);
        assert_eq!(stats.label_count, 3);

        let nodes_with_label_0 = index.get_nodes(0).unwrap();
        assert_eq!(nodes_with_label_0.len(), 2);
        assert!(nodes_with_label_0.contains(1));
        assert!(nodes_with_label_0.contains(2));
    }

    #[test]
    fn test_label_index_intersection() {
        let index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();
        index.add_node(3, &[1, 2]).unwrap();

        let nodes_with_both = index.get_nodes_with_labels(&[0, 1]).unwrap();
        assert_eq!(nodes_with_both.len(), 1);
        assert!(nodes_with_both.contains(1));
    }

    #[test]
    fn test_label_index_union() {
        let index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();
        index.add_node(3, &[1, 2]).unwrap();

        let nodes_with_any = index.get_nodes_with_any_labels(&[0, 1]).unwrap();
        assert_eq!(nodes_with_any.len(), 3);
    }

    #[test]
    fn test_label_index_remove_node() {
        let index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();

        index.remove_node(1).unwrap();

        let nodes_with_label_0 = index.get_nodes(0).unwrap();
        assert_eq!(nodes_with_label_0.len(), 1);
        assert!(nodes_with_label_0.contains(2));
    }

    #[test]
    fn test_knn_index_creation() {
        let index = KnnIndex::new(128).unwrap();
        assert_eq!(index.dimension(), 128);

        let stats = index.get_stats();
        assert_eq!(stats.total_vectors, 0);
        assert_eq!(stats.dimension, 128);
    }

    #[test]
    fn test_knn_index_add_vector() {
        let index = KnnIndex::new(3).unwrap();

        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];

        index.add_vector(1, embedding1).unwrap();
        index.add_vector(2, embedding2).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_vectors, 2);
    }

    #[test]
    fn test_knn_index_search() {
        let index = KnnIndex::new(3).unwrap();

        let embedding1 = vec![1.0, 0.0, 0.0];
        let embedding2 = vec![0.0, 1.0, 0.0];
        let embedding3 = vec![0.0, 0.0, 1.0];

        index.add_vector(1, embedding1).unwrap();
        index.add_vector(2, embedding2).unwrap();
        index.add_vector(3, embedding3).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search_knn(&query, 2).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1); // Most similar to query
        assert!(results[0].1 > 0.9); // High similarity
    }

    #[test]
    fn test_knn_index_dimension_mismatch() {
        let index = KnnIndex::new(3).unwrap();

        let wrong_dimension = vec![1.0, 0.0];
        let result = index.add_vector(1, wrong_dimension);
        assert!(result.is_err());
    }

    #[test]
    fn test_knn_index_remove_vector() {
        let index = KnnIndex::new(3).unwrap();

        let embedding = vec![1.0, 0.0, 0.0];
        index.add_vector(1, embedding).unwrap();

        assert!(index.has_vector(1));

        index.remove_vector(1).unwrap();

        assert!(!index.has_vector(1));

        let stats = index.get_stats();
        assert_eq!(stats.total_vectors, 0);
    }

    #[test]
    fn test_cosine_similarity() {
        let _index = KnnIndex::new(3).unwrap();

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        // Helper function to calculate cosine similarity
        fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
            let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

            if norm_a == 0.0 || norm_b == 0.0 {
                0.0
            } else {
                dot_product / (norm_a * norm_b)
            }
        }

        // Same vectors should have similarity 1.0
        let sim_ab = cosine_similarity(&a, &b);
        assert!((sim_ab - 1.0).abs() < 1e-6);

        // Orthogonal vectors should have similarity 0.0
        let sim_ac = cosine_similarity(&a, &c);
        assert!((sim_ac - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_normalize_vector() {
        let index = KnnIndex::new(3).unwrap();

        let mut vector = vec![3.0, 4.0, 0.0];
        index.normalize_vector(&mut vector);

        let norm: f32 = vector.iter().map(|&x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_knn_index_clear() {
        let mut index = KnnIndex::new(3).unwrap();

        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(2, vec![0.0, 1.0, 0.0]).unwrap();

        assert_eq!(index.get_stats().total_vectors, 2);

        index.clear().unwrap();

        assert_eq!(index.get_stats().total_vectors, 0);
        assert!(index.get_all_nodes().is_empty());
    }

    #[test]
    fn test_label_index_clear() {
        let mut index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();

        assert_eq!(index.get_stats().total_nodes, 2);

        index.clear().unwrap();

        assert_eq!(index.get_stats().total_nodes, 0);
        assert_eq!(index.get_stats().label_count, 0);
    }
}
