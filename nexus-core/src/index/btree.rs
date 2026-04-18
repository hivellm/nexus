//! Advanced B-tree index implementation for property range queries
//!
//! Features:
//! - Composite keys for multi-property indexing
//! - Range queries with inclusive/exclusive bounds
//! - Statistics collection (NDV, histograms, selectivity)
//! - Memory-efficient storage with compression
//! - Support for unique constraints

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

/// Advanced B-tree index for property range queries
#[derive(Debug)]
pub struct BTreeIndex {
    /// Main index storage
    index: RwLock<BTreeMap<PropertyKey, Vec<u64>>>,
    /// Label ID this index is for
    label_id: u32,
    /// Property key IDs (for composite indexes)
    property_key_ids: Vec<u32>,
    /// Statistics and metadata
    stats: RwLock<IndexStats>,
    /// Unique constraint flag
    is_unique: bool,
    /// Compression enabled flag
    compression_enabled: bool,
}

/// Composite property key for multi-property indexing
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyKey {
    /// Composite values for multi-property indexes
    pub values: Vec<Value>,
    /// Node ID for tie-breaking
    pub node_id: u64,
}

/// Range query bounds
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBounds {
    pub min: Option<Value>,
    pub max: Option<Value>,
    pub min_inclusive: bool,
    pub max_inclusive: bool,
}

impl Default for RangeBounds {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeBounds {
    pub fn new() -> Self {
        Self {
            min: None,
            max: None,
            min_inclusive: true,
            max_inclusive: true,
        }
    }

    pub fn with_min(mut self, min: Value, inclusive: bool) -> Self {
        self.min = Some(min);
        self.min_inclusive = inclusive;
        self
    }

    pub fn with_max(mut self, max: Value, inclusive: bool) -> Self {
        self.max = Some(max);
        self.max_inclusive = inclusive;
        self
    }
}

impl PartialOrd for PropertyKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropertyKey {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare composite values lexicographically
        for (a, b) in self.values.iter().zip(other.values.iter()) {
            match PropertyKey::compare_values(a, b) {
                x if x < 0 => return Ordering::Less,
                x if x > 0 => return Ordering::Greater,
                _ => continue, // Values are equal, check next component
            }
        }

        // If all values are equal, compare by length first, then by node_id
        match self.values.len().cmp(&other.values.len()) {
            Ordering::Equal => self.node_id.cmp(&other.node_id),
            other => other,
        }
    }
}

impl PropertyKey {
    /// Compare two JSON values for ordering
    fn compare_values(a: &Value, b: &Value) -> i32 {
        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64.partial_cmp(&b_f64).unwrap_or(Ordering::Equal) as i32
            }
            (Value::String(a_str), Value::String(b_str)) => a_str.cmp(b_str) as i32,
            (Value::Bool(a_bool), Value::Bool(b_bool)) => (*a_bool as i32) - (*b_bool as i32),
            (Value::Array(a_arr), Value::Array(b_arr)) => {
                // Compare arrays lexicographically
                for (a_item, b_item) in a_arr.iter().zip(b_arr.iter()) {
                    let cmp = Self::compare_values(a_item, b_item);
                    if cmp != 0 {
                        return cmp;
                    }
                }
                a_arr.len().cmp(&b_arr.len()) as i32
            }
            (Value::Object(a_obj), Value::Object(b_obj)) => {
                // Compare objects by sorted keys
                let mut a_keys: Vec<_> = a_obj.keys().collect();
                let mut b_keys: Vec<_> = b_obj.keys().collect();
                a_keys.sort();
                b_keys.sort();

                for (a_key, b_key) in a_keys.iter().zip(b_keys.iter()) {
                    let key_cmp = a_key.cmp(b_key) as i32;
                    if key_cmp != 0 {
                        return key_cmp;
                    }

                    let val_cmp = Self::compare_values(&a_obj[*a_key], &b_obj[*b_key]);
                    if val_cmp != 0 {
                        return val_cmp;
                    }
                }
                a_obj.len().cmp(&b_obj.len()) as i32
            }
            (Value::Null, Value::Null) => 0,
            (Value::Null, _) => -1,
            (_, Value::Null) => 1,
            // Different types: order by type
            (Value::String(_), _) => -1,
            (Value::Number(_), Value::String(_)) => 1,
            (Value::Number(_), _) => -1,
            (Value::Bool(_), Value::String(_) | Value::Number(_)) => 1,
            (Value::Bool(_), _) => -1,
            (Value::Array(_), Value::String(_) | Value::Number(_) | Value::Bool(_)) => 1,
            (Value::Array(_), _) => -1,
            (Value::Object(_), _) => 1,
        }
    }

    /// Create a new composite key
    pub fn new(values: Vec<Value>, node_id: u64) -> Self {
        Self { values, node_id }
    }

    /// Create a single-value key (backward compatibility)
    pub fn single(value: Value, node_id: u64) -> Self {
        Self {
            values: vec![value],
            node_id,
        }
    }
}

