//! Enhanced Vectorizer Query Caching Layer
//!
//! Provides intelligent caching for vectorizer queries with TTL and size limits

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cache entry with TTL
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Cached value
    value: serde_json::Value,
    /// Timestamp when cached
    cached_at: Instant,
    /// Time-to-live duration
    ttl: Duration,
    /// Number of times accessed
    access_count: usize,
    /// Last access time
    last_accessed: Instant,
}

impl CacheEntry {
    fn new(value: serde_json::Value, ttl: Duration) -> Self {
        let now = Instant::now();
        Self {
            value,
            cached_at: now,
            ttl,
            access_count: 0,
            last_accessed: now,
        }
    }

    fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.ttl
    }

    fn access(&mut self) -> serde_json::Value {
        self.access_count += 1;
        self.last_accessed = Instant::now();
        self.value.clone()
    }
}

/// Vectorizer query cache with TTL and LRU eviction
pub struct VectorizerQueryCache {
    /// Cache storage
    cache: HashMap<String, CacheEntry>,
    /// Maximum cache size (number of entries)
    max_size: usize,
    /// Default TTL for cache entries
    default_ttl: Duration,
    /// Cache statistics
    stats: CacheStatistics,
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStatistics {
    /// Total cache hits
    pub hits: usize,
    /// Total cache misses
    pub misses: usize,
    /// Total evictions
    pub evictions: usize,
    /// Total entries expired
    pub expirations: usize,
    /// Current cache size
    pub current_size: usize,
}

impl CacheStatistics {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl VectorizerQueryCache {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_size: 1000,
            default_ttl: Duration::from_secs(300), // 5 minutes
            stats: CacheStatistics::default(),
        }
    }

    /// Create a cache with custom settings
    pub fn with_config(max_size: usize, default_ttl: Duration) -> Self {
        Self {
            cache: HashMap::new(),
            max_size,
            default_ttl,
            stats: CacheStatistics::default(),
        }
    }

    /// Get a value from cache
    pub fn get(&mut self, key: &str) -> Option<serde_json::Value> {
        // Remove expired entries during get
        self.remove_expired();

        if let Some(entry) = self.cache.get_mut(key) {
            if entry.is_expired() {
                self.cache.remove(key);
                self.stats.expirations += 1;
                self.stats.misses += 1;
                None
            } else {
                self.stats.hits += 1;
                Some(entry.access())
            }
        } else {
            self.stats.misses += 1;
            None
        }
    }

    /// Insert a value into cache with default TTL
    pub fn insert(&mut self, key: String, value: serde_json::Value) {
        self.insert_with_ttl(key, value, self.default_ttl);
    }

    /// Insert a value into cache with custom TTL
    pub fn insert_with_ttl(&mut self, key: String, value: serde_json::Value, ttl: Duration) {
        // Evict if at capacity
        if self.cache.len() >= self.max_size && !self.cache.contains_key(&key) {
            self.evict_lru();
        }

        let entry = CacheEntry::new(value, ttl);
        self.cache.insert(key, entry);
        self.update_size();
    }

    /// Remove a specific key from cache
    pub fn remove(&mut self, key: &str) -> Option<serde_json::Value> {
        self.cache.remove(key).map(|entry| {
            self.update_size();
            entry.value
        })
    }

    /// Clear all cache entries
    pub fn clear(&mut self) {
        self.cache.clear();
        self.stats.current_size = 0;
    }

    /// Remove all expired entries
    pub fn remove_expired(&mut self) {
        let expired_keys: Vec<_> = self
            .cache
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(key, _)| key.clone())
            .collect();

        for key in expired_keys {
            self.cache.remove(&key);
            self.stats.expirations += 1;
        }

        self.update_size();
    }

    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some((lru_key, _)) = self
            .cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
        {
            let key = lru_key.clone();
            self.cache.remove(&key);
            self.stats.evictions += 1;
        }
    }

    /// Update current size statistic
    fn update_size(&mut self) {
        self.stats.current_size = self.cache.len();
    }

    /// Get cache statistics
    pub fn get_statistics(&self) -> CacheStatistics {
        self.stats.clone()
    }

    /// Reset statistics (keep cached data)
    pub fn reset_statistics(&mut self) {
        self.stats = CacheStatistics {
            current_size: self.cache.len(),
            ..Default::default()
        };
    }

    /// Get cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Get keys in cache
    pub fn keys(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }

    /// Check if key exists in cache
    pub fn contains_key(&self, key: &str) -> bool {
        if let Some(entry) = self.cache.get(key) {
            !entry.is_expired()
        } else {
            false
        }
    }
}

