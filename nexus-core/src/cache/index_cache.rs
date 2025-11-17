//! Index Cache - Accelerates index page lookups
//!
//! This module provides caching for frequently accessed index pages to accelerate
//! label lookups, property queries, and other index operations.
//!
//! ## Features
//!
//! - LRU-based eviction for index pages
//! - Support for different index types (Label, Property, KNN)
//! - Memory-bounded with configurable limits
//! - TTL-based expiration with automatic cleanup
//! - Thread-safe concurrent access

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Key for cached index entries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IndexKey {
    /// Label index entry (label_id)
    Label(u32),
    /// Property index entry (label_id, key_id)
    Property(u32, u32),
    /// KNN index entry (node_id)
    Knn(u64),
    /// Full-text index entry (field_hash)
    FullText(u64),
}

/// Cached index page with metadata
#[derive(Debug, Clone)]
pub struct CachedIndexPage {
    /// The cached index data (serialized or deserialized)
    pub data: serde_json::Value,
    /// When this page was cached
    pub cached_at: Instant,
    /// How many times this page has been accessed
    pub access_count: u64,
    /// Memory size estimate of the cached page
    pub memory_size: usize,
    /// Index type for statistics
    pub index_type: IndexType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexType {
    Label,
    Property,
    Knn,
    FullText,
}

impl CachedIndexPage {
    /// Create a new cached index page
    pub fn new(data: serde_json::Value, index_type: IndexType) -> Self {
        let memory_size = Self::estimate_memory_size(&data);
        Self {
            data,
            cached_at: Instant::now(),
            access_count: 0,
            memory_size,
            index_type,
        }
    }

    /// Check if the page has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }

    /// Record an access to this page
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }

    /// Estimate memory size of the index page data
    fn estimate_memory_size(data: &serde_json::Value) -> usize {
        match data {
            serde_json::Value::Null => 4,
            serde_json::Value::Bool(_) => 1,
            serde_json::Value::Number(n) => n.to_string().len(),
            serde_json::Value::String(s) => s.len(),
            serde_json::Value::Array(arr) => {
                8 + arr
                    .iter()
                    .map(|v| Self::estimate_memory_size(v))
                    .sum::<usize>()
            }
            serde_json::Value::Object(obj) => {
                8 + obj.keys().map(|k| k.len()).sum::<usize>()
                    + obj
                        .values()
                        .map(|v| Self::estimate_memory_size(v))
                        .sum::<usize>()
            }
        }
    }
}

/// LRU cache implementation for index pages
struct LruIndexCache {
    cache: HashMap<IndexKey, CachedIndexPage>,
    order: VecDeque<IndexKey>,
    capacity: usize,
}

impl LruIndexCache {
    fn new(capacity: usize) -> Self {
        Self {
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&mut self, key: &IndexKey) -> Option<&CachedIndexPage> {
        if self.cache.contains_key(key) {
            // Move to front (most recently used)
            self.order.retain(|k| k != key);
            self.order.push_front(key.clone());
            self.cache.get(key)
        } else {
            None
        }
    }

    fn put(&mut self, key: IndexKey, value: CachedIndexPage) {
        if self.cache.contains_key(&key) {
            // Update existing - just move to front
            self.order.retain(|k| k != &key);
            self.order.push_front(key.clone());
            self.cache.insert(key, value);
        } else {
            // New entry
            if self.cache.len() >= self.capacity {
                // Evict least recently used
                if let Some(lru_key) = self.order.pop_back() {
                    self.cache.remove(&lru_key);
                }
            }

            self.cache.insert(key.clone(), value);
            self.order.push_front(key);
        }
    }

    fn remove(&mut self, key: &IndexKey) -> Option<CachedIndexPage> {
        if self.cache.contains_key(key) {
            self.order.retain(|k| k != key);
        }
        self.cache.remove(key)
    }

    fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
    }

    fn len(&self) -> usize {
        self.cache.len()
    }

    fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    fn keys(&self) -> Vec<IndexKey> {
        self.cache.keys().cloned().collect()
    }
}

/// Index cache with TTL-based eviction
pub struct IndexCache {
    /// LRU cache storage
    cache: Arc<RwLock<LruIndexCache>>,
    /// Maximum memory usage
    max_memory: usize,
    /// Default TTL for index pages
    default_ttl: Duration,
    /// Maximum size for individual index pages
    max_page_size: usize,
    /// Current memory usage
    current_memory: Arc<AtomicUsize>,
    /// Statistics
    stats: Arc<IndexCacheStats>,
}

#[derive(Debug, Default)]
struct IndexCacheStats {
    hits: AtomicUsize,
    misses: AtomicUsize,
    evictions: AtomicUsize,
    inserts: AtomicUsize,
}

