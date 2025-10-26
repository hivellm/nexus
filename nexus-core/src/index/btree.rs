//! B-tree index implementation for property range queries

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::RwLock;

/// B-tree index for property range queries
#[derive(Debug)]
pub struct BTreeIndex {
    index: RwLock<BTreeMap<PropertyKey, Vec<u64>>>,
    label_id: u32,
    property_key_id: u32,
    stats: RwLock<IndexStats>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyKey {
    pub value: Value,
    pub node_id: u64,
}

impl PartialOrd for PropertyKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PropertyKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match PropertyKey::compare_values(&self.value, &other.value) {
            x if x < 0 => std::cmp::Ordering::Less,
            x if x > 0 => std::cmp::Ordering::Greater,
            _ => self.node_id.cmp(&other.node_id),
        }
    }
}

impl PropertyKey {
    fn compare_values(a: &Value, b: &Value) -> i32 {
        match (a, b) {
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64
                    .partial_cmp(&b_f64)
                    .unwrap_or(std::cmp::Ordering::Equal) as i32
            }
            (Value::String(a_str), Value::String(b_str)) => a_str.cmp(b_str) as i32,
            (Value::Bool(a_bool), Value::Bool(b_bool)) => (*a_bool as i32) - (*b_bool as i32),
            (Value::Null, Value::Null) => 0,
            (Value::Null, _) => -1,
            (_, Value::Null) => 1,
            _ => format!("{:?}", a).cmp(&format!("{:?}", b)) as i32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub entry_count: u64,
    pub unique_value_count: u64,
    pub size_bytes: u64,
    pub height: u32,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

impl BTreeIndex {
    /// Create a new B-tree index for a specific label and property
    pub fn new(label_id: u32, property_key_id: u32) -> Self {
        Self {
            index: RwLock::new(BTreeMap::new()),
            label_id,
            property_key_id,
            stats: RwLock::new(IndexStats {
                entry_count: 0,
                unique_value_count: 0,
                size_bytes: 0,
                height: 0,
                last_updated: chrono::Utc::now(),
            }),
        }
    }

    /// Insert a node with a property value into the index
    pub fn insert(&self, node_id: u64, value: Value) -> Result<()> {
        let key = PropertyKey {
            value: value.clone(),
            node_id,
        };
        let mut index = self
            .index
            .write()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut stats = self
            .stats
            .write()
            .map_err(|_| Error::internal("B-tree stats lock poisoned"))?;

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

        // Update size estimation (rough calculation)
        stats.size_bytes = self.estimate_size_bytes(&index);
        stats.last_updated = chrono::Utc::now();

        Ok(())
    }

    /// Remove a node from the index
    pub fn remove(&self, node_id: u64, value: &Value) -> Result<bool> {
        let key = PropertyKey {
            value: value.clone(),
            node_id,
        };
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
                stats.size_bytes = self.estimate_size_bytes(&index);
                stats.last_updated = chrono::Utc::now();
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Find all nodes with an exact property value
    pub fn find_exact(&self, value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            if key.value == *value {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Find all nodes with property values in a range (inclusive)
    pub fn find_range(&self, min_value: &Value, max_value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            let cmp_min = self.compare_values(&key.value, min_value);
            let cmp_max = self.compare_values(&key.value, max_value);
            if cmp_min >= 0 && cmp_max <= 0 {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Find all nodes with property values greater than the given value
    pub fn find_greater_than(&self, value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            if self.compare_values(&key.value, value) > 0 {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Find all nodes with property values less than the given value
    pub fn find_less_than(&self, value: &Value) -> Result<Vec<u64>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut results = Vec::new();

        for (key, node_ids) in index.iter() {
            if self.compare_values(&key.value, value) < 0 {
                results.extend(node_ids);
            }
        }
        Ok(results)
    }

    /// Get all unique values in the index
    pub fn get_unique_values(&self) -> Result<Vec<Value>> {
        let index = self
            .index
            .read()
            .map_err(|_| Error::internal("B-tree index lock poisoned"))?;
        let mut values = Vec::new();

        for key in index.keys() {
            if !values.contains(&key.value) {
                values.push(key.value.clone());
            }
        }
        Ok(values)
    }

    /// Check if a specific node has a specific property value
    pub fn has_value(&self, node_id: u64, value: &Value) -> Result<bool> {
        let key = PropertyKey {
            value: value.clone(),
            node_id,
        };
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
        stats.entry_count = 0;
        stats.unique_value_count = 0;
        stats.size_bytes = 0;
        stats.last_updated = chrono::Utc::now();

        Ok(())
    }

    /// Get the label ID this index is for
    pub fn label_id(&self) -> u32 {
        self.label_id
    }

    /// Get the property key ID this index is for
    pub fn property_key_id(&self) -> u32 {
        self.property_key_id
    }

    fn compare_values(&self, a: &Value, b: &Value) -> i32 {
        PropertyKey::compare_values(a, b)
    }

    fn estimate_size_bytes(&self, index: &BTreeMap<PropertyKey, Vec<u64>>) -> u64 {
        // Rough estimation: each key + value pair
        let mut size = 0;
        for (key, node_ids) in index.iter() {
            size += std::mem::size_of::<PropertyKey>() as u64;
            size += node_ids.len() as u64 * 8; // u64 per node_id
            size += serde_json::to_string(&key.value).unwrap_or_default().len() as u64;
        }
        size
    }
}

impl Clone for BTreeIndex {
    fn clone(&self) -> Self {
        let index = self.index.read().unwrap();
        let stats = self.stats.read().unwrap();
        Self {
            index: RwLock::new(index.clone()),
            label_id: self.label_id,
            property_key_id: self.property_key_id,
            stats: RwLock::new(stats.clone()),
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
            value: json!(10),
            node_id: 1,
        };
        let key2 = PropertyKey {
            value: json!(20),
            node_id: 2,
        };
        let key3 = PropertyKey {
            value: json!(10),
            node_id: 3,
        };

        assert!(key1 < key2);
        assert!(key1 < key3); // Same value, different node_id
        assert!(key2 > key3);
    }

    #[test]
    fn test_string_ordering() {
        let key1 = PropertyKey {
            value: json!("apple"),
            node_id: 1,
        };
        let key2 = PropertyKey {
            value: json!("banana"),
            node_id: 2,
        };

        assert!(key1 < key2);
    }

    #[test]
    fn test_boolean_ordering() {
        let key1 = PropertyKey {
            value: json!(false),
            node_id: 1,
        };
        let key2 = PropertyKey {
            value: json!(true),
            node_id: 2,
        };

        assert!(key1 < key2);
    }

    #[test]
    fn test_null_ordering() {
        let key1 = PropertyKey {
            value: json!(null),
            node_id: 1,
        };
        let key2 = PropertyKey {
            value: json!(10),
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
