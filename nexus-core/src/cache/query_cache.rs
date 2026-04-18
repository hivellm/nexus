//! Query Cache - Execution plan and result caching
//!
//! This module provides caching for query execution plans and results
//! to accelerate repeated query execution.
//!
//! ## Features
//!
//! - LRU-based eviction for query plans and results
//! - Separate caches for plans vs results
//! - TTL-based expiration for results
//! - Query hash-based lookup for fast retrieval
//! - Memory-bounded operations

use crate::executor::ResultSet;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Key for query cache entries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryKey {
    /// Hash of the query string
    pub query_hash: String,
}

/// Cached query result with metadata
#[derive(Debug, Clone)]
pub struct CachedQueryResult {
    /// The cached result set
    pub result: ResultSet,
    /// When this result was cached
    pub cached_at: Instant,
    /// How many times this result has been accessed
    pub access_count: u64,
    /// Memory size estimate of the cached result
    pub memory_size: usize,
}

impl CachedQueryResult {
    /// Create a new cached query result
    pub fn new(result: ResultSet) -> Self {
        let memory_size = Self::estimate_memory_size(&result);
        Self {
            result,
            cached_at: Instant::now(),
            access_count: 0,
            memory_size,
        }
    }

    /// Check if the result has expired
    pub fn is_expired(&self, ttl: Duration) -> bool {
        self.cached_at.elapsed() > ttl
    }

    /// Record an access to this result
    pub fn record_access(&mut self) {
        self.access_count += 1;
    }

    /// Estimate memory size of ResultSet (rough approximation)
    fn estimate_memory_size(result: &ResultSet) -> usize {
        let mut size = 0;

        // Columns
        size += result.columns.len() * 32; // Rough string overhead

        // Rows
        for row in &result.rows {
            size += row.values.len() * 16; // JSON value overhead
            for value in &row.values {
                size += Self::estimate_json_size(value);
            }
        }

        size
    }

    /// Estimate memory size of a JSON value
    fn estimate_json_size(value: &serde_json::Value) -> usize {
        match value {
            serde_json::Value::Null => 4,
            serde_json::Value::Bool(_) => 1,
            serde_json::Value::Number(n) => n.to_string().len(),
            serde_json::Value::String(s) => s.len(),
            serde_json::Value::Array(arr) => {
                8 + arr
                    .iter()
                    .map(|v| Self::estimate_json_size(v))
                    .sum::<usize>()
            }
            serde_json::Value::Object(obj) => {
                8 + obj.keys().map(|k| k.len()).sum::<usize>()
                    + obj
                        .values()
                        .map(|v| Self::estimate_json_size(v))
                        .sum::<usize>()
            }
        }
    }
}

/// Cached query plan (placeholder for now - will be expanded)
#[derive(Debug, Clone)]
pub struct CachedQueryPlan {
    /// The cached plan (serialized AST or execution plan)
    pub plan: serde_json::Value,
    /// When this plan was cached
    pub cached_at: Instant,
    /// Access count
    pub access_count: u64,
}

/// LRU cache implementation for query results
struct LruCache<K, V> {
    cache: HashMap<K, V>,
    order: VecDeque<K>,
    capacity: usize,
}

