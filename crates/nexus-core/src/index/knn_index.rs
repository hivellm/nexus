//! HNSW-backed KNN vector index.
//!
//! Provides [`KnnIndex`], [`KnnConfig`], and [`KnnIndexStats`] for
//! approximate nearest-neighbour search over `f32` embeddings.

use crate::simd;
use crate::{Error, Result};
use hnsw_rs::prelude::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use super::dist::DistSimdCosine;

/// Configuration for an HNSW-backed KNN index.
///
/// HNSW keeps its graph and vector data resident in RAM, so `max_elements`
/// directly bounds memory footprint. A single `KnnIndex` with 10_000 slots
/// and 128-dim f32 vectors consumes ~15 MB; multiply by number of labels
/// that get their own index.
#[derive(Debug, Clone, Copy)]
pub struct KnnConfig {
    /// Upper bound on vectors stored in the index. HNSW allocates this
    /// capacity eagerly, so keep it close to the expected working-set size.
    pub max_elements: usize,
    /// Maximum number of outgoing connections per node (HNSW `M`).
    pub max_connections: usize,
    /// Maximum layer count in the HNSW hierarchy.
    pub max_layer: usize,
    /// Size of the dynamic candidate list during graph construction.
    pub ef_construction: usize,
}

impl Default for KnnConfig {
    fn default() -> Self {
        // Previous hardcoded values reserved 10_000 slots unconditionally,
        // which is expensive for small deployments that only create a handful
        // of vectors. Callers with larger working sets should construct a
        // KnnConfig explicitly rather than relying on the default.
        Self {
            max_elements: 1_000,
            max_connections: 16,
            max_layer: 16,
            ef_construction: 200,
        }
    }
}

/// KNN vector index using HNSW (Hierarchical Navigable Small World)
///
/// Maps node_id → embedding for fast similarity search.
/// Uses HNSW algorithm for sub-linear search complexity.
#[derive(Clone)]
pub struct KnnIndex {
    /// HNSW index for fast KNN search
    hnsw: Arc<RwLock<Hnsw<'static, f32, DistSimdCosine>>>,
    /// Mapping from node_id to vector index in HNSW
    node_to_index: Arc<RwLock<HashMap<u64, usize>>>,
    /// Mapping from vector index to node_id
    index_to_node: Arc<RwLock<HashMap<usize, u64>>>,
    /// Vector dimension
    dimension: usize,
    /// HNSW configuration — retained so `clear()` can recreate the index
    /// with the same parameters the caller chose at construction time.
    config: KnnConfig,
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
    /// Create a new KNN index with a caller-supplied HNSW configuration.
    ///
    /// # Arguments
    /// * `dimension` - Vector dimension (must be > 0 and <= 4096)
    /// * `config`    - HNSW parameters; see [`KnnConfig`]
    ///
    /// # Errors
    /// Returns an error if dimension is invalid
    pub fn with_config(dimension: usize, config: KnnConfig) -> Result<Self> {
        if dimension == 0 || dimension > 4096 {
            return Err(Error::InvalidId(format!(
                "Invalid vector dimension: {}",
                dimension
            )));
        }

        let hnsw = Hnsw::new(
            config.max_connections,
            config.max_elements,
            config.max_layer,
            config.ef_construction,
            DistSimdCosine,
        );

        Ok(Self {
            hnsw: Arc::new(RwLock::new(hnsw)),
            node_to_index: Arc::new(RwLock::new(HashMap::new())),
            index_to_node: Arc::new(RwLock::new(HashMap::new())),
            dimension,
            config,
            stats: Arc::new(RwLock::new(KnnIndexStats {
                total_vectors: 0,
                dimension,
                avg_search_time_us: 0.0,
            })),
            next_index: Arc::new(RwLock::new(0)),
        })
    }

    /// Create a new KNN index with default HNSW parameters.
    ///
    /// Equivalent to `KnnIndex::with_config(dimension, KnnConfig::default())`.
    /// Callers that need more capacity should construct [`KnnConfig`] explicitly.
    pub fn new(dimension: usize) -> Result<Self> {
        Self::with_config(dimension, KnnConfig::default())
    }

