//! Vectorizer Query Caching Layer
//!
//! Provides advanced caching capabilities for vectorizer queries including:
//! - Multiple eviction policies (LRU, LFU, TTL, Size-based)
//! - Cache warming and preloading
//! - Performance metrics and monitoring
//! - Memory-efficient storage
//! - Thread-safe concurrent access

use crate::error::Result;
use crate::performance::cache::{CacheMetrics, EvictionPolicy, EvictionReason};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::RwLock;

/// Vectorizer query cache with advanced eviction policies
#[derive(Debug)]
pub struct VectorizerCache {
    /// Cache storage
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// LRU tracking for LRU eviction
    lru_order: Arc<RwLock<VecDeque<String>>>,
    /// LFU tracking for LFU eviction
    lfu_counts: Arc<RwLock<HashMap<String, u64>>>,
    /// Cache configuration
    config: CacheConfig,
    /// Performance metrics
    metrics: Arc<RwLock<CacheMetrics>>,
    /// Cache statistics
    stats: Arc<RwLock<CacheStatistics>>,
}

/// Cache entry containing the query result and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    /// The cached query result
    pub result: serde_json::Value,
    /// When this entry was created
    pub created_at: SystemTime,
    /// When this entry was last accessed
    pub last_accessed: SystemTime,
    /// Number of times this entry has been accessed
    pub access_count: u64,
    /// Size of the entry in bytes (approximate)
    pub size_bytes: usize,
    /// Query metadata for cache invalidation
    pub query_metadata: QueryMetadata,
}