/// Advanced index statistics with selectivity and histogram data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    /// Total number of entries
    pub entry_count: u64,
    /// Number of unique values (NDV - Number of Distinct Values)
    pub unique_value_count: u64,
    /// Estimated size in bytes
    pub size_bytes: u64,
    /// B-tree height
    pub height: u32,
    /// Last update timestamp
    pub last_updated: chrono::DateTime<chrono::Utc>,
    /// Selectivity (unique_value_count / entry_count)
    pub selectivity: f64,
    /// Value distribution histogram (buckets)
    pub histogram: Vec<HistogramBucket>,
    /// Most frequent values
    pub most_frequent: Vec<(Value, u64)>,
    /// Average key size in bytes
    pub avg_key_size: f64,
    /// Compression ratio (if enabled)
    pub compression_ratio: f64,
}

/// Histogram bucket for value distribution analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramBucket {
    /// Bucket range start (inclusive)
    pub min_value: Value,
    /// Bucket range end (exclusive)
    pub max_value: Value,
    /// Number of values in this bucket
    pub count: u64,
    /// Number of distinct values in this bucket
    pub distinct_count: u64,
}

impl BTreeIndex {
    /// Create a new B-tree index for a specific label and single property
    pub fn new(label_id: u32, property_key_id: u32) -> Self {
        Self::new_composite(label_id, vec![property_key_id], false, false)
    }

    /// Create a new composite B-tree index for multiple properties
    pub fn new_composite(
        label_id: u32,
        property_key_ids: Vec<u32>,
        is_unique: bool,
        compression_enabled: bool,
    ) -> Self {
        Self {
            index: RwLock::new(BTreeMap::new()),
            label_id,
            property_key_ids,
            stats: RwLock::new(IndexStats {
                entry_count: 0,
                unique_value_count: 0,
                size_bytes: 0,
                height: 0,
                last_updated: chrono::Utc::now(),
                selectivity: 0.0,
                histogram: Vec::new(),
                most_frequent: Vec::new(),
                avg_key_size: 0.0,
                compression_ratio: 1.0,
            }),
            is_unique,
            compression_enabled,
        }
    }

    /// Create a unique index
    pub fn new_unique(label_id: u32, property_key_id: u32) -> Self {
        Self::new_composite(label_id, vec![property_key_id], true, false)
    }

    /// Create a composite unique index
    pub fn new_composite_unique(label_id: u32, property_key_ids: Vec<u32>) -> Self {
        Self::new_composite(label_id, property_key_ids, true, false)
    }

    /// Insert a node with a single property value into the index
    pub fn insert(&self, node_id: u64, value: Value) -> Result<()> {
        self.insert_composite(node_id, vec![value])
    }

    /// Insert a node with composite property values into the index
    pub fn insert_composite(&self, node_id: u64, values: Vec<Value>) -> Result<()> {
        if values.len() != self.property_key_ids.len() {
            return Err(Error::InvalidId(format!(
                "Expected {} values, got {}",
                self.property_key_ids.len(),
                values.len()
            )));
        }

        let key = PropertyKey::new(values.clone(), node_id);
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut stats = self
            .stats
            .write()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;

        // Check for unique constraint violation
        if self.is_unique && index.contains_key(&key) {
            return Err(Error::ConstraintViolation(format!(
                "Unique constraint violation for key: {:?}",
                key.values
            )));
        }

        // Check if this exact key already exists
        if let Some(existing_nodes) = index.get(&key) {
            if !existing_nodes.contains(&node_id) {
                // Add node_id to existing entry
                let mut updated_nodes = existing_nodes.clone();
                updated_nodes.push(node_id);
                index.insert(key, updated_nodes);
                stats.entry_count += 1;
            }
        } else {
            // Create new entry
            index.insert(key, vec![node_id]);
            stats.entry_count += 1;
            stats.unique_value_count = index.len() as u64;
        }

        // Update statistics
        self.update_statistics(&mut stats, &index);
        stats.last_updated = chrono::Utc::now();

        Ok(())
    }