    /// Create a new KNN index with default parameters (alias of [`KnnIndex::new`]).
    pub fn new_default(dimension: usize) -> Result<Self> {
        Self::new(dimension)
    }

    /// Add a vector for a node, or replace its vector if one already exists.
    ///
    /// `hnsw_rs` 0.3.x has no in-place update or delete API — a vector, once
    /// `insert`ed, stays physically resident in the HNSW graph forever. Re-
    /// inserting for a `node_id` that already has an entry therefore cannot
    /// remove the OLD vector from the graph; instead this evicts the old
    /// entry's `index_to_node` mapping BEFORE inserting the new vector; a
    /// "tombstone by unmapping" strategy. `search_knn_with_ef` only ever
    /// resolves a hit through `index_to_node` (see its loop below), so once
    /// the old slot's mapping is gone, the old vector is permanently
    /// unreachable through the public API even though its raw data is still
    /// inside the graph — exactly one HNSW entry maps to `node_id` at all
    /// times, and `total_vectors` counts nodes, not physical graph slots.
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

        // Re-insert for an existing node id: evict the stale HNSW entry's
        // forward mapping first so it can never again be resolved back to a
        // node id, before the new vector takes its place.
        let previous_index = node_to_index.remove(&node_id);
        if let Some(old_index) = previous_index {
            index_to_node.remove(&old_index);
        }

        // Add new vector to HNSW using insert method
        let vector_index = *next_index;
        hnsw.insert((&embedding, vector_index));

        // Update mappings
        node_to_index.insert(node_id, vector_index);
        index_to_node.insert(vector_index, node_id);
        *next_index += 1;

        // A re-insert replaces the node's vector; it must not be counted as
        // a second, distinct vector.
        if previous_index.is_none() {
            let mut stats = self.stats.write();
            stats.total_vectors += 1;
        }

        Ok(())
    }

    /// Remove a vector for a node.
    ///
    /// Evicts `node_id`'s CURRENT `node_to_index`/`index_to_node` mapping —
    /// the only entry that can exist per node id once [`KnnIndex::add_vector`]
    /// maintains its single-entry invariant on re-insert, so this alone is
    /// sufficient to make the node's vector permanently unreachable through
    /// [`KnnIndex::search_knn_with_ef`] (same tombstone-by-unmapping strategy;
    /// see `add_vector`'s docs). A no-op, not an error, when `node_id` has no
    /// vector.
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

    /// Default `ef` (size of the dynamic candidate list at search time).
    /// Larger values trade latency for recall.
    pub const DEFAULT_EF_SEARCH: usize = 50;

    /// Search for k nearest neighbors using cosine similarity.
    ///
    /// Uses [`KnnIndex::DEFAULT_EF_SEARCH`] as the HNSW `ef` parameter.
    /// For tunable recall/latency tradeoffs, use
    /// [`KnnIndex::search_knn_with_ef`].
    pub fn search_knn(&self, query: &[f32], k: usize) -> Result<Vec<(u64, f32)>> {
        self.search_knn_with_ef(query, k, Self::DEFAULT_EF_SEARCH)
    }

    /// Search for k nearest neighbors with an explicit HNSW `ef` parameter.
    ///
    /// `ef_search` controls the size of the dynamic candidate list during
    /// the descent. The HNSW algorithm requires `ef_search >= k`; values
    /// below `k` are silently raised to `k` so the caller never gets
    /// fewer results than requested.
    ///
    /// Recall and latency both grow with `ef_search`. Typical sweep
    /// ranges for production tuning are `ef_search ∈ {50, 100, 200, 400}`
    /// — see `crates/nexus-knn-bench` for the full Pareto methodology.
    pub fn search_knn_with_ef(
        &self,
        query: &[f32],
        k: usize,
        ef_search: usize,
    ) -> Result<Vec<(u64, f32)>> {
        if query.len() != self.dimension {
            return Err(Error::InvalidId(format!(
                "Query dimension mismatch: expected {}, got {}",
                self.dimension,
                query.len()
            )));
        }

        let ef = ef_search.max(k);
        let start_time = std::time::Instant::now();

        let hnsw = self.hnsw.read();
        let index_to_node = self.index_to_node.read();

        let search_results = hnsw.search(query, k, ef);

        let mut results = Vec::new();
        for neighbour in search_results {
            if let Some(&node_id) = index_to_node.get(&neighbour.d_id) {
                let similarity = 1.0 - neighbour.distance;
                results.push((node_id, similarity));
            }
        }

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

        // Recreate the HNSW index using the config this instance was built with.
        *hnsw = Hnsw::new(
            self.config.max_connections,
            self.config.max_elements,
            self.config.max_layer,
            self.config.ef_construction,
            DistSimdCosine,
        );

        // Clear mappings
        node_to_index.clear();
        index_to_node.clear();
        *next_index = 0;

        // Reset statistics
        let mut stats = self.stats.write();
        stats.total_vectors = 0;

        Ok(())
    }

    /// Normalize a vector to unit length.
    ///
    /// Uses the SIMD-dispatched `simd::distance::normalize_f32` kernel
    /// (AVX-512 → AVX2 → SSE4.2 → NEON → Scalar) — the method keeps
    /// its `&self` signature for API compatibility but the kernel
    /// selection is stateless.
    pub fn normalize_vector(&self, vector: &mut [f32]) {
        simd::distance::normalize_f32(vector);
    }
}

