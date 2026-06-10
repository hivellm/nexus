//! Property B-tree index for range queries and unique constraints.
//!
//! Maps `(label_id, key_id, value)` → set of `node_id`s for fast
//! property-based queries using [`BTreeMap`] for ordered iteration.

use crate::{Error, Result};
use parking_lot::RwLock;
use roaring::RoaringBitmap;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Type alias for property index trees
type PropertyIndexTree = BTreeMap<PropertyValue, RoaringBitmap>;

/// Property value for indexing
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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
            (PropertyValue::Float(a), PropertyValue::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (PropertyValue::Boolean(a), PropertyValue::Boolean(b)) => a.cmp(b),
            (PropertyValue::Null, PropertyValue::Null) => std::cmp::Ordering::Equal,
            // Different types are ordered by variant order
            (PropertyValue::String(_), _) => std::cmp::Ordering::Less,
            (PropertyValue::Integer(_), PropertyValue::String(_)) => std::cmp::Ordering::Greater,
            (PropertyValue::Integer(_), _) => std::cmp::Ordering::Less,
            (PropertyValue::Float(_), PropertyValue::String(_) | PropertyValue::Integer(_)) => {
                std::cmp::Ordering::Greater
            }
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

/// Property B-tree index for range queries and unique constraints
///
/// Maps (label_id, key_id, value) → set of node_ids for fast property-based queries.
/// Uses BTreeMap for efficient range queries and ordered iteration.
#[derive(Clone)]
pub struct PropertyIndex {
    /// Mapping from (label_id, key_id) to value → set of node_ids
    property_trees: Arc<RwLock<HashMap<(u32, u32), PropertyIndexTree>>>,
    /// Statistics
    stats: Arc<RwLock<PropertyIndexStats>>,
}

impl PropertyIndex {
    /// Create a new property index
    pub fn new() -> Self {
        Self {
            property_trees: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(PropertyIndexStats::default())),
        }
    }

    /// True when at least one property index is registered for `label_id`
    /// (#21). Lets per-write maintenance skip the per-property
    /// `get_key_id` catalog reads entirely for nodes whose labels have no
    /// index. O(#registered indexes) HashMap key scan — registered
    /// indexes are few. The registration set is the `property_trees` map
    /// itself, kept in sync by `create_index` / `remove_index` and the
    /// startup rebuild (#11).
    pub fn has_index_for_label(&self, label_id: u32) -> bool {
        self.property_trees
            .read()
            .keys()
            .any(|&(l, _)| l == label_id)
    }

    /// Add a property value for a node
    pub fn add_property(
        &self,
        node_id: u64,
        label_id: u32,
        key_id: u32,
        value: PropertyValue,
    ) -> Result<()> {
        // Null-key contract (Neo4j-aligned): a null property value means the
        // property is absent, so it is never indexed. Skipping it keeps the
        // typed property index free of null-keyed entries — `find_exact(..,
        // Null)` therefore never matches and legacy null-valued properties
        // cannot pollute index seeks. See docs/ops/graph-rebuild.md.
        if value == PropertyValue::Null {
            return Ok(());
        }

        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        let tree = trees.entry((label_id, key_id)).or_default();
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

    /// Check if an index exists for a (label_id, key_id) combination
    pub fn has_index(&self, label_id: u32, key_id: u32) -> bool {
        let trees = self.property_trees.read();
        trees.contains_key(&(label_id, key_id))
    }

    /// True if at least one property index is registered. Cheap fast-path
    /// guard (#21): write paths that maintain the typed index can skip the
    /// per-property × per-label `has_index` loop entirely when no index exists
    /// — the common case for un-indexed graphs.
    pub fn has_any_index(&self) -> bool {
        !self.property_trees.read().is_empty()
    }

    /// Create an index for a (label_id, key_id) combination
    /// This initializes an empty index structure. The index will be populated
    /// as properties are added via add_property().
    pub fn create_index(&self, label_id: u32, key_id: u32) -> Result<()> {
        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        // Create empty index if it doesn't exist
        if trees.entry((label_id, key_id)).or_default().is_empty() {
            stats.indexed_properties = trees.len() as u32;
        }

        Ok(())
    }

    /// Drop an index for a (label_id, key_id) combination
    /// This removes all indexed data for this property.
    pub fn drop_index(&self, label_id: u32, key_id: u32) -> Result<()> {
        let mut trees = self.property_trees.write();
        let mut stats = self.stats.write();

        if let Some(tree) = trees.remove(&(label_id, key_id)) {
            // Update stats: subtract entries from this index
            let mut removed_entries = 0u64;
            for bitmap in tree.values() {
                removed_entries += bitmap.len();
            }
            stats.total_entries = stats.total_entries.saturating_sub(removed_entries);
            stats.indexed_properties = trees.len() as u32;
            stats.avg_entries_per_property = if stats.indexed_properties > 0 {
                stats.total_entries as f64 / stats.indexed_properties as f64
            } else {
                0.0
            };
        }

        Ok(())
    }

    /// Health check for the property index
    pub fn health_check(&self) -> Result<()> {
        let trees = self.property_trees.read();
        let stats = self.stats.read();

        // Check if the number of indexed properties is reasonable
        if trees.len() > 1_000_000 {
            // 1 million max
            return Err(Error::index("Too many indexed properties"));
        }

        // Check if the total entries count is reasonable
        if stats.total_entries > 1_000_000_000 {
            // 1 billion max
            return Err(Error::index("Too many entries in property index"));
        }

        Ok(())
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
    fn test_property_index_creation() {
        let index = PropertyIndex::new();
        let stats = index.get_stats();
        assert_eq!(stats.total_entries, 0);
        assert_eq!(stats.indexed_properties, 0);
    }

    #[test]
    fn test_property_index_add_property() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(1, 0, 1, PropertyValue::Integer(42))
            .unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.indexed_properties, 2);
    }

    #[test]
    fn test_property_index_remove_property() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_entries, 2);

        index
            .remove_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();

        let stats = index.get_stats();
        assert_eq!(stats.total_entries, 1);
    }

    #[test]
    fn test_property_index_find_exact() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(3, 0, 0, PropertyValue::String("other".to_string()))
            .unwrap();

        let results = index
            .find_exact(0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(1));
        assert!(results.contains(2));
        assert!(!results.contains(3));
    }

    #[test]
    fn test_property_index_find_range() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::Integer(10))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::Integer(20))
            .unwrap();
        index
            .add_property(3, 0, 0, PropertyValue::Integer(30))
            .unwrap();
        index
            .add_property(4, 0, 0, PropertyValue::Integer(40))
            .unwrap();

        // Range 15-35
        let results = index
            .find_range(
                0,
                0,
                Some(PropertyValue::Integer(15)),
                Some(PropertyValue::Integer(35)),
            )
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(2));
        assert!(results.contains(3));
    }

    #[test]
    fn test_property_index_find_greater_than() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::Integer(10))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::Integer(20))
            .unwrap();
        index
            .add_property(3, 0, 0, PropertyValue::Integer(30))
            .unwrap();

        let results = index
            .find_greater_than(0, 0, PropertyValue::Integer(15))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(2));
        assert!(results.contains(3));
    }

    #[test]
    fn test_property_index_find_less_than() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::Integer(10))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::Integer(20))
            .unwrap();
        index
            .add_property(3, 0, 0, PropertyValue::Integer(30))
            .unwrap();

        let results = index
            .find_less_than(0, 0, PropertyValue::Integer(25))
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(1));
        assert!(results.contains(2));
    }

    #[test]
    fn test_property_index_has_value() {
        let index = PropertyIndex::new();

        assert!(
            !index
                .has_value(0, 0, PropertyValue::String("test".to_string()))
                .unwrap()
        );

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        assert!(
            index
                .has_value(0, 0, PropertyValue::String("test".to_string()))
                .unwrap()
        );
        assert!(
            !index
                .has_value(0, 0, PropertyValue::String("other".to_string()))
                .unwrap()
        );
    }

    #[test]
    fn test_property_index_get_unique_values() {
        let index = PropertyIndex::new();

        assert!(index.get_unique_values(0, 0).unwrap().is_empty());

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        index
            .add_property(2, 0, 0, PropertyValue::String("other".to_string()))
            .unwrap();
        index
            .add_property(3, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();

        let values = index.get_unique_values(0, 0).unwrap();
        assert_eq!(values.len(), 2);
        assert!(values.contains(&PropertyValue::String("test".to_string())));
        assert!(values.contains(&PropertyValue::String("other".to_string())));
    }

    #[test]
    fn test_property_index_clear() {
        let index = PropertyIndex::new();

        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        assert_eq!(index.get_stats().total_entries, 1);

        index.clear().unwrap();
        assert_eq!(index.get_stats().total_entries, 0);
    }

    #[test]
    fn test_property_index_estimate_memory_usage() {
        let index = PropertyIndex::new();

        // Empty index should have minimal memory usage
        let usage = index.estimate_memory_usage();
        assert_eq!(usage, 0);

        // Add some data
        index
            .add_property(1, 0, 0, PropertyValue::String("test".to_string()))
            .unwrap();
        let usage = index.estimate_memory_usage();
        assert!(usage > 0);
    }

    #[test]
    fn test_property_index_health_check() {
        let index = PropertyIndex::new();

        // Empty index should pass health check
        index.health_check().unwrap();

        // Add reasonable amount of data
        for i in 0..1000 {
            index
                .add_property(i, i as u32 % 10, 0, PropertyValue::Integer(i as i64))
                .unwrap();
        }
        index.health_check().unwrap();
    }

    #[test]
    fn test_property_value_ordering() {
        // Test ordering of different property value types
        let values = [
            PropertyValue::String("a".to_string()),
            PropertyValue::Integer(1),
            PropertyValue::Float(1.0),
            PropertyValue::Boolean(true),
            PropertyValue::Null,
        ];

        for i in 0..values.len() {
            for j in 0..values.len() {
                if i < j {
                    assert!(values[i] < values[j]);
                } else if i > j {
                    assert!(values[i] > values[j]);
                } else {
                    assert_eq!(values[i], values[j]);
                }
            }
        }
    }

    #[test]
    fn test_property_value_hashing() {
        use std::collections::HashMap;

        let mut map = HashMap::new();

        map.insert(PropertyValue::String("test".to_string()), 1);
        map.insert(PropertyValue::Integer(42), 2);
        map.insert(PropertyValue::Boolean(true), 3);

        assert_eq!(
            map.get(&PropertyValue::String("test".to_string())),
            Some(&1)
        );
        assert_eq!(map.get(&PropertyValue::Integer(42)), Some(&2));
        assert_eq!(map.get(&PropertyValue::Boolean(true)), Some(&3));
        assert_eq!(map.get(&PropertyValue::String("other".to_string())), None);
    }

    #[test]
    fn add_property_skips_null_value() {
        let index = PropertyIndex::new();
        // label_id=1, key_id=1
        index.create_index(1, 1).unwrap();

        // Adding Null must be a no-op — find_exact returns an empty bitmap.
        index.add_property(42, 1, 1, PropertyValue::Null).unwrap();
        let null_hits = index.find_exact(1, 1, PropertyValue::Null).unwrap();
        assert!(
            null_hits.is_empty(),
            "find_exact for Null should return empty bitmap, got {null_hits:?}"
        );

        // A subsequent non-null add must still be indexed normally.
        index
            .add_property(42, 1, 1, PropertyValue::Integer(99))
            .unwrap();
        let hits = index.find_exact(1, 1, PropertyValue::Integer(99)).unwrap();
        assert!(
            hits.contains(42),
            "node 42 should appear after non-null add"
        );
    }

    #[test]
    fn has_any_index_reflects_registration() {
        // #21: the write-path fast-path guard. Empty index => no work.
        let index = PropertyIndex::new();
        assert!(!index.has_any_index(), "fresh index has no registrations");
        index.create_index(1, 1).unwrap();
        assert!(
            index.has_any_index(),
            "after create_index, at least one exists"
        );
        index.drop_index(1, 1).unwrap();
        assert!(!index.has_any_index(), "after drop_index, none remain");
    }

    #[test]
    fn has_index_for_label_reflects_registration() {
        // #21: the per-node prefilter guard — a node whose labels have no
        // registered index does zero per-property catalog work. CREATE /
        // DROP INDEX must keep the answer current.
        let index = PropertyIndex::new();
        assert!(
            !index.has_index_for_label(1),
            "fresh index: no label has one"
        );

        index.create_index(1, 7).unwrap();
        assert!(index.has_index_for_label(1), "label 1 indexed after create");
        assert!(
            !index.has_index_for_label(2),
            "other labels remain un-indexed"
        );

        index.drop_index(1, 7).unwrap();
        assert!(
            !index.has_index_for_label(1),
            "label 1 un-indexed after drop"
        );
    }
}