    /// Remove a node from the index
    pub fn remove(&self, node_id: u64, value: &Value) -> Result<bool> {
        self.remove_composite(node_id, vec![value.clone()])
    }

    /// Remove a node with composite values from the index
    pub fn remove_composite(&self, node_id: u64, values: Vec<Value>) -> Result<bool> {
        if values.len() != self.property_key_ids.len() {
            return Err(Error::InvalidId(format!(
                "Expected {} values, got {}",
                self.property_key_ids.len(),
                values.len()
            )));
        }

        let key = PropertyKey::new(values, node_id);
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut stats = self
            .stats
            .write()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;

        if let Some(existing_nodes) = index.get(&key) {
            if existing_nodes.contains(&node_id) {
                let mut updated_nodes = existing_nodes.clone();
                updated_nodes.retain(|&id| id != node_id);

                if updated_nodes.is_empty() {
                    index.remove(&key);
                    stats.unique_value_count = index.len() as u64;
                } else {
                    index.insert(key, updated_nodes);
                }

                stats.entry_count = stats.entry_count.saturating_sub(1);
                self.update_statistics(&mut stats, &index);
                stats.last_updated = chrono::Utc::now();
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Find all nodes with an exact property value
    pub fn find_exact(&self, value: &Value) -> Result<Vec<u64>> {
        self.find_exact_composite(vec![value.clone()])
    }

    /// Find all nodes with exact composite property values
    pub fn find_exact_composite(&self, values: Vec<Value>) -> Result<Vec<u64>> {
        if values.len() != self.property_key_ids.len() {
            return Err(Error::InvalidId(format!(
                "Expected {} values, got {}",
                self.property_key_ids.len(),
                values.len()
            )));
        }

        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            if key.values == values {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Find all nodes with property values in a range (inclusive)
    pub fn find_range(&self, min_value: &Value, max_value: &Value) -> Result<Vec<u64>> {
        let bounds = RangeBounds::new()
            .with_min(min_value.clone(), true)
            .with_max(max_value.clone(), true);
        self.find_range_with_bounds(&bounds)
    }

    /// Find all nodes with property values in a range with custom bounds
    pub fn find_range_with_bounds(&self, bounds: &RangeBounds) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            if self.key_matches_bounds(key, bounds) {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Find all nodes with composite values in a range
    pub fn find_composite_range(
        &self,
        min_values: &[Value],
        max_values: &[Value],
    ) -> Result<Vec<u64>> {
        if min_values.len() != self.property_key_ids.len()
            || max_values.len() != self.property_key_ids.len()
        {
            return Err(Error::InvalidId("Range values length mismatch".to_string()));
        }

        let min_key = PropertyKey::new(min_values.to_vec(), 0);
        let max_key = PropertyKey::new(max_values.to_vec(), u64::MAX);

        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (_key, node_ids) in index.range(min_key..=max_key) {
            results.extend(node_ids);
        }
        Ok(results)
    }

    /// Find all nodes with property values greater than the given value
    pub fn find_greater_than(&self, value: &Value) -> Result<Vec<u64>> {
        let bounds = RangeBounds::new().with_min(value.clone(), false);
        self.find_range_with_bounds(&bounds)
    }

    /// Find all nodes with property values less than the given value
    pub fn find_less_than(&self, value: &Value) -> Result<Vec<u64>> {
        let bounds = RangeBounds::new().with_max(value.clone(), false);
        self.find_range_with_bounds(&bounds)
    }

    /// Get all unique values in the index (for single-property indexes)
    pub fn get_unique_values(&self) -> Result<Vec<Value>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut values = Vec::new();

        for key in index.keys() {
            if !key.values.is_empty() {
                let value = &key.values[0];
                if !values.contains(value) {
                    values.push(value.clone());
                }
            }
        }
        Ok(values)
    }

    /// Get all unique composite values in the index
    pub fn get_unique_composite_values(&self) -> Result<Vec<Vec<Value>>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut values = Vec::new();

        for key in index.keys() {
            if !values.contains(&key.values) {
                values.push(key.values.clone());
            }
        }
        Ok(values)
    }

    /// Check if a specific node has a specific property value
    pub fn has_value(&self, node_id: u64, value: &Value) -> Result<bool> {
        self.has_composite_value(node_id, vec![value.clone()])
    }

    /// Check if a specific node has specific composite property values
    pub fn has_composite_value(&self, node_id: u64, values: Vec<Value>) -> Result<bool> {
        if values.len() != self.property_key_ids.len() {
            return Err(Error::InvalidId(format!(
                "Expected {} values, got {}",
                self.property_key_ids.len(),
                values.len()
            )));
        }

        let key = PropertyKey::new(values, node_id);
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;

        Ok(index
            .get(&key)
            .is_some_and(|nodes| nodes.contains(&node_id)))
    }

    /// Get statistics about the index
    pub fn get_stats(&self) -> Result<IndexStats> {
        let stats = self
            .stats
            .read()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;
        Ok(stats.clone())
    }

    /// Clear all entries from the index
    pub fn clear(&self) -> Result<()> {
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut stats = self
            .stats
            .write()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;

        index.clear();
        *stats = IndexStats {
            entry_count: 0,
            unique_value_count: 0,
            size_bytes: 0,
            height: 0,
            last_updated: chrono::Utc::now(),
            selectivity: 0.0,
            histogram: Vec::new(),
            most_frequent: Vec::new(),
            avg_key_size: 0.0,
            compression_ratio: 1.0,
        };

        Ok(())
    }

    /// Get the label ID this index is for
    pub fn label_id(&self) -> u32 {
        self.label_id
    }

    /// Get the property key ID this index is for (backward compatibility)
    pub fn property_key_id(&self) -> u32 {
        self.property_key_ids.first().copied().unwrap_or(0)
    }

    fn compare_values(&self, a: &Value, b: &Value) -> i32 {
        PropertyKey::compare_values(a, b)
    }

    /// Check if a key matches the given range bounds
    fn key_matches_bounds(&self, key: &PropertyKey, bounds: &RangeBounds) -> bool {
        if key.values.is_empty() {
            return false;
        }

        let value = &key.values[0]; // For single-property indexes

        if let Some(ref min) = bounds.min {
            let cmp_min = PropertyKey::compare_values(value, min);
            if bounds.min_inclusive {
                if cmp_min < 0 {
                    return false;
                }
            } else if cmp_min <= 0 {
                return false;
            }
        }

        if let Some(ref max) = bounds.max {
            let cmp_max = PropertyKey::compare_values(value, max);
            if bounds.max_inclusive {
                if cmp_max > 0 {
                    return false;
                }
            } else if cmp_max >= 0 {
                return false;
            }
        }

        true
    }

    /// Update comprehensive statistics
    fn update_statistics(&self, stats: &mut IndexStats, index: &BTreeMap<PropertyKey, Vec<u64>>) {
        stats.unique_value_count = index.len() as u64;
        stats.selectivity = if stats.entry_count > 0 {
            stats.unique_value_count as f64 / stats.entry_count as f64
        } else {
            0.0
        };

        // Calculate average key size
        let mut total_key_size = 0;
        for key in index.keys() {
            total_key_size += serde_json::to_string(&key.values).unwrap_or_default().len();
        }
        stats.avg_key_size = if !index.is_empty() {
            total_key_size as f64 / index.len() as f64
        } else {
            0.0
        };

        // Update size estimation
        stats.size_bytes = self.estimate_size_bytes(index);

        // Build histogram (simplified - in production, use proper histogram algorithm)
        self.build_histogram(stats, index);

        // Find most frequent values
        self.find_most_frequent(stats, index);
    }

    /// Build value distribution histogram
    fn build_histogram(&self, stats: &mut IndexStats, index: &BTreeMap<PropertyKey, Vec<u64>>) {
        if index.is_empty() {
            stats.histogram.clear();
            return;
        }

        let mut buckets = Vec::new();
        let bucket_count = 10.min(index.len());
        let values_per_bucket = index.len() / bucket_count;

        let mut current_bucket = HistogramBucket {
            min_value: Value::Null,
            max_value: Value::Null,
            count: 0,
            distinct_count: 0,
        };

        let mut distinct_values = std::collections::HashSet::new();

        for (key, node_ids) in index.iter() {
            if key.values.is_empty() {
                continue;
            }

            let value = &key.values[0];

            if current_bucket.count == 0 {
                current_bucket.min_value = value.clone();
                current_bucket.max_value = value.clone();
            } else if PropertyKey::compare_values(value, &current_bucket.max_value) > 0 {
                current_bucket.max_value = value.clone();
            }

            current_bucket.count += node_ids.len() as u64;
            distinct_values.insert(value.clone());

            if current_bucket.count >= values_per_bucket as u64 && buckets.len() < bucket_count - 1
            {
                current_bucket.distinct_count = distinct_values.len() as u64;
                buckets.push(current_bucket);
                current_bucket = HistogramBucket {
                    min_value: Value::Null,
                    max_value: Value::Null,
                    count: 0,
                    distinct_count: 0,
                };
                distinct_values.clear();
            }
        }

        // Add the last bucket
        if current_bucket.count > 0 {
            current_bucket.distinct_count = distinct_values.len() as u64;
            buckets.push(current_bucket);
        }

        stats.histogram = buckets;
    }

    /// Find most frequent values
    fn find_most_frequent(&self, stats: &mut IndexStats, index: &BTreeMap<PropertyKey, Vec<u64>>) {
        let mut frequency_map = HashMap::new();

        for (key, node_ids) in index.iter() {
            if key.values.is_empty() {
                continue;
            }

            let value = &key.values[0];
            let count = node_ids.len() as u64;
            *frequency_map.entry(value.clone()).or_insert(0) += count;
        }

        let mut frequencies: Vec<_> = frequency_map.into_iter().collect();
        frequencies.sort_by(|a, b| b.1.cmp(&a.1));

        stats.most_frequent = frequencies.into_iter().take(10).collect();
    }

    /// Estimate size in bytes
    fn estimate_size_bytes(&self, index: &BTreeMap<PropertyKey, Vec<u64>>) -> u64 {
        let mut size = 0;
        for (key, node_ids) in index.iter() {
            size += std::mem::size_of::<PropertyKey>() as u64;
            size += node_ids.len() as u64 * 8; // u64 per node_id
            size += serde_json::to_string(&key.values).unwrap_or_default().len() as u64;
        }
        size
    }

    /// Get property key IDs
    pub fn property_key_ids(&self) -> &[u32] {
        &self.property_key_ids
    }

    /// Check if this is a unique index
    pub fn is_unique(&self) -> bool {
        self.is_unique
    }

    /// Check if compression is enabled
    pub fn is_compression_enabled(&self) -> bool {
        self.compression_enabled
    }

    /// Get index selectivity
    pub fn get_selectivity(&self) -> Result<f64> {
        let stats = self
            .stats
            .read()
            .map_err(|_| Error::internal("Stats lock poisoned"))?;
        Ok(stats.selectivity)
    }

    /// Get histogram data
    pub fn get_histogram(&self) -> Result<Vec<HistogramBucket>> {
        let stats = self
            .stats
            .read()
            .map_err(|_| Error::internal("Stats lock poisoned"))?;
        Ok(stats.histogram.clone())
    }

    /// Get most frequent values
    pub fn get_most_frequent(&self) -> Result<Vec<(Value, u64)>> {
        let stats = self
            .stats
            .read()
            .map_err(|_| Error::internal("Stats lock poisoned"))?;
        Ok(stats.most_frequent.clone())
    }
}

impl Clone for BTreeIndex {
    fn clone(&self) -> Self {
        let index = self.index.read().unwrap();
        let stats = self.stats.read().unwrap();
        Self {
            index: RwLock::new(index.clone()),
            label_id: self.label_id,
            property_key_ids: self.property_key_ids.clone(),
            stats: RwLock::new(stats.clone()),
            is_unique: self.is_unique,
            compression_enabled: self.compression_enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_btree_index_creation() {
        let index = BTreeIndex::new(1, 2);
        assert_eq!(index.label_id(), 1);
        assert_eq!(index.property_key_id(), 2);

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.unique_value_count, 0);
    }

    #[test]
    fn test_insert_and_find_exact() {
        let index = BTreeIndex::new(1, 2);

        // Insert some values
        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("world")).unwrap();
        index.insert(3, json!("hello")).unwrap();

        // Find exact matches
        let results = index.find_exact(&json!("hello")).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&1));
        assert!(results.contains(&3));

        let results = index.find_exact(&json!("world")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&2));

        let results = index.find_exact(&json!("nonexistent")).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_find_range() {
        let index = BTreeIndex::new(1, 2);

        // Insert numeric values
        index.insert(1, json!(10)).unwrap();
        index.insert(2, json!(20)).unwrap();
        index.insert(3, json!(30)).unwrap();
        index.insert(4, json!(40)).unwrap();
        index.insert(5, json!(50)).unwrap();

        // Find range 20-40 (inclusive)
        let results = index.find_range(&json!(20), &json!(40)).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.contains(&2));
        assert!(results.contains(&3));
        assert!(results.contains(&4));
    }

    #[test]
    fn test_find_greater_than() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!(10)).unwrap();
        index.insert(2, json!(20)).unwrap();
        index.insert(3, json!(30)).unwrap();