/// Query metadata for cache invalidation and management
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryMetadata {
    /// Query type (semantic, metadata, hybrid)
    pub query_type: String,
    /// Collection name
    pub collection: String,
    /// Query string or parameters
    pub query_string: String,
    /// Similarity threshold used
    pub threshold: Option<f32>,
    /// Limit used in query
    pub limit: Option<usize>,
    /// Additional filters
    pub filters: Option<serde_json::Value>,
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum number of entries in cache
    pub max_entries: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Default TTL for entries
    pub default_ttl: Duration,
    /// Eviction policy to use
    pub eviction_policy: EvictionPolicy,
    /// Enable cache warming
    pub enable_warming: bool,
    /// Enable preloading
    pub enable_preloading: bool,
    /// Cache warming batch size
    pub warming_batch_size: usize,
    /// Preload prediction window
    pub preload_window: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 10000,
            max_memory_bytes: 100 * 1024 * 1024, // 100MB
            default_ttl: Duration::from_secs(3600), // 1 hour
            eviction_policy: EvictionPolicy::Lru,
            enable_warming: true,
            enable_preloading: true,
            warming_batch_size: 100,
            preload_window: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CacheStatistics {
    /// Total number of cache hits
    pub hits: u64,
    /// Total number of cache misses
    pub misses: u64,
    /// Total number of evictions
    pub evictions: u64,
    /// Total memory usage in bytes
    pub memory_usage: usize,
    /// Average hit rate
    pub hit_rate: f64,
    /// Average response time for cache hits (microseconds)
    pub avg_hit_time_us: u64,
    /// Average response time for cache misses (microseconds)
    pub avg_miss_time_us: u64,
    /// Number of cache warming operations
    pub warming_operations: u64,
    /// Number of preload operations
    pub preload_operations: u64,
}

impl VectorizerCache {
    /// Create a new vectorizer cache with default configuration
    pub fn new() -> Self {
        Self::with_config(CacheConfig::default())
    }

    /// Create a new vectorizer cache with custom configuration
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            lru_order: Arc::new(RwLock::new(VecDeque::new())),
            lfu_counts: Arc::new(RwLock::new(HashMap::new())),
            config,
            metrics: Arc::new(RwLock::new(CacheMetrics::new())),
            stats: Arc::new(RwLock::new(CacheStatistics::default())),
        }
    }

    /// Get a cached result if it exists and is still valid
    pub async fn get(&self, cache_key: &str) -> Result<Option<serde_json::Value>> {
        let start_time = Instant::now();
        
        // Check if entry exists
        let entry = {
            let cache = self.cache.read().await;
            cache.get(cache_key).cloned()
        };

        match entry {
            Some(entry) => {
                // Check if entry is still valid (not expired)
                if self.is_entry_valid(&entry).await {
                    // Update access tracking
                    self.update_access_tracking(cache_key).await;
                    
                    // Record hit
                    self.record_hit(start_time).await;
                    
                    Ok(Some(entry.result))
                } else {
                    // Entry expired, remove it
                    self.remove_entry(cache_key).await;
                    self.record_miss(start_time).await;
                    Ok(None)
                }
            }
            None => {
                self.record_miss(start_time).await;
                Ok(None)
            }
        }
    }

    /// Store a result in the cache
    pub async fn put(
        &self,
        cache_key: String,
        result: serde_json::Value,
        query_metadata: QueryMetadata,
        _ttl: Option<Duration>,
    ) -> Result<()> {
        let _start_time = Instant::now();
        
        // Calculate entry size
        let size_bytes = self.calculate_entry_size(&result, &query_metadata);
        
        // Create cache entry
        let entry = CacheEntry {
            result: result.clone(),
            created_at: SystemTime::now(),
            last_accessed: SystemTime::now(),
            access_count: 0,
            size_bytes,
            query_metadata,
        };

        // Check if we need to evict entries
        self.ensure_space_available(size_bytes).await?;

        // Store the entry
        {
            let mut cache = self.cache.write().await;
            cache.insert(cache_key.clone(), entry);
        }

        // Update tracking structures
        self.update_tracking_structures(&cache_key).await;

        // Update statistics
        self.update_memory_usage(size_bytes as isize).await;

        Ok(())
    }

    /// Remove an entry from the cache
    pub async fn remove(&self, cache_key: &str) -> Result<()> {
        self.remove_entry(cache_key).await;
        Ok(())
    }

    /// Clear all entries from the cache
    pub async fn clear(&self) -> Result<()> {
        {
            let mut cache = self.cache.write().await;
            cache.clear();
        }
        
        {
            let mut lru_order = self.lru_order.write().await;
            lru_order.clear();
        }
        
        {
            let mut lfu_counts = self.lfu_counts.write().await;
            lfu_counts.clear();
        }

        // Reset statistics
        {
            let mut stats = self.stats.write().await;
            stats.memory_usage = 0;
        }

        Ok(())
    }

    /// Get cache statistics
    pub async fn get_statistics(&self) -> CacheStatistics {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Get cache metrics
    pub async fn get_metrics(&self) -> CacheMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Warm the cache with frequently accessed queries
    pub async fn warm_cache(&self, queries: Vec<(String, serde_json::Value, QueryMetadata)>) -> Result<()> {
        if !self.config.enable_warming {
            return Ok(());
        }

        let mut warming_count = 0;
        
        for (cache_key, result, query_metadata) in queries {
            if warming_count >= self.config.warming_batch_size {
                break;
            }

            self.put(cache_key, result, query_metadata, None).await?;
            warming_count += 1;
        }

        // Update warming statistics
        {
            let mut stats = self.stats.write().await;
            stats.warming_operations += 1;
        }

        Ok(())
    }

    /// Preload cache based on access patterns
    pub async fn preload_cache(&self, access_patterns: Vec<String>) -> Result<()> {
        if !self.config.enable_preloading {
            return Ok(());
        }

        // This is a simplified preloading implementation
        // In a real system, this would analyze access patterns and predict future queries
        for pattern in access_patterns {
            // For now, just log the pattern for future implementation
            // tracing::debug!("Preloading pattern: {}", pattern);
            let _ = pattern; // Suppress unused variable warning
        }

        // Update preload statistics
        {
            let mut stats = self.stats.write().await;
            stats.preload_operations += 1;
        }

        Ok(())
    }

    /// Invalidate cache entries matching a pattern
    pub async fn invalidate_pattern(&self, pattern: &str) -> Result<usize> {
        let mut invalidated = 0;
        let keys_to_remove: Vec<String> = {
            let cache = self.cache.read().await;
            cache
                .keys()
                .filter(|key| key.contains(pattern))
                .cloned()
                .collect()
        };

        for key in keys_to_remove {
            self.remove_entry(&key).await;
            invalidated += 1;
        }

        Ok(invalidated)
    }

    /// Check if an entry is still valid (not expired)
    async fn is_entry_valid(&self, entry: &CacheEntry) -> bool {
        let ttl = self.config.default_ttl;
        
        if let Ok(elapsed) = entry.created_at.elapsed() {
            elapsed < ttl
        } else {
            false
        }
    }

    /// Update access tracking for LRU and LFU
    async fn update_access_tracking(&self, cache_key: &str) {
        // Update LRU order
        {
            let mut lru_order = self.lru_order.write().await;
            // Remove from current position if exists
            lru_order.retain(|key| key != cache_key);
            // Add to front (most recently used)
            lru_order.push_front(cache_key.to_string());
        }

        // Update LFU count
        {
            let mut lfu_counts = self.lfu_counts.write().await;
            *lfu_counts.entry(cache_key.to_string()).or_insert(0) += 1;
        }

        // Update access count in cache entry
        {
            let mut cache = self.cache.write().await;
            if let Some(entry) = cache.get_mut(cache_key) {
                entry.last_accessed = SystemTime::now();
                entry.access_count += 1;
            }
        }
    }

    /// Update tracking structures when adding a new entry
    async fn update_tracking_structures(&self, cache_key: &str) {
        // Add to LRU order
        {
            let mut lru_order = self.lru_order.write().await;
            lru_order.push_front(cache_key.to_string());
        }

        // Initialize LFU count
        {
            let mut lfu_counts = self.lfu_counts.write().await;
            lfu_counts.insert(cache_key.to_string(), 0);
        }
    }

    /// Remove an entry and update tracking structures
    async fn remove_entry(&self, cache_key: &str) {
        // Remove from cache
        let entry_size = {
            let mut cache = self.cache.write().await;
            let entry = cache.remove(cache_key);
            entry.map(|e| e.size_bytes).unwrap_or(0)
        };

        // Remove from LRU order
        {
            let mut lru_order = self.lru_order.write().await;
            lru_order.retain(|key| key != cache_key);
        }

        // Remove from LFU counts
        {
            let mut lfu_counts = self.lfu_counts.write().await;
            lfu_counts.remove(cache_key);
        }

        // Update memory usage
        if entry_size > 0 {
            self.update_memory_usage(-(entry_size as isize)).await;
        }

        // Record eviction
        self.record_eviction().await;
    }

    /// Ensure there's enough space for a new entry
    async fn ensure_space_available(&self, required_size: usize) -> Result<()> {
        let current_memory = {
            let stats = self.stats.read().await;
            stats.memory_usage
        };

        // Check if we need to evict entries
        if current_memory + required_size > self.config.max_memory_bytes {
            self.evict_entries(required_size).await?;
        }

        // Check entry count limit
        let current_entries = {
            let cache = self.cache.read().await;
            cache.len()
        };

        if current_entries >= self.config.max_entries {
            self.evict_entries_by_count(1).await?;
        }

        Ok(())
    }

    /// Evict entries to make space
    async fn evict_entries(&self, required_size: usize) -> Result<()> {
        let mut evicted_size = 0;
        let mut evicted_count = 0;

        while evicted_size < required_size && evicted_count < self.config.max_entries / 4 {
            let key_to_evict = self.select_eviction_candidate().await;
            
            if let Some(key) = key_to_evict {
                let entry_size = {
                    let cache = self.cache.read().await;
                    cache.get(&key).map(|e| e.size_bytes).unwrap_or(0)
                };
                
                self.remove_entry(&key).await;
                evicted_size += entry_size;
                evicted_count += 1;
            } else {
                break;
            }
        }

        Ok(())
    }

    /// Evict entries by count
    async fn evict_entries_by_count(&self, count: usize) -> Result<()> {
        for _ in 0..count {
            let key_to_evict = self.select_eviction_candidate().await;
            if let Some(key) = key_to_evict {
                self.remove_entry(&key).await;
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Select a candidate for eviction based on the eviction policy
    async fn select_eviction_candidate(&self) -> Option<String> {
        match self.config.eviction_policy {
            EvictionPolicy::Lru => {
                let mut lru_order = self.lru_order.write().await;
                lru_order.pop_back()
            }
            EvictionPolicy::Lfu => {
                let lfu_counts = self.lfu_counts.read().await;
                lfu_counts
                    .iter()
                    .min_by_key(|(_, count)| *count)
                    .map(|(key, _)| key.clone())
            }
            EvictionPolicy::Fifo => {
                let mut lru_order = self.lru_order.write().await;
                lru_order.pop_back()
            }
            EvictionPolicy::Random => {
                let cache = self.cache.read().await;
                cache.keys().next().cloned()
            }
            EvictionPolicy::Ttl => {
                // Find the oldest entry
                let cache = self.cache.read().await;
                cache
                    .iter()
                    .min_by_key(|(_, entry)| entry.created_at)
                    .map(|(key, _)| key.clone())
            }
        }
    }

    /// Calculate the approximate size of a cache entry
    fn calculate_entry_size(&self, result: &serde_json::Value, metadata: &QueryMetadata) -> usize {
        let result_size = serde_json::to_string(result)
            .map(|s| s.len())
            .unwrap_or(0);
        
        let metadata_size = serde_json::to_string(metadata)
            .map(|s| s.len())
            .unwrap_or(0);
        
        // Add overhead for the CacheEntry structure
        result_size + metadata_size + 200 // Approximate overhead
    }

    /// Update memory usage statistics
    async fn update_memory_usage(&self, size_delta: isize) {
        let mut stats = self.stats.write().await;
        if size_delta > 0 {
            stats.memory_usage += size_delta as usize;
        } else {
            stats.memory_usage = stats.memory_usage.saturating_sub((-size_delta) as usize);
        }
    }

    /// Record a cache hit
    async fn record_hit(&self, start_time: Instant) {
        let hit_time = start_time.elapsed().as_micros() as u64;
        
        {
            let mut metrics = self.metrics.write().await;
            metrics.hits += 1;
            metrics.last_access = Instant::now();
            metrics.access_count += 1;
        }

        {
            let mut stats = self.stats.write().await;
            stats.hits += 1;
            stats.avg_hit_time_us = (stats.avg_hit_time_us + hit_time) / 2;
            stats.hit_rate = stats.hits as f64 / (stats.hits + stats.misses) as f64;
        }
    }

    /// Record a cache miss
    async fn record_miss(&self, start_time: Instant) {
        let miss_time = start_time.elapsed().as_micros() as u64;
        
        {
            let mut metrics = self.metrics.write().await;
            metrics.misses += 1;
            metrics.last_access = Instant::now();
            metrics.access_count += 1;
        }

        {
            let mut stats = self.stats.write().await;
            stats.misses += 1;
            stats.avg_miss_time_us = (stats.avg_miss_time_us + miss_time) / 2;
            stats.hit_rate = stats.hits as f64 / (stats.hits + stats.misses) as f64;
        }
    }

    /// Record a cache eviction
    async fn record_eviction(&self) {
        {
            let mut metrics = self.metrics.write().await;
            metrics.evictions += 1;
            metrics.eviction_reasons
                .entry(EvictionReason::SizeLimit)
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }

        {
            let mut stats = self.stats.write().await;
            stats.evictions += 1;
        }
    }
}

impl Default for VectorizerCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = VectorizerCache::new();
        let stats = cache.get_statistics().await;
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }

    #[tokio::test]
    async fn test_cache_put_and_get() {
        let cache = VectorizerCache::new();
        
        let result = json!({"data": "test"});
        let metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: "test_collection".to_string(),
            query_string: "test query".to_string(),
            threshold: Some(0.8),
            limit: Some(10),
            filters: None,
        };

        cache.put("test_key".to_string(), result.clone(), metadata, None).await.unwrap();
        
        let cached_result = cache.get("test_key").await.unwrap();
        assert!(cached_result.is_some());
        assert_eq!(cached_result.unwrap(), result);
    }

    #[tokio::test]
    async fn test_cache_miss() {
        let cache = VectorizerCache::new();
        
        let result = cache.get("nonexistent_key").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_eviction() {
        let mut config = CacheConfig::default();
        config.max_entries = 2;
        let cache = VectorizerCache::with_config(config);
        
        let metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: "test".to_string(),
            query_string: "test".to_string(),
            threshold: None,
            limit: None,
            filters: None,
        };

        // Add entries up to the limit
        cache.put("key1".to_string(), json!({"data": "1"}), metadata.clone(), None).await.unwrap();
        cache.put("key2".to_string(), json!({"data": "2"}), metadata.clone(), None).await.unwrap();
        
        // Add one more to trigger eviction
        cache.put("key3".to_string(), json!({"data": "3"}), metadata, None).await.unwrap();
        
        // First key should be evicted (LRU)
        let result1 = cache.get("key1").await.unwrap();
        assert!(result1.is_none());
        
        // Other keys should still be there
        let result2 = cache.get("key2").await.unwrap();
        let result3 = cache.get("key3").await.unwrap();
        assert!(result2.is_some());
        assert!(result3.is_some());
    }

    #[tokio::test]
    async fn test_cache_clear() {
        let cache = VectorizerCache::new();
        
        let metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: "test".to_string(),
            query_string: "test".to_string(),
            threshold: None,
            limit: None,
            filters: None,
        };

        cache.put("key1".to_string(), json!({"data": "1"}), metadata, None).await.unwrap();
        cache.clear().await.unwrap();
        
        let result = cache.get("key1").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_cache_statistics() {
        let cache = VectorizerCache::new();
        
        let metadata = QueryMetadata {
            query_type: "semantic".to_string(),
            collection: "test".to_string(),
            query_string: "test".to_string(),
            threshold: None,
            limit: None,
            filters: None,
        };

        // Add an entry
        cache.put("key1".to_string(), json!({"data": "1"}), metadata, None).await.unwrap();
        
        // Hit
        cache.get("key1").await.unwrap();
        
        // Miss
        cache.get("key2").await.unwrap();
        
        let stats = cache.get_statistics().await;
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }
}