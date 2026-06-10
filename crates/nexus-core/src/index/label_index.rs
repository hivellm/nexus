//! Label bitmap index using roaring bitmaps.
//!
//! Maps `label_id` → bitmap of `node_id`s for fast label-based queries.

use crate::{Error, Result};
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Label bitmap index using roaring bitmaps
///
/// Maps label_id → bitmap of node_ids for fast label-based queries.
/// Uses RoaringBitmap for efficient compression and operations.
#[derive(Clone)]
pub struct LabelIndex {
    /// Mapping from label_id to bitmap of node_ids
    label_bitmaps: Arc<RwLock<HashMap<u32, RoaringBitmap>>>,
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
    fn recompute_stats(bitmaps: &HashMap<u32, RoaringBitmap>) -> LabelIndexStats {
        let mut unique_nodes: HashSet<u32> = HashSet::new();
        let mut total_entries: u64 = 0;

        for bitmap in bitmaps.values() {
            total_entries += bitmap.len();
            unique_nodes.extend(bitmap.iter());
        }

        let label_count = bitmaps.len() as u32;
        let avg_nodes_per_label = if label_count > 0 {
            total_entries as f64 / label_count as f64
        } else {
            0.0
        };

        LabelIndexStats {
            total_nodes: unique_nodes.len() as u64,
            label_count,
            avg_nodes_per_label,
        }
    }

    /// Create a new label index
    pub fn new() -> Self {
        Self {
            label_bitmaps: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a node with given labels
    pub fn add_node(&self, node_id: u64, label_ids: &[u32]) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();

        for &label_id in label_ids {
            bitmaps.entry(label_id).or_default().insert(node_id as u32);
        }

        Ok(())
    }

    /// Remove a node from all labels
    pub fn remove_node(&self, node_id: u64) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();

        for bitmap in bitmaps.values_mut() {
            bitmap.remove(node_id as u32);
        }

        bitmaps.retain(|_, bitmap| !bitmap.is_empty());
        Ok(())
    }

    /// Replace the labels associated with a node
    pub fn set_node_labels(&self, node_id: u64, label_ids: &[u32]) -> Result<()> {
        let mut bitmaps = self.label_bitmaps.write();

        for bitmap in bitmaps.values_mut() {
            bitmap.remove(node_id as u32);
        }

        for &label_id in label_ids {
            bitmaps.entry(label_id).or_default().insert(node_id as u32);
        }

        bitmaps.retain(|_, bitmap| !bitmap.is_empty());
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

    /// Get statistics. Computed on demand from the label bitmaps — the
    /// previous design recomputed (O(N) over every node in every bitmap) on
    /// every `add_node`/`remove_node`, which pinned CPU under sustained write
    /// load (issue #12). Stats are diagnostic-only, so the cost moves to the
    /// (rare) read instead of the (hot) write.
    pub fn get_stats(&self) -> LabelIndexStats {
        let bitmaps = self.label_bitmaps.read();
        Self::recompute_stats(&bitmaps)
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

        Ok(())
    }

    /// Health check for the label index
    pub fn health_check(&self) -> Result<()> {
        let bitmaps = self.label_bitmaps.read();

        // Check if the number of labels is reasonable
        if bitmaps.len() > 1_000_000 {
            // 1 million max
            return Err(Error::index("Too many labels"));
        }

        // Check if the total nodes count is reasonable
        let total_nodes = Self::recompute_stats(&bitmaps).total_nodes;
        if total_nodes > 1_000_000_000 {
            // 1 billion max
            return Err(Error::index("Too many nodes in label index"));
        }

        Ok(())
    }
}

impl Default for LabelIndex {
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
    fn test_label_index_clear() {
        let mut index = LabelIndex::new();

        index.add_node(1, &[0, 1]).unwrap();
        index.add_node(2, &[0]).unwrap();

        assert_eq!(index.get_stats().total_nodes, 2);

        index.clear().unwrap();

        assert_eq!(index.get_stats().total_nodes, 0);
        assert_eq!(index.get_stats().label_count, 0);
    }

    #[test]
    fn test_label_index_estimate_cardinality() {
        let index = LabelIndex::new();

        // Empty index
        assert_eq!(index.estimate_cardinality(0), 0);

        // Add some nodes
        index.add_node(1, &[0]).unwrap();
        index.add_node(2, &[0]).unwrap();
        index.add_node(3, &[1]).unwrap();

        assert_eq!(index.estimate_cardinality(0), 2);
        assert_eq!(index.estimate_cardinality(1), 1);
        assert_eq!(index.estimate_cardinality(2), 0);
    }

    #[test]
    fn test_label_index_has_label() {
        let index = LabelIndex::new();

        assert!(!index.has_label(0));

        index.add_node(1, &[0]).unwrap();
        assert!(index.has_label(0));
        assert!(!index.has_label(1));
    }

    #[test]
    fn test_label_index_get_all_labels() {
        let index = LabelIndex::new();

        assert!(index.get_all_labels().is_empty());

        index.add_node(1, &[0, 1, 2]).unwrap();
        let labels = index.get_all_labels();
        assert_eq!(labels.len(), 3);
        assert!(labels.contains(&0));
        assert!(labels.contains(&1));
        assert!(labels.contains(&2));
    }

    #[test]
    fn test_label_index_health_check() {
        let index = LabelIndex::new();

        // Empty index should pass health check
        index.health_check().unwrap();

        // Add reasonable amount of data
        for i in 0..1000 {
            index.add_node(i, &[i as u32 % 10]).unwrap();
        }
        index.health_check().unwrap();
    }

    #[test]
    fn test_label_index_empty_labels() {
        let index = LabelIndex::new();

        // Test with empty label list
        let result = index.get_nodes_with_labels(&[]).unwrap();
        assert!(result.is_empty());

        let result = index.get_nodes_with_any_labels(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_label_index_nonexistent_labels() {
        let index = LabelIndex::new();

        // Test with non-existent labels
        let result = index.get_nodes_with_labels(&[999]).unwrap();
        assert!(result.is_empty());

        let result = index.get_nodes_with_any_labels(&[999, 998]).unwrap();
        assert!(result.is_empty());
    }
}