impl<K, V> LruCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn new(capacity: usize) -> Self {
        Self {
            cache: HashMap::new(),
            order: VecDeque::new(),
            capacity,
        }
    }

    fn get(&mut self, key: &K) -> Option<&V> {
        if self.cache.contains_key(key) {
            // Move to front (most recently used)
            self.order.retain(|k| k != key);
            self.order.push_front(key.clone());
            self.cache.get(key)
        } else {
            None
        }
    }

    fn put(&mut self, key: K, value: V) {
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

    fn remove(&mut self, key: &K) -> Option<V> {
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
}

/// Query cache manager
pub struct QueryCache {
    /// Cache for query results
    result_cache: Arc<RwLock<LruCache<String, CachedQueryResult>>>,
    /// Cache for query plans
    plan_cache: Arc<RwLock<LruCache<String, CachedQueryPlan>>>,
    /// Maximum number of cached results
    max_results: usize,
    /// Maximum number of cached plans
    max_plans: usize,
    /// TTL for cached results
    result_ttl: Duration,
    /// Current memory usage estimate
    current_memory: Arc<AtomicUsize>,
    /// Statistics
    stats: Arc<QueryCacheStats>,
}

#[derive(Debug, Default)]
struct QueryCacheStats {
    result_hits: AtomicUsize,
    result_misses: AtomicUsize,
    plan_hits: AtomicUsize,
    plan_misses: AtomicUsize,
    evictions: AtomicUsize,
}

impl QueryCache {
    /// Create a new query cache
    pub fn new(config: super::QueryCacheConfig) -> Self {
        Self {
            result_cache: Arc::new(RwLock::new(LruCache::new(config.max_results))),
            plan_cache: Arc::new(RwLock::new(LruCache::new(config.max_plans))),
            max_results: config.max_results,
            max_plans: config.max_plans,
            result_ttl: config.result_ttl,
            current_memory: Arc::new(AtomicUsize::new(0)),
            stats: Arc::new(QueryCacheStats::default()),
        }
    }

    /// Get a cached query result
    pub fn get_result(&self, query_hash: &str) -> Option<CachedQueryResult> {
        let mut cache = self.result_cache.write().ok()?;

        if let Some(result) = cache.get(&query_hash.to_string()) {
            // Check if expired
            if result.is_expired(self.result_ttl) {
                // Remove expired result
                let memory_freed = result.memory_size;
                cache.remove(&query_hash.to_string());
                self.current_memory
                    .fetch_sub(memory_freed, Ordering::Relaxed);
                self.stats.evictions.fetch_add(1, Ordering::Relaxed);
                self.stats.result_misses.fetch_add(1, Ordering::Relaxed);
                return None;
            }

            // Record access (we need to modify the cached item)
            // This is a limitation - in a real implementation we'd use Arc<RwLock<CachedQueryResult>>
            self.stats.result_hits.fetch_add(1, Ordering::Relaxed);
            Some(result.clone())
        } else {
            self.stats.result_misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Cache a query result
    pub fn put_result(&self, query_hash: String, result: CachedQueryResult) {
        let mut cache = match self.result_cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        // Add to memory usage
        self.current_memory
            .fetch_add(result.memory_size, Ordering::Relaxed);

        // Put in cache (LRU will handle eviction)
        cache.put(query_hash, result);
    }

    /// Get a cached query plan
    pub fn get_plan(&self, query_hash: &str) -> Option<CachedQueryPlan> {
        let mut cache = self.plan_cache.write().ok()?;

        if let Some(plan) = cache.get(&query_hash.to_string()) {
            self.stats.plan_hits.fetch_add(1, Ordering::Relaxed);
            Some(plan.clone())
        } else {
            self.stats.plan_misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    /// Cache a query plan
    pub fn put_plan(&self, query_hash: String, plan: CachedQueryPlan) {
        let mut cache = match self.plan_cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        cache.put(query_hash, plan);
    }

    /// Remove a cached result
    pub fn remove_result(&self, query_hash: &str) -> bool {
        let mut cache = match self.result_cache.write() {
            Ok(c) => c,
            Err(_) => return false,
        };

        if let Some(result) = cache.remove(&query_hash.to_string()) {
            self.current_memory
                .fetch_sub(result.memory_size, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    /// Remove a cached plan
    pub fn remove_plan(&self, query_hash: &str) -> bool {
        let mut cache = self.plan_cache.write();
        match cache {
            Ok(mut c) => c.remove(&query_hash.to_string()).is_some(),
            Err(_) => false,
        }
    }

    /// Clear all cached results
    pub fn clear_results(&self) {
        let mut cache = match self.result_cache.write() {
            Ok(c) => c,
            Err(_) => return,
        };

        cache.clear();
        self.current_memory.store(0, Ordering::Relaxed);
    }

    /// Clear all cached plans
    pub fn clear_plans(&self) {
        let mut cache = self.plan_cache.write().ok();
        if let Some(mut c) = cache {
            c.clear();
        }
    }

    /// Clear all caches
    pub fn clear(&self) {
        self.clear_results();
        self.clear_plans();
    }

    /// Get current result cache size
    pub fn result_size(&self) -> usize {
        self.result_cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Get current plan cache size
    pub fn plan_size(&self) -> usize {
        self.plan_cache.read().map(|c| c.len()).unwrap_or(0)
    }

    /// Get total cache size
    pub fn size(&self) -> usize {
        self.result_size() + self.plan_size()
    }

    /// Get current memory usage estimate
    pub fn memory_usage(&self) -> usize {
        self.current_memory.load(Ordering::Relaxed)
    }

    /// Get cache statistics
    pub fn stats(&self) -> super::QueryCacheStats {
        super::QueryCacheStats {
            result_hits: self.stats.result_hits.load(Ordering::Relaxed),
            result_misses: self.stats.result_misses.load(Ordering::Relaxed),
            plan_hits: self.stats.plan_hits.load(Ordering::Relaxed),
            plan_misses: self.stats.plan_misses.load(Ordering::Relaxed),
            evictions: self.stats.evictions.load(Ordering::Relaxed),
        }
    }

    /// Calculate result cache hit rate
    pub fn result_hit_rate(&self) -> f64 {
        let stats = self.stats();
        let total = stats.result_hits + stats.result_misses;
        if total == 0 {
            0.0
        } else {
            stats.result_hits as f64 / total as f64
        }
    }

    /// Calculate plan cache hit rate
    pub fn plan_hit_rate(&self) -> f64 {
        let stats = self.stats();
        let total = stats.plan_hits + stats.plan_misses;
        if total == 0 {
            0.0
        } else {
            stats.plan_hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ResultSet;
    use std::thread::sleep;

    fn create_test_result() -> ResultSet {
        let mut result = ResultSet::default();
        result.columns = vec!["id".to_string(), "name".to_string()];
        result
    }

    #[test]
    fn test_query_cache_creation() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_result_cache_put_get() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);
        let query_hash = "SELECT * FROM test";
        let result = create_test_result();
        let cached_result = CachedQueryResult::new(result.clone());

        cache.put_result(query_hash.to_string(), cached_result);

        let retrieved = cache.get_result(query_hash);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().result.columns, result.columns);
    }

    #[test]
    fn test_result_cache_expiration() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_millis(100), // Very short TTL
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);
        let query_hash = "SELECT * FROM test";
        let result = create_test_result();
        let cached_result = CachedQueryResult::new(result);

        cache.put_result(query_hash.to_string(), cached_result);

        // Should be available immediately
        assert!(cache.get_result(query_hash).is_some());

        // Wait for expiration
        sleep(Duration::from_millis(200));

        // Should be expired now
        assert!(cache.get_result(query_hash).is_none());
    }

    #[test]
    fn test_result_cache_capacity() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 3, // Very small capacity
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);

        // Fill cache beyond capacity
        for i in 0..5 {
            let query_hash = format!("SELECT * FROM test{}", i);
            let result = create_test_result();
            let cached_result = CachedQueryResult::new(result);
            cache.put_result(query_hash, cached_result);
        }

        // Should not exceed capacity
        assert!(cache.result_size() <= 3);
    }

    #[test]
    fn test_plan_cache() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);
        let query_hash = "SELECT * FROM users";
        let plan = CachedQueryPlan {
            plan: serde_json::json!({"type": "scan", "table": "users"}),
            cached_at: Instant::now(),
            access_count: 0,
        };

        cache.put_plan(query_hash.to_string(), plan.clone());

        let retrieved = cache.get_plan(query_hash);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().plan, plan.plan);
    }

    #[test]
    fn test_cache_clear() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);

        // Add some data
        let result = create_test_result();
        let cached_result = CachedQueryResult::new(result);
        cache.put_result("query1".to_string(), cached_result);

        let plan = CachedQueryPlan {
            plan: serde_json::json!({"test": true}),
            cached_at: Instant::now(),
            access_count: 0,
        };
        cache.put_plan("plan1".to_string(), plan);

        assert!(cache.size() > 0);

        cache.clear();

        assert_eq!(cache.size(), 0);
        assert_eq!(cache.memory_usage(), 0);
    }

    #[test]
    fn test_cache_stats() {
        let config = super::super::QueryCacheConfig {
            max_plans: 100,
            max_results: 50,
            result_ttl: Duration::from_secs(60),
            min_execution_time: Duration::from_millis(10),
        };

        let cache = QueryCache::new(config);

        // Generate some hits and misses
        let result = create_test_result();
        let cached_result = CachedQueryResult::new(result);
        cache.put_result("query1".to_string(), cached_result);

        // Hit
        let _ = cache.get_result("query1");
        // Miss
        let _ = cache.get_result("query2");
        // Hit again
        let _ = cache.get_result("query1");

        let plan = CachedQueryPlan {
            plan: serde_json::json!({"test": true}),
            cached_at: Instant::now(),
            access_count: 0,
        };
        cache.put_plan("plan1".to_string(), plan);

        // Plan hit
        let _ = cache.get_plan("plan1");
        // Plan miss
        let _ = cache.get_plan("plan2");

        let stats = cache.stats();
        assert_eq!(stats.result_hits, 2);
        assert_eq!(stats.result_misses, 1);
        assert_eq!(stats.plan_hits, 1);
        assert_eq!(stats.plan_misses, 1);

        assert!(cache.result_hit_rate() > 0.5);
        assert_eq!(cache.plan_hit_rate(), 0.5);
    }

    #[test]
    fn test_cached_query_result_memory_estimation() {
        let result = create_test_result();
        let cached = CachedQueryResult::new(result);

        // Should have some memory size estimate
        assert!(cached.memory_size > 0);
    }
}