impl IndexCache {
    /// Create a new index cache
    pub fn new(config: super::IndexCacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(LruIndexCache::new(config.max_memory / 1024))), // Rough capacity estimate
            max_memory: config.max_memory,
            default_ttl: config.ttl,
            max_page_size: config.max_memory / 100, // Max 1% of total memory per page
            current_memory: Arc::new(AtomicUsize::new(0)),
            stats: Arc::new(IndexCacheStats::default()),
        }
    }

    /// Get an index page from the cache
    pub fn get(&self, key: &IndexKey) -> Option<CachedIndexPage> {
        let mut cache = self.cache.write().ok()?;

        if let Some(page) = cache.get(key) {
            // Check if expired
            if page.is_expired(self.default_ttl) {
                // Remove expired page
                let memory_freed = page.memory_size;
                drop(cache.remove(key));
                self.current_memory
                    .fetch_sub(memory_freed, Ordering::Relaxed);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // Record access (we need to modify the cached item)
            // This is a limitation - in a real implementation we'd use Arc<RwLock<CachedIndexPage>>
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            Some(page.clone())
        } else {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Put an index page in the cache
    pub fn put(&self, key: IndexKey, data: serde_json::Value, index_type: IndexType) {
        let page = CachedIndexPage::new(data, index_type);

        // Skip if page is too large
        if page.memory_size > self.max_page_size {
            return;
        }

        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        // Check if we need to evict before inserting
        let new_memory_needed = page.memory_size;
        if self.current_memory.load(Ordering::Relaxed) + new_memory_needed > self.max_memory {
            self.evict_expired_pages(&mut cache);
        }

        // If still need more space, evict LRU pages
        if self.current_memory.load(Ordering::Relaxed) + new_memory_needed > self.max_memory {
            self.evict_lru_pages(&mut cache, new_memory_needed);
        }

        // Insert the new page
        if let Some(old_page) = cache.remove(&key) {
            // Adjust memory for replaced page
            self.current_memory
                .fetch_sub(old_page.memory_size, Ordering::Relaxed);
        }

        self.current_memory
            .fetch_add(page.memory_size, Ordering::Relaxed);
        cache.put(key, page);
        self.stats.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove an index page from the cache
    pub fn remove(&self, key: &IndexKey) -> bool {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        if let Some(page) = cache.remove(key) {
            self.current_memory
                .fetch_sub(page.memory_size, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Clear all cached pages
    pub fn clear(&self) {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        cache.clear();
        self.current_memory.store(0, Ordering::Relaxed);
    }

    /// Get current cache size (number of pages)
    pub fn size(&self) -> usize {
        self.cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Get current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_memory.load(Ordering::Relaxed)
    }

    /// Get cache statistics
    pub fn stats(&self) -> super::ObjectCacheStats {
        super::ObjectCacheStats {
            hits: self.stats.hits.load(Ordering::Relaxed),
            misses: self.stats.misses.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
            inserts: self.stats.inserts.load(Ordering::Relaxed),
        }
    }

    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let stats = self.stats();
        let total = stats.hits + stats.misses;
        if total == 0 {
            0.0
        } else {
            stats.hits as f64 / total as f64
        }
    }

    /// Get all cached keys
    pub fn keys(&self) -> Vec<IndexKey> {
        self.cache.read().map(|c| c.keys()).unwrap_or_default()
    }

    /// Get statistics by index type
    pub fn stats_by_type(&self) -> HashMap<IndexType, IndexTypeStats> {
        let cache = match self.cache.read() {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let mut type_stats: HashMap<IndexType, IndexTypeStats> = HashMap::new();

        for page in cache.cache.values() {
            let stats = type_stats.entry(page.index_type).or_default();
            stats.count += 1;
            stats.total_memory += page.memory_size;
            stats.total_accesses += page.access_count;
        }

        type_stats
    }

    /// Evict expired pages
    fn evict_expired_pages(&self, cache: &mut LruIndexCache) {
        let keys_to_remove: Vec<IndexKey> = cache
            .cache
            .iter()
            .filter(|(_, page)| page.is_expired(self.default_ttl))
            .map(|(key, _)| key.clone())
            .collect();

        let mut memory_freed = 0;
        for key in keys_to_remove {
            if let Some(page) = cache.remove(&key) {
                memory_freed += page.memory_size;
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }

        if memory_freed > 0 {
            self.current_memory
                .fetch_sub(memory_freed, Ordering::Relaxed);
        }
    }

    /// Evict LRU pages to free up space
    fn evict_lru_pages(&self, cache: &mut LruIndexCache, needed_space: usize) {
        let mut candidates: Vec<(IndexKey, u64, usize)> = cache
            .cache
            .iter()
            .map(|(k, v)| (k.clone(), v.access_count, v.memory_size))
            .collect();

        // Sort by access count (ascending) - LRU first
        candidates.sort_by(|a, b| a.1.cmp(&b.1));

        let mut memory_freed = 0;
        let mut to_remove = Vec::new();

        for (key, _, size) in candidates {
            if memory_freed >= needed_space {
                break;
            }
            to_remove.push(key);
            memory_freed += size;
        }

        for key in to_remove {
            if let Some(page) = cache.remove(&key) {
                self.current_memory
                    .fetch_sub(page.memory_size, Ordering::Relaxed);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// Statistics for a specific index type
#[derive(Debug, Clone, Default)]
pub struct IndexTypeStats {
    /// Number of cached pages of this type
    pub count: usize,
    /// Total memory used by this type
    pub total_memory: usize,
    /// Total access count for this type
    pub total_accesses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_index_cache_creation() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_index_cache_put_get() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);
        let key = IndexKey::Label(42);
        let data = serde_json::json!({"bitmap": [1, 2, 3]});

        cache.put(key.clone(), data.clone(), IndexType::Label);

        let retrieved = cache.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().data, data);
    }

    #[test]
    fn test_index_cache_expiration() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_millis(100), // Very short TTL
        };

        let cache = IndexCache::new(config);
        let key = IndexKey::Property(1, 2);
        let data = serde_json::json!({"values": ["a", "b", "c"]});

        cache.put(key.clone(), data, IndexType::Property);

        // Should be available immediately
        assert!(cache.get(&key).is_some());

        // Wait for expiration
        sleep(Duration::from_millis(200));

        // Should be expired now
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_index_cache_memory_limits() {
        let config = super::super::IndexCacheConfig {
            max_memory: 200, // Very small limit
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);

        // Add pages until we hit memory limit
        for i in 0..10 {
            let key = IndexKey::Label(i);
            let data = serde_json::json!({"data": "x".repeat(50)}); // Large data
            cache.put(key, data, IndexType::Label);
        }

        // Memory usage should be bounded
        assert!(cache.memory_usage() <= 300); // Should be close to limit
    }

    #[test]
    fn test_index_cache_clear() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);

        cache.put(
            IndexKey::Label(1),
            serde_json::json!({"test": 1}),
            IndexType::Label,
        );
        cache.put(
            IndexKey::Property(1, 1),
            serde_json::json!({"test": 2}),
            IndexType::Property,
        );

        assert!(cache.size() > 0);

        cache.clear();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_index_cache_stats() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);

        // Generate some hits and misses
        let key1 = IndexKey::Label(1);
        let key2 = IndexKey::Label(2);

        cache.put(
            key1.clone(),
            serde_json::json!({"test": true}),
            IndexType::Label,
        );

        // Hit
        let _ = cache.get(&key1);
        // Miss
        let _ = cache.get(&key2);
        // Hit again
        let _ = cache.get(&key1);

        let stats = cache.stats();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.inserts, 1);

        assert!(cache.hit_rate() > 0.5);
    }

    #[test]
    fn test_index_cache_different_key_types() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);

        let key1 = IndexKey::Label(1);
        let key2 = IndexKey::Property(1, 2);
        let key3 = IndexKey::Knn(3);
        let key4 = IndexKey::FullText(4);

        cache.put(
            key1.clone(),
            serde_json::json!({"type": "label"}),
            IndexType::Label,
        );
        cache.put(
            key2.clone(),
            serde_json::json!({"type": "property"}),
            IndexType::Property,
        );
        cache.put(
            key3.clone(),
            serde_json::json!({"type": "knn"}),
            IndexType::Knn,
        );
        cache.put(
            key4.clone(),
            serde_json::json!({"type": "fulltext"}),
            IndexType::FullText,
        );

        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_some());
        assert!(cache.get(&key3).is_some());
        assert!(cache.get(&key4).is_some());
    }

    #[test]
    fn test_index_cache_stats_by_type() {
        let config = super::super::IndexCacheConfig {
            max_memory: 1024 * 1024,
            ttl: Duration::from_secs(60),
        };

        let cache = IndexCache::new(config);

        cache.put(
            IndexKey::Label(1),
            serde_json::json!({"l": 1}),
            IndexType::Label,
        );
        cache.put(
            IndexKey::Label(2),
            serde_json::json!({"l": 2}),
            IndexType::Label,
        );
        cache.put(
            IndexKey::Property(1, 1),
            serde_json::json!({"p": 1}),
            IndexType::Property,
        );

        let type_stats = cache.stats_by_type();

        assert_eq!(type_stats.get(&IndexType::Label).unwrap().count, 2);
        assert_eq!(type_stats.get(&IndexType::Property).unwrap().count, 1);
        assert_eq!(
            type_stats
                .get(&IndexType::Knn)
                .unwrap_or(&IndexTypeStats::default())
                .count,
            0
        );
    }

    #[test]
    fn test_index_key_hash_and_eq() {
        let key1 = IndexKey::Label(1);
        let key2 = IndexKey::Property(1, 2);
        let key3 = IndexKey::Knn(3);
        let key4 = IndexKey::FullText(4);

        // Test inequality
        assert_ne!(key1, key2);
        assert_ne!(key2, key3);
        assert_ne!(key3, key4);

        // Test equality with same values
        assert_eq!(key1, IndexKey::Label(1));
        assert_eq!(key2, IndexKey::Property(1, 2));
        assert_eq!(key3, IndexKey::Knn(3));
        assert_eq!(key4, IndexKey::FullText(4));
    }

    #[test]
    fn test_cached_index_page_memory_estimation() {
        let data = serde_json::json!({"array": [1, 2, 3, "test"], "nested": {"key": "value"}});
        let page = CachedIndexPage::new(data, IndexType::Label);

        // Should have some memory size estimate
        assert!(page.memory_size > 0);
    }
}
