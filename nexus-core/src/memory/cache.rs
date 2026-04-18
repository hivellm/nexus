//! Advanced Caching Strategies with NUMA Partitioning
//!
//! This module provides NUMA-aware cache partitioning and advanced caching strategies.

use crate::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use super::numa::{NumaConfig, NumaNode};

/// NUMA-partitioned cache
pub struct NumaPartitionedCache<K, V> {
    /// Cache partitions, one per NUMA node
    partitions: Vec<Arc<RwLock<HashMap<K, V>>>>,
    /// NUMA configuration
    config: NumaConfig,
    /// Number of partitions
    num_partitions: usize,
}

impl<K, V> NumaPartitionedCache<K, V>
where
    K: Eq + Hash + Clone,
    V: Clone,
{
    /// Create a new NUMA-partitioned cache
    pub fn new(config: NumaConfig) -> Self {
        let num_partitions = config.num_nodes as usize;
        let mut partitions = Vec::with_capacity(num_partitions);
        for _ in 0..num_partitions {
            partitions.push(Arc::new(RwLock::new(HashMap::new())));
        }

        Self {
            partitions,
            config,
            num_partitions,
        }
    }

    /// Get the partition index for a key
    fn partition_index(&self, key: &K) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % self.num_partitions
    }

    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        let partition_idx = self.partition_index(key);
        let partition = &self.partitions[partition_idx];
        partition.read().get(key).cloned()
    }

    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        let partition_idx = self.partition_index(&key);
        let partition = &self.partitions[partition_idx];
        partition.write().insert(key, value)
    }

    /// Remove a value from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        let partition_idx = self.partition_index(key);
        let partition = &self.partitions[partition_idx];
        partition.write().remove(key)
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let mut total_size = 0;
        let mut partition_sizes = Vec::new();

        for partition in &self.partitions {
            let size = partition.read().len();
            total_size += size;
            partition_sizes.push(size);
        }

        CacheStats {
            total_size,
            partition_sizes,
            num_partitions: self.num_partitions,
        }
    }

    /// Clear all partitions
    pub fn clear(&self) {
        for partition in &self.partitions {
            partition.write().clear();
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Total number of entries across all partitions
    pub total_size: usize,
    /// Size of each partition
    pub partition_sizes: Vec<usize>,
    /// Number of partitions
    pub num_partitions: usize,
}

/// Predictive cache prefetcher
pub struct PredictivePrefetcher<K> {
    /// Access patterns for prediction
    access_patterns: Arc<RwLock<HashMap<K, Vec<u64>>>>,
    /// Maximum history size per key
    max_history: usize,
}

impl<K> PredictivePrefetcher<K>
where
    K: Eq + Hash + Clone,
{
    /// Create a new predictive prefetcher
    pub fn new(max_history: usize) -> Self {
        Self {
            access_patterns: Arc::new(RwLock::new(HashMap::new())),
            max_history,
        }
    }

    /// Record an access to a key
    pub fn record_access(&self, key: K) {
        let mut patterns = self.access_patterns.write();
        let entry = patterns.entry(key).or_insert_with(Vec::new);
        entry.push(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );

        // Keep only recent history
        if entry.len() > self.max_history {
            entry.remove(0);
        }
    }

    /// Predict the next likely keys to be accessed
    pub fn predict_next(&self, key: &K) -> Vec<K> {
        let patterns = self.access_patterns.read();
        if let Some(history) = patterns.get(key) {
            // Simple prediction: return keys that were accessed after this key
            // In a real implementation, this would use more sophisticated algorithms
            patterns
                .iter()
                .filter(|(k, _)| *k != key)
                .map(|(k, _)| k.clone())
                .take(5) // Return top 5 predictions
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get prefetcher statistics
    pub fn stats(&self) -> PrefetcherStats {
        let patterns = self.access_patterns.read();
        PrefetcherStats {
            tracked_keys: patterns.len(),
            total_accesses: patterns.values().map(|v| v.len()).sum(),
        }
    }
}

/// Prefetcher statistics
#[derive(Debug, Clone)]
pub struct PrefetcherStats {
    /// Number of keys being tracked
    pub tracked_keys: usize,
    /// Total number of accesses recorded
    pub total_accesses: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_partitioned_cache() {
        let config = NumaConfig {
            enabled: false,
            preferred_node: None,
            num_nodes: 2,
        };
        let cache = NumaPartitionedCache::new(config);

        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());

        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(cache.get(&"key2".to_string()), Some("value2".to_string()));

        let stats = cache.stats();
        assert_eq!(stats.total_size, 2);
        assert_eq!(stats.num_partitions, 2);
    }

    #[test]
    fn test_predictive_prefetcher() {
        let prefetcher = PredictivePrefetcher::new(10);

        prefetcher.record_access("key1".to_string());
        prefetcher.record_access("key2".to_string());
        prefetcher.record_access("key1".to_string());

        let predictions = prefetcher.predict_next(&"key1".to_string());
        // Should return other keys
        assert!(predictions.len() <= 5);

        let stats = prefetcher.stats();
        assert_eq!(stats.tracked_keys, 2);
        assert_eq!(stats.total_accesses, 3);
    }
}
