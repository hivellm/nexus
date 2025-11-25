//! Object Cache - Deserialized object caching
//!
//! This module provides caching for deserialized objects (nodes, relationships, properties)
//! to avoid repeated JSON deserialization overhead.
//!
//! ## Features
//!
//! - TTL-based eviction with configurable expiration
//! - Memory-bounded cache with automatic cleanup
//! - Support for different object types (Node, Relationship, Property)
//! - Thread-safe operations with interior mutability

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Key for cached objects
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ObjectKey {
    /// Node object
    Node(u64),
    /// Relationship object
    Relationship(u64),
    /// Property object (entity_id + key_id)
    Property(u64, u32),
    /// Label index entry
    Label(u32),
    /// Relationship type index entry
    RelationshipType(u32),
}

/// Cached object with metadata
#[derive(Debug, Clone)]
pub struct CachedObject {
    /// The actual cached data
    pub data: serde_json::Value,
    /// When this object was cached
    pub cached_at: Instant,
    /// How many times this object has been accessed
    pub access_count: u64,
    /// Memory size of the cached object
    pub memory_size: usize,
}

impl CachedObject {
    /// Create a new cached object
    pub fn new(data: serde_json::Value) -> Self {
        let memory_size = Self::estimate_memory_size(&data);
        Self {
            data,
            cached_at: Instant::now(),
            access_count: 0,
            memory_size,
        }
    }

    /// Check if the object has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }

    /// Record an access to this object
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }

    /// Estimate memory size of JSON value (rough approximation)
    fn estimate_memory_size(value: &serde_json::Value) -> usize {
        match value {
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

/// Object cache with TTL-based eviction
pub struct ObjectCache {
    /// Cache storage
    cache: Arc<RwLock<HashMap<ObjectKey, CachedObject>>>,
    /// Maximum memory usage
    max_memory: usize,
    /// Default TTL for objects
    default_ttl: Duration,
    /// Maximum size for individual objects
    max_object_size: usize,
    /// Current memory usage
    current_memory: Arc<AtomicUsize>,
    /// Cache statistics
    stats: Arc<ObjectCacheStats>,
}

#[derive(Debug, Default)]
struct ObjectCacheStats {
    hits: AtomicUsize,
    misses: AtomicUsize,
    evictions: AtomicUsize,
    inserts: AtomicUsize,
}

impl ObjectCache {
    /// Create a new object cache
    pub fn new(config: super::ObjectCacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            max_memory: config.max_memory,
            default_ttl: config.default_ttl,
            max_object_size: config.max_object_size,
            current_memory: Arc::new(AtomicUsize::new(0)),
            stats: Arc::new(ObjectCacheStats::default()),
        }
    }

    /// Get an object from the cache
    pub fn get(&self, key: &ObjectKey) -> Option<CachedObject> {
        let mut cache = self.cache.write().ok()?;

        if let Some(mut obj) = cache.get_mut(key) {
            // Check if expired
            if obj.is_expired(self.default_ttl) {
                // Remove expired object
                let memory_freed = obj.memory_size;
                drop(cache.remove(key));
                self.current_memory
                    .fetch_sub(memory_freed, Ordering::Relaxed);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats.misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // Record access and return clone
            obj.record_access();
            self.stats.hits.fetch_add(1, Ordering::Relaxed);
            Some(obj.clone())
        } else {
            self.stats.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Put an object in the cache
    pub fn put(&self, key: ObjectKey, data: serde_json::Value) {
        let obj = CachedObject::new(data);

        // Skip if object is too large
        if obj.memory_size > self.max_object_size {
            return;
        }

        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        // Check if we need to evict before inserting
        let new_memory_needed = obj.memory_size;
        if self.current_memory.load(Ordering::Relaxed) + new_memory_needed > self.max_memory {
            self.evict_expired_objects(&mut cache);
        }

        // If still need more space, evict LRU objects
        if self.current_memory.load(Ordering::Relaxed) + new_memory_needed > self.max_memory {
            self.evict_lru_objects(&mut cache, new_memory_needed);
        }

        // Insert the new object
        if let Some(old_obj) = cache.insert(key, obj.clone()) {
            // Adjust memory for replaced object
            self.current_memory
                .fetch_sub(old_obj.memory_size, Ordering::Relaxed);
        }

        self.current_memory
            .fetch_add(obj.memory_size, Ordering::Relaxed);
        self.stats.inserts.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove an object from the cache
    pub fn remove(&self, key: &ObjectKey) -> bool {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        if let Some(obj) = cache.remove(key) {
            self.current_memory
                .fetch_sub(obj.memory_size, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Clear all objects from the cache
    pub fn clear(&self) {
        let mut cache = match self.cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        cache.clear();
        self.current_memory.store(0, Ordering::Relaxed);
    }

    /// Get current cache size (number of objects)
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

    /// Evict expired objects
    fn evict_expired_objects(&self, cache: &mut HashMap<ObjectKey, CachedObject>) {
        let mut to_remove = Vec::new();
        let mut memory_freed = 0;

        for (key, obj) in cache.iter() {
            if obj.is_expired(self.default_ttl) {
                to_remove.push(key.clone());
                memory_freed += obj.memory_size;
            }
        }

        for key in to_remove {
            cache.remove(&key);
        }

        if memory_freed > 0 {
            self.current_memory
                .fetch_sub(memory_freed, Ordering::Relaxed);
            self.stats.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Evict LRU objects to free up space
    fn evict_lru_objects(&self, cache: &mut HashMap<ObjectKey, CachedObject>, needed_space: usize) {
        let mut candidates: Vec<(ObjectKey, u64, usize)> = cache
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
            if let Some(obj) = cache.remove(&key) {
                self.current_memory
                    .fetch_sub(obj.memory_size, Ordering::Relaxed);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_object_cache_creation() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 1024 * 1024,
            default_ttl: Duration::from_secs(60),
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_object_cache_put_get() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 1024 * 1024,
            default_ttl: Duration::from_secs(60),
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);
        let key = ObjectKey::Node(42);
        let data = serde_json::json!({"name": "test_node", "age": 30});

        cache.put(key.clone(), data.clone());

        let retrieved = cache.get(&key);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().data, data);
    }

    #[test]
    fn test_object_cache_expiration() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 1024 * 1024,
            default_ttl: Duration::from_millis(100), // Very short TTL
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);
        let key = ObjectKey::Node(1);
        let data = serde_json::json!({"test": true});

        cache.put(key.clone(), data);

        // Should be available immediately
        assert!(cache.get(&key).is_some());

        // Wait for expiration
        sleep(Duration::from_millis(200));

        // Should be expired now
        assert!(cache.get(&key).is_none());
    }

    #[test]
    fn test_object_cache_memory_limits() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 100, // Very small limit
            default_ttl: Duration::from_secs(60),
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);

        // Add objects until we hit memory limit
        for i in 0..10 {
            let key = ObjectKey::Node(i);
            let data = serde_json::json!({"data": "x".repeat(50)}); // Large object
            cache.put(key, data);
        }

        // Memory usage should be bounded
        assert!(cache.memory_usage() <= 200); // Should be close to limit
    }

    #[test]
    fn test_object_cache_clear() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 1024 * 1024,
            default_ttl: Duration::from_secs(60),
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);

        cache.put(ObjectKey::Node(1), serde_json::json!({"test": 1}));
        cache.put(ObjectKey::Node(2), serde_json::json!({"test": 2}));

        assert!(cache.size() > 0);

        cache.clear();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_object_cache_stats() {
        let config = super::super::ObjectCacheConfig {
            max_memory: 1024 * 1024,
            default_ttl: Duration::from_secs(60),
            max_object_size: 64 * 1024,
        };

        let cache = ObjectCache::new(config);

        // Generate some hits and misses
        let key1 = ObjectKey::Node(1);
        let key2 = ObjectKey::Node(2);

        cache.put(key1.clone(), serde_json::json!({"test": true}));

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
    fn test_object_key_types() {
        let key1 = ObjectKey::Node(1);
        let key2 = ObjectKey::Relationship(2);
        let key3 = ObjectKey::Property(3, 4);
        let key4 = ObjectKey::Label(5);
        let key5 = ObjectKey::RelationshipType(6);

        // Test Hash and Eq
        assert_ne!(key1, key2);
        assert_ne!(key2, key3);
        assert_ne!(key3, key4);
        assert_ne!(key4, key5);

        let key1_copy = ObjectKey::Node(1);
        assert_eq!(key1, key1_copy);
    }
}