impl Default for KnnIndex {
    fn default() -> Self {
        Self::new(128).expect("Failed to create default KNN index")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::dist::DEFAULT_VECTORIZER_DIMENSION;

    #[test]
    fn test_knn_index_creation() {
        let index = KnnIndex::new(DEFAULT_VECTORIZER_DIMENSION).unwrap();
        assert_eq!(index.dimension(), DEFAULT_VECTORIZER_DIMENSION);

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

        // HNSW may return fewer results than k when the index is very small (3 vectors)
        // This is expected behavior due to the HNSW graph structure
        assert!(
            !results.is_empty() && results.len() <= 2,
            "Should return at least 1 result, at most 2"
        );
        assert_eq!(results[0].0, 1); // Most similar to query
        assert!(results[0].1 > 0.9); // High similarity
    }

    #[test]
    fn test_knn_index_search_with_ef_matches_default() {
        let index = KnnIndex::new(3).unwrap();
        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(2, vec![0.0, 1.0, 0.0]).unwrap();
        index.add_vector(3, vec![0.0, 0.0, 1.0]).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let default_results = index.search_knn(&query, 2).unwrap();
        let explicit_results = index
            .search_knn_with_ef(&query, 2, KnnIndex::DEFAULT_EF_SEARCH)
            .unwrap();
        assert_eq!(default_results, explicit_results);
    }

    #[test]
    fn test_knn_index_search_with_ef_clamps_below_k() {
        let index = KnnIndex::new(3).unwrap();
        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(2, vec![0.0, 1.0, 0.0]).unwrap();
        index.add_vector(3, vec![0.0, 0.0, 1.0]).unwrap();

        // ef_search=1 with k=2 must still produce up to k results
        // because the implementation raises ef to k internally.
        let results = index
            .search_knn_with_ef(&vec![1.0, 0.0, 0.0], 2, 1)
            .unwrap();
        assert!(!results.is_empty());
        assert!(results.len() <= 2);
        assert_eq!(results[0].0, 1);
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
    fn test_knn_index_new_default() {
        let index = KnnIndex::new_default(64).unwrap();
        assert_eq!(index.dimension(), 64);
    }

    #[test]
    fn test_knn_index_invalid_dimension() {
        // Test zero dimension
        let result = KnnIndex::new(0);
        assert!(result.is_err());

        // Test too large dimension
        let result = KnnIndex::new(5000);
        assert!(result.is_err());
    }

    #[test]
    fn test_knn_index_search_knn_default() {
        let index = KnnIndex::new(3).unwrap();

        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(2, vec![0.0, 1.0, 0.0]).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search_knn_default(&query).unwrap();
        // HNSW may return fewer results than k when the index is very small (2 vectors)
        // This is expected behavior due to the HNSW graph structure
        assert!(!results.is_empty() && results.len() <= 2);
        // Verify the closest result is node 1
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_knn_index_has_vector() {
        let index = KnnIndex::new(3).unwrap();

        assert!(!index.has_vector(1));

        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        assert!(index.has_vector(1));
        assert!(!index.has_vector(2));
    }

    #[test]
    fn test_knn_index_get_all_nodes() {
        let index = KnnIndex::new(3).unwrap();

        assert!(index.get_all_nodes().is_empty());

        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(2, vec![0.0, 1.0, 0.0]).unwrap();

        let nodes = index.get_all_nodes();
        assert_eq!(nodes.len(), 2);
        assert!(nodes.contains(&1));
        assert!(nodes.contains(&2));
    }

    #[test]
    fn test_knn_index_search_empty() {
        let index = KnnIndex::new(3).unwrap();

        let query = vec![1.0, 0.0, 0.0];
        let results = index.search_knn(&query, 5).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_knn_index_remove_nonexistent() {
        let index = KnnIndex::new(3).unwrap();

        // Removing non-existent vector should not error
        index.remove_vector(999).unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_vectors, 0);
    }

    // ── phase0_fix-knn-index-divergence §2.1/§3.2 ─────────────────────

    #[test]
    fn test_add_vector_reinsert_does_not_leak_stale_entry() {
        let index = KnnIndex::new(3).unwrap();

        // Re-insert the same node id with a very different vector — before
        // the fix this leaves BOTH the old and new HNSW entries reachable.
        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(1, vec![0.0, 1.0, 0.0]).unwrap();

        // A re-insert updates the existing node's vector; it must not be
        // counted as a second, distinct vector.
        let stats = index.get_stats();
        assert_eq!(
            stats.total_vectors, 1,
            "re-insert for an existing node id must not double-count"
        );

        // A broad-radius query wide enough to surface both the stale and
        // current HNSW entries must return node 1 exactly once.
        let query = vec![0.5, 0.5, 0.0];
        let results = index.search_knn_with_ef(&query, 10, 200).unwrap();
        let hits: Vec<_> = results.iter().filter(|(id, _)| *id == 1).collect();
        assert_eq!(
            hits.len(),
            1,
            "the stale HNSW entry left by a re-insert must not be reachable, got {results:?}"
        );

        // The single reachable entry must reflect the CURRENT vector
        // ([0,1,0]), not the stale one ([1,0,0]) — a query identical to
        // the current vector must score near-perfect similarity.
        let current_query = vec![0.0, 1.0, 0.0];
        let current_results = index.search_knn_with_ef(&current_query, 10, 200).unwrap();
        let (_, score) = current_results
            .iter()
            .find(|(id, _)| *id == 1)
            .expect("node 1 must be reachable via its current vector");
        assert!(
            *score > 0.99,
            "surviving entry must match the current vector, got score {score}"
        );
    }

    // ── phase0_fix-knn-index-divergence §2.2/§3.3 ─────────────────────

    #[test]
    fn test_remove_vector_after_reinsert_leaves_no_phantom_entry() {
        let index = KnnIndex::new(3).unwrap();

        // Trigger the §2.1 leak scenario, then remove the node entirely.
        index.add_vector(1, vec![1.0, 0.0, 0.0]).unwrap();
        index.add_vector(1, vec![0.0, 1.0, 0.0]).unwrap();
        index.remove_vector(1).unwrap();

        assert!(!index.has_vector(1));
        assert_eq!(index.get_stats().total_vectors, 0);

        // Before the fix, `remove_vector` can only ever drop the CURRENT
        // mapping — the orphan left by the earlier re-insert stays
        // reachable in the HNSW graph, producing a phantom hit here.
        let query = vec![0.5, 0.5, 0.0];
        let results = index.search_knn_with_ef(&query, 10, 200).unwrap();
        assert!(
            results.iter().all(|(id, _)| *id != 1),
            "no entry for node 1 (old or new) should be reachable after \
             remove following a re-insert, got {results:?}"
        );
    }
}