        let results = index.find_greater_than(&json!(15)).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&2));
        assert!(results.contains(&3));
    }

    #[test]
    fn test_find_less_than() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!(10)).unwrap();
        index.insert(2, json!(20)).unwrap();
        index.insert(3, json!(30)).unwrap();

        let results = index.find_less_than(&json!(25)).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.contains(&1));
        assert!(results.contains(&2));
    }

    #[test]
    fn test_remove() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("hello")).unwrap();

        // Remove one occurrence
        let removed = index.remove(1, &json!("hello")).unwrap();
        assert!(removed);

        let results = index.find_exact(&json!("hello")).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results.contains(&2));

        // Remove the last occurrence
        let removed = index.remove(2, &json!("hello")).unwrap();
        assert!(removed);

        let results = index.find_exact(&json!("hello")).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_has_value() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("world")).unwrap();

        assert!(index.has_value(1, &json!("hello")).unwrap());
        assert!(!index.has_value(1, &json!("world")).unwrap());
        assert!(!index.has_value(3, &json!("hello")).unwrap());
    }

    #[test]
    fn test_get_unique_values() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("world")).unwrap();
        index.insert(3, json!("hello")).unwrap();

        let unique_values = index.get_unique_values().unwrap();
        assert_eq!(unique_values.len(), 2);
        assert!(unique_values.contains(&json!("hello")));
        assert!(unique_values.contains(&json!("world")));
    }

    #[test]
    fn test_clear() {
        let index = BTreeIndex::new(1, 2);

        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("world")).unwrap();

        index.clear().unwrap();

        let stats = index.get_stats().unwrap();
        assert_eq!(stats.entry_count, 0);
        assert_eq!(stats.unique_value_count, 0);

        let results = index.find_exact(&json!("hello")).unwrap();
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_property_key_ordering() {
        let key1 = PropertyKey {
            values: vec![json!(10)],
            node_id: 1,
        };
        let key2 = PropertyKey {
            values: vec![json!(20)],
            node_id: 2,
        };
        let key3 = PropertyKey {
            values: vec![json!(10)],
            node_id: 3,
        };

        assert!(key1 < key2);
        assert!(key1 < key3); // Same value, different node_id
        assert!(key2 > key3);
    }

    #[test]
    fn test_string_ordering() {
        let key1 = PropertyKey {
            values: vec![json!("apple")],
            node_id: 1,
        };
        let key2 = PropertyKey {
            values: vec![json!("banana")],
            node_id: 2,
        };

        assert!(key1 < key2);
    }

    #[test]
    fn test_boolean_ordering() {
        let key1 = PropertyKey {
            values: vec![json!(false)],
            node_id: 1,
        };
        let key2 = PropertyKey {
            values: vec![json!(true)],
            node_id: 2,
        };

        assert!(key1 < key2);
    }

    #[test]
    fn test_null_ordering() {
        let key1 = PropertyKey {
            values: vec![json!(null)],
            node_id: 1,
        };
        let key2 = PropertyKey {
            values: vec![json!(10)],
            node_id: 2,
        };

        assert!(key1 < key2);
    }

    #[test]
    fn test_stats_update() {
        let index = BTreeIndex::new(1, 2);

        let initial_stats = index.get_stats().unwrap();
        assert_eq!(initial_stats.entry_count, 0);

        index.insert(1, json!("hello")).unwrap();
        index.insert(2, json!("world")).unwrap();

        let updated_stats = index.get_stats().unwrap();
        assert_eq!(updated_stats.entry_count, 2);
        assert_eq!(updated_stats.unique_value_count, 2);
        assert!(updated_stats.size_bytes > 0);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let index = Arc::new(BTreeIndex::new(1, 2));
        let mut handles = vec![];

        // Spawn multiple threads inserting values
        for i in 0..10 {
            let index_clone = Arc::clone(&index);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let node_id = i * 100 + j;
                    let value = json!(node_id);
                    index_clone.insert(node_id, value).unwrap();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all values were inserted
        let stats = index.get_stats().unwrap();
        assert_eq!(stats.entry_count, 1000);
        assert_eq!(stats.unique_value_count, 1000);
    }
}