impl Default for VectorizerQueryCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache key builder for vectorizer queries
pub struct CacheKeyBuilder {
    parts: Vec<String>,
}

impl CacheKeyBuilder {
    /// Create a new cache key builder
    pub fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Add collection name
    pub fn collection(mut self, collection: &str) -> Self {
        self.parts.push(format!("col:{}", collection));
        self
    }

    /// Add query text
    pub fn query(mut self, query: &str) -> Self {
        self.parts.push(format!("q:{}", query));
        self
    }

    /// Add filter
    pub fn filter(mut self, filter: &str) -> Self {
        self.parts.push(format!("f:{}", filter));
        self
    }

    /// Add limit
    pub fn limit(mut self, limit: usize) -> Self {
        self.parts.push(format!("l:{}", limit));
        self
    }

    /// Build the cache key
    pub fn build(self) -> String {
        self.parts.join("|")
    }
}

impl Default for CacheKeyBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_insert_and_get() {
        let mut cache = VectorizerQueryCache::new();
        let value = serde_json::json!({"test": "data"});

        cache.insert("key1".to_string(), value.clone());
        assert_eq!(cache.get("key1"), Some(value));
    }

    #[test]
    fn test_cache_miss() {
        let mut cache = VectorizerQueryCache::new();
        assert_eq!(cache.get("nonexistent"), None);
        assert_eq!(cache.stats.misses, 1);
    }

    #[test]
    fn test_cache_hit() {
        let mut cache = VectorizerQueryCache::new();
        let value = serde_json::json!({"test": "data"});

        cache.insert("key1".to_string(), value.clone());
        let _ = cache.get("key1");

        assert_eq!(cache.stats.hits, 1);
    }

    #[test]
    fn test_cache_expiration() {
        let mut cache = VectorizerQueryCache::new();
        let value = serde_json::json!({"test": "data"});

        // Insert with very short TTL
        cache.insert_with_ttl("key1".to_string(), value, Duration::from_millis(1));

        // Wait for expiration
        std::thread::sleep(Duration::from_millis(10));

        assert_eq!(cache.get("key1"), None);
        assert!(cache.stats.expirations > 0);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = VectorizerQueryCache::with_config(2, Duration::from_secs(300));

        cache.insert("key1".to_string(), serde_json::json!({"a": 1}));
        cache.insert("key2".to_string(), serde_json::json!({"b": 2}));

        // This should trigger eviction
        cache.insert("key3".to_string(), serde_json::json!({"c": 3}));

        assert_eq!(cache.len(), 2);
        assert!(cache.stats.evictions > 0);
    }

    #[test]
    fn test_cache_clear() {
        let mut cache = VectorizerQueryCache::new();

        cache.insert("key1".to_string(), serde_json::json!({"a": 1}));
        cache.insert("key2".to_string(), serde_json::json!({"b": 2}));

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_hit_rate_calculation() {
        let mut cache = VectorizerQueryCache::new();
        let value = serde_json::json!({"test": "data"});

        cache.insert("key1".to_string(), value);

        let _ = cache.get("key1"); // hit
        let _ = cache.get("key1"); // hit
        let _ = cache.get("key2"); // miss

        let stats = cache.get_statistics();
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_key_builder() {
        let key = CacheKeyBuilder::new()
            .collection("functions")
            .query("search term")
            .filter("rust")
            .limit(10)
            .build();

        assert!(key.contains("col:functions"));
        assert!(key.contains("q:search term"));
        assert!(key.contains("f:rust"));
        assert!(key.contains("l:10"));
    }

    #[test]
    fn test_contains_key() {
        let mut cache = VectorizerQueryCache::new();
        cache.insert("key1".to_string(), serde_json::json!({"a": 1}));

        assert!(cache.contains_key("key1"));
        assert!(!cache.contains_key("key2"));
    }

    #[test]
    fn test_remove() {
        let mut cache = VectorizerQueryCache::new();
        let value = serde_json::json!({"test": "data"});

        cache.insert("key1".to_string(), value.clone());
        let removed = cache.remove("key1");

        assert_eq!(removed, Some(value));
        assert!(!cache.contains_key("key1"));
    }

    #[test]
    fn test_keys() {
        let mut cache = VectorizerQueryCache::new();

        cache.insert("key1".to_string(), serde_json::json!({"a": 1}));
        cache.insert("key2".to_string(), serde_json::json!({"b": 2}));

        let keys = cache.keys();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }
}
