//! Multi-Layer Cache System
//!
//! This module implements a sophisticated multi-layer caching architecture
//! to improve read performance across different data access patterns.
//!
//! ## Cache Layers Hierarchy
//!
//! ```text
//! ┌─────────────────┐
//! │  Query Cache    │ ← Execution plans & results (LRU)
//! ├─────────────────┤
//! │  Index Cache    │ ← Index pages & lookups (bounded)
//! ├─────────────────┤
//! │  Object Cache   │ ← Deserialized objects (TTL)
//! ├─────────────────┤
//! │  Page Cache     │ ← 8KB data pages (LRU + prefetch)
//! └─────────────────┘
//! ```
//!
//! ## Features
//!
//! - **Query Cache**: Caches execution plans and results for repeated queries
//! - **Index Cache**: Accelerates index lookups with memory-bounded storage
//! - **Object Cache**: Caches deserialized nodes, relationships, and properties
//! - **Page Cache**: Enhanced page cache with LRU eviction and prefetching
//! - **Unified API**: Single interface for cache operations across layers

use crate::Result;
use crate::executor::ResultSet;
use crate::page_cache::{Page, PageCache};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing;

// Re-export property index types
pub use property_index::{PropertyIndexManager, PropertyIndexStats};

pub mod index_cache;
pub mod object_cache;
pub mod performance_tests;
pub mod property_index;
pub mod query_cache;
pub mod relationship_index;

pub use index_cache::{CachedIndexPage, IndexCache, IndexKey};
pub use object_cache::{CachedObject, ObjectCache, ObjectKey};
pub use query_cache::{CachedQueryResult, QueryCache, QueryKey};
pub use relationship_index::RelationshipIndex;

/// Statistics for the object cache
#[derive(Debug, Clone, Default)]
pub struct ObjectCacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
    pub inserts: usize,
}

/// Statistics for the query cache
#[derive(Debug, Clone, Default)]
pub struct QueryCacheStats {
    pub result_hits: usize,
    pub result_misses: usize,
    pub plan_hits: usize,
    pub plan_misses: usize,
    pub evictions: usize,
}

/// Cache layer types for metrics and monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CacheLayer {
    Page,
    Object,
    Index,
    Query,
    Relationship,
}

/// Cache operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CacheOperation {
    Hit,
    Miss,
    Eviction,
    Invalidation,
}

/// Cache statistics for monitoring
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CacheStats {
    /// Total operations per layer
    pub operations: HashMap<CacheLayer, HashMap<CacheOperation, u64>>,
    /// Current cache sizes
    pub sizes: HashMap<CacheLayer, usize>,
    /// Memory usage per layer
    pub memory_usage: HashMap<CacheLayer, usize>,
    /// Hit rates per layer
    pub hit_rates: HashMap<CacheLayer, f64>,
    /// Eviction counts per layer
    pub evictions: HashMap<CacheLayer, u64>,
}

impl CacheStats {
    /// Record a cache operation
    pub fn record_operation(&mut self, layer: CacheLayer, operation: CacheOperation) {
        let layer_ops = self.operations.entry(layer).or_default();
        *layer_ops.entry(operation).or_insert(0) += 1;
    }

    /// Update cache size for a layer
    pub fn update_size(&mut self, layer: CacheLayer, size: usize) {
        self.sizes.insert(layer, size);
    }

    /// Update memory usage for a layer
    pub fn update_memory(&mut self, layer: CacheLayer, memory: usize) {
        self.memory_usage.insert(layer, memory);
    }

    /// Calculate hit rate for a layer
    pub fn calculate_hit_rate(&mut self, layer: CacheLayer) {
        if let Some(ops) = self.operations.get(&layer) {
            let hits = ops.get(&CacheOperation::Hit).copied().unwrap_or(0);
            let total = hits + ops.get(&CacheOperation::Miss).copied().unwrap_or(0);
            let hit_rate = if total > 0 {
                hits as f64 / total as f64
            } else {
                0.0
            };
            self.hit_rates.insert(layer, hit_rate);
        }
    }

    /// Get total operations across all layers
    pub fn total_operations(&self) -> u64 {
        self.operations.values().flat_map(|ops| ops.values()).sum()
    }
}

/// Configuration for the multi-layer cache system
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Page cache configuration
    pub page_cache: PageCacheConfig,
    /// Object cache configuration
    pub object_cache: ObjectCacheConfig,
    /// Query cache configuration
    pub query_cache: QueryCacheConfig,
    /// Index cache configuration
    pub index_cache: IndexCacheConfig,
    /// Global cache settings
    pub global: GlobalCacheConfig,
}

#[derive(Debug, Clone)]
pub struct PageCacheConfig {
    /// Maximum number of pages
    pub max_pages: usize,
    /// Enable prefetching
    pub enable_prefetch: bool,
    /// Prefetch distance (pages ahead/behind)
    pub prefetch_distance: usize,
}

#[derive(Debug, Clone)]
pub struct ObjectCacheConfig {
    /// Maximum memory usage (bytes)
    pub max_memory: usize,
    /// Default TTL for cached objects
    pub default_ttl: Duration,
    /// Maximum object size to cache
    pub max_object_size: usize,
}

#[derive(Debug, Clone)]
pub struct QueryCacheConfig {
    /// Maximum number of cached query plans
    pub max_plans: usize,
    /// Maximum number of cached results
    pub max_results: usize,
    /// Default TTL for cached results
    pub result_ttl: Duration,
    /// Minimum query execution time to cache
    pub min_execution_time: Duration,
}

#[derive(Debug, Clone)]
pub struct IndexCacheConfig {
    /// Maximum memory usage (bytes)
    pub max_memory: usize,
    /// Index entry TTL
    pub ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct GlobalCacheConfig {
    /// Enable cache warming on startup
    pub enable_warming: bool,
    /// Cache statistics collection interval
    pub stats_interval: Duration,
    /// Maximum total memory usage across all caches
    pub max_total_memory: usize,
    /// Cache warming configuration
    pub warming: CacheWarmingConfig,
}

/// Cache warming configuration
#[derive(Debug, Clone)]
pub struct CacheWarmingConfig {
    /// Enable automatic cache warming based on access patterns
    pub enable_auto_warming: bool,
    /// Maximum time to spend warming cache (seconds)
    pub max_warm_time_secs: u64,
    /// Minimum access count to consider for warming
    pub min_access_count: u64,
    /// Maximum number of items to warm per layer
    pub max_warm_items: usize,
}

impl Default for CacheWarmingConfig {
    fn default() -> Self {
        Self {
            enable_auto_warming: false,
            max_warm_time_secs: 60,
            min_access_count: 10,
            max_warm_items: 1000,
        }
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            page_cache: PageCacheConfig {
                max_pages: 1024, // ~8MB with 8KB pages
                enable_prefetch: true,
                prefetch_distance: 2,
            },
            object_cache: ObjectCacheConfig {
                max_memory: 50 * 1024 * 1024,          // 50MB
                default_ttl: Duration::from_secs(300), // 5 minutes
                max_object_size: 1024 * 1024,          // 1MB
            },
            query_cache: QueryCacheConfig {
                max_plans: 1000,
                max_results: 100,
                result_ttl: Duration::from_secs(60), // 1 minute
                min_execution_time: Duration::from_millis(10),
            },
            index_cache: IndexCacheConfig {
                max_memory: 100 * 1024 * 1024, // 100MB
                ttl: Duration::from_secs(600), // 10 minutes
            },
            global: GlobalCacheConfig {
                enable_warming: false,
                stats_interval: Duration::from_secs(60),
                max_total_memory: 200 * 1024 * 1024, // 200MB total
                warming: CacheWarmingConfig {
                    enable_auto_warming: true,
                    max_warm_time_secs: 30,
                    min_access_count: 5,
                    max_warm_items: 100,
                },
            },
        }
    }
}

/// Unified cache key for frequency tracking
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CacheKey {
    Page(u64),
    Object(String),
    Query(String),
    Index(String),
}

/// Property index key for efficient WHERE clause filtering
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropertyIndexKey {
    /// Property name (e.g., "age", "name")
    pub property_name: String,
    /// Property value
    pub property_value: String,
    /// Node ID that has this property
    pub node_id: u64,
}

/// Multi-layer cache manager
pub struct MultiLayerCache {
    /// Page cache (foundation layer)
    page_cache: PageCache,
    /// Object cache (deserialization layer)
    object_cache: ObjectCache,
    /// Query cache (execution layer)
    query_cache: QueryCache,
    /// Index cache (lookup acceleration layer)
    index_cache: IndexCache,
    /// Relationship index (relationship query acceleration layer)
    relationship_index: RelationshipIndex,
    /// Property index manager (WHERE clause acceleration layer)
    property_index_manager: PropertyIndexManager,
    /// Configuration
    config: CacheConfig,
    /// Statistics
    stats: CacheStats,
    /// Last stats update
    last_stats_update: Instant,
    /// Access frequency tracking for intelligent eviction
    access_frequency: HashMap<CacheKey, u64>,
    /// Last access time for temporal eviction
    last_access: HashMap<CacheKey, Instant>,
}

impl MultiLayerCache {
    /// Create a new multi-layer cache system
    pub fn new(config: CacheConfig) -> Result<Self> {
        let page_cache = PageCache::new(config.page_cache.max_pages)?;
        let object_cache = ObjectCache::new(config.object_cache.clone());
        let query_cache = QueryCache::new(config.query_cache.clone());
        let index_cache = IndexCache::new(config.index_cache.clone());
        let relationship_index = RelationshipIndex::new();
        let property_index_manager = PropertyIndexManager::new(
            config.global.max_total_memory / 4, // Use 1/4 of total cache memory
            Duration::from_secs(3600),          // 1 hour TTL
        );

        Ok(Self {
            page_cache,
            object_cache,
            query_cache,
            index_cache,
            relationship_index,
            property_index_manager,
            config,
            stats: CacheStats::default(),
            last_stats_update: Instant::now(),
            access_frequency: HashMap::new(),
            last_access: HashMap::new(),
        })
    }

    /// Track access patterns for intelligent eviction
    fn track_access(&mut self, key: CacheKey) {
        let now = Instant::now();

        // Update frequency
        *self.access_frequency.entry(key.clone()).or_insert(0) += 1;

        // Update last access time
        self.last_access.insert(key, now);

        // Periodic cleanup of old access patterns (keep last 1000 entries)
        if self.access_frequency.len() > 1000 {
            self.cleanup_access_tracking();
        }
    }

    /// Clean up old access tracking data
    fn cleanup_access_tracking(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(3600); // 1 hour ago

        // Remove entries older than cutoff
        self.last_access.retain(|_, time| *time > cutoff);

        // Keep only top 500 most frequent entries
        if self.access_frequency.len() > 500 {
            let mut entries: Vec<(CacheKey, u64)> = self.access_frequency.drain().collect();
            entries.sort_by(|a, b| b.1.cmp(&a.1)); // Sort by frequency descending

            // Keep only top 500
            entries.truncate(500);

            // Reinsert top entries
            for (key, freq) in entries {
                self.access_frequency.insert(key, freq);
            }
        }
    }

    /// Get page from cache (with prefetch if enabled)
    pub fn get_page(&mut self, page_id: u64) -> Result<Arc<Page>> {
        self.stats
            .record_operation(CacheLayer::Page, CacheOperation::Hit);

        // Track access for intelligent eviction
        self.track_access(CacheKey::Page(page_id));

        let page = self.page_cache.get_page(page_id)?;

        // Prefetch adjacent pages if enabled
        if self.config.page_cache.enable_prefetch {
            self.prefetch_pages(page_id);
        }

        Ok(page)
    }

    /// Get object from cache
    pub fn get_object(&mut self, key: &ObjectKey) -> Option<serde_json::Value> {
        match self.object_cache.get(key) {
            Some(obj) => {
                self.stats
                    .record_operation(CacheLayer::Object, CacheOperation::Hit);
                Some(obj.data)
            }
            None => {
                self.stats
                    .record_operation(CacheLayer::Object, CacheOperation::Miss);
                None
            }
        }
    }

    /// Put object in cache
    pub fn put_object(&mut self, key: ObjectKey, data: serde_json::Value) {
        self.object_cache.put(key, data);
    }

    /// Get cached query result
    pub fn get_query_result(&mut self, query_hash: &str) -> Option<CachedQueryResult> {
        match self.query_cache.get_result(query_hash) {
            Some(result) => {
                self.stats
                    .record_operation(CacheLayer::Query, CacheOperation::Hit);
                Some(result)
            }
            None => {
                self.stats
                    .record_operation(CacheLayer::Query, CacheOperation::Miss);
                None
            }
        }
    }

    /// Cache query result
    pub fn put_query_result(
        &mut self,
        query_hash: String,
        result: ResultSet,
        execution_time: Duration,
    ) {
        // Only cache if execution took long enough
        if execution_time >= self.config.query_cache.min_execution_time {
            let cached_result = CachedQueryResult::new(result);
            self.query_cache.put_result(query_hash, cached_result);
        }
    }

    /// Get cached query plan
    pub fn get_query_plan(&mut self, query_hash: &str) -> Option<serde_json::Value> {
        // For now, retrieve query plans from the index cache
        // TODO: Implement proper plan caching in QueryCache
        let hash = query_hash
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_add(b as u64));
        self.index_cache
            .get(&IndexKey::FullText(hash))
            .map(|page| page.data)
    }

    /// Cache query plan
    pub fn put_query_plan(&mut self, query_hash: String, plan: serde_json::Value) {
        // For now, store query plans in the index cache as a placeholder
        // TODO: Implement proper query plan caching in QueryCache
        self.index_cache.put(
            IndexKey::FullText(
                query_hash
                    .as_bytes()
                    .iter()
                    .fold(0u64, |acc, &b| acc.wrapping_add(b as u64)),
            ),
            plan,
            index_cache::IndexType::FullText,
        );
    }

    /// Get cache statistics
    pub fn stats(&mut self) -> &CacheStats {
        self.update_stats_if_needed();
        &self.stats
    }

    /// Get relationship index
    pub fn relationship_index(&self) -> &RelationshipIndex {
        &self.relationship_index
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        let _ = self.page_cache.clear();
        self.object_cache.clear();
        self.query_cache.clear();
        self.index_cache.clear();
        let _ = self.relationship_index.clear();
    }

    /// Prefetch pages around the given page ID
    fn prefetch_pages(&mut self, page_id: u64) {
        let distance = self.config.page_cache.prefetch_distance as i64;

        // Prefetch pages before and after
        for offset in -distance..=distance {
            if offset == 0 {
                continue; // Skip the current page
            }

            let prefetch_page_id = if offset > 0 {
                page_id.saturating_add(offset as u64)
            } else {
                page_id.saturating_sub((-offset) as u64)
            };

            // Try to prefetch (ignore errors)
            let _ = self.page_cache.get_page(prefetch_page_id);
        }
    }

    /// Update cache statistics if needed
    fn update_stats_if_needed(&mut self) {
        if self.last_stats_update.elapsed() >= self.config.global.stats_interval {
            self.update_stats();
            self.last_stats_update = Instant::now();
        }
    }

    /// Force update of all cache statistics
    pub fn update_stats(&mut self) {
        // Update page cache stats
        let page_stats = self.page_cache.stats();
        self.stats
            .update_size(CacheLayer::Page, page_stats.cache_size);
        self.stats
            .update_memory(CacheLayer::Page, page_stats.cache_size * 8192); // Rough estimate

        // Update object cache stats
        let obj_memory = self.object_cache.memory_usage();
        self.stats
            .update_size(CacheLayer::Object, self.object_cache.size());
        self.stats.update_memory(CacheLayer::Object, obj_memory);

        // Update query cache stats
        self.stats
            .update_size(CacheLayer::Query, self.query_cache.size());
        self.stats
            .update_memory(CacheLayer::Query, self.query_cache.memory_usage());

        // Update index cache stats
        self.stats
            .update_size(CacheLayer::Index, self.index_cache.size());
        self.stats
            .update_memory(CacheLayer::Index, self.index_cache.memory_usage());

        // Update relationship index stats
        let rel_stats = self.relationship_index.stats();
        self.stats.update_size(
            CacheLayer::Relationship,
            rel_stats.total_relationships as usize,
        );
        self.stats
            .update_memory(CacheLayer::Relationship, rel_stats.memory_usage);

        // Calculate hit rates
        for &layer in &[
            CacheLayer::Page,
            CacheLayer::Object,
            CacheLayer::Query,
            CacheLayer::Index,
            CacheLayer::Relationship,
        ] {
            self.stats.calculate_hit_rate(layer);
        }
    }

    /// Get property index manager for WHERE clause optimization
    pub fn property_index_manager(&self) -> &PropertyIndexManager {
        &self.property_index_manager
    }

    /// Intelligent cache eviction based on access patterns
    ///
    /// This method implements a hybrid LFU/LRU eviction policy that considers
    /// both frequency of access and recency.
    ///
    /// Note: This is a placeholder implementation. Full implementation would require
    /// integration with each cache layer's internal structures.
    pub fn intelligent_evict(&mut self) -> crate::Result<()> {
        // For now, this is a no-op. In production, this would:
        // 1. Calculate total memory usage
        // 2. Evict least valuable items if over threshold
        // 3. Track eviction statistics
        Ok(())
    }

    /// Warm up cache based on access patterns
    ///
    /// This method analyzes recent access patterns and warms up frequently
    /// accessed items across all cache layers.
    pub fn warm_cache(&mut self) -> crate::Result<()> {
        if !self.config.global.warming.enable_auto_warming {
            return Ok(());
        }

        let start_time = std::time::Instant::now();
        let _max_warm_time =
            std::time::Duration::from_secs(self.config.global.warming.max_warm_time_secs);

        tracing::info!("Starting cache warming...");

        // Warm page cache - prefetch frequently accessed pages
        let page_warm_count = self.warm_page_cache()?;
        tracing::info!("Warmed {} pages", page_warm_count);

        // Warm object cache - preload frequently accessed objects
        let object_warm_count = self.warm_object_cache()?;
        tracing::info!("Warmed {} objects", object_warm_count);

        // Warm index cache - preload frequently used indexes
        let index_warm_count = self.warm_index_cache()?;
        tracing::info!("Warmed {} index entries", index_warm_count);

        // Warm query cache - preload frequent query patterns
        let query_warm_count = self.warm_query_cache()?;
        tracing::info!("Warmed {} query patterns", query_warm_count);

        let elapsed = start_time.elapsed();
        tracing::info!("Cache warming completed in {:.2}s", elapsed.as_secs_f64());

        Ok(())
    }

    /// Warm page cache by prefetching frequently accessed pages
    fn warm_page_cache(&mut self) -> crate::Result<usize> {
        let mut warmed = 0;
        let max_items = self.config.global.warming.max_warm_items;

        // Prefetch some common pages to improve startup performance
        for page_id in 0..max_items.min(50) {
            if self.page_cache.get_page(page_id as u64).is_ok() {
                warmed += 1;
            }
        }

        Ok(warmed)
    }

    /// Warm object cache by preloading frequently accessed objects
    fn warm_object_cache(&mut self) -> crate::Result<usize> {
        // Placeholder implementation - would preload common objects in production
        Ok(0)
    }

    /// Warm index cache by preloading frequently used indexes
    fn warm_index_cache(&mut self) -> crate::Result<usize> {
        let mut warmed = 0;
        let max_items = self.config.global.warming.max_warm_items;

        // Warm label indexes
        for label_id in 0..max_items.min(20) {
            let key = IndexKey::Label(label_id as u32);
            if self.index_cache.get(&key).is_some() {
                warmed += 1;
            }
        }

        Ok(warmed)
    }

    /// Warm query cache by preloading frequent query patterns
    fn warm_query_cache(&mut self) -> crate::Result<usize> {
        // Placeholder implementation - would preload common query patterns in production
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_cache_config_defaults() {
        let config = CacheConfig::default();
        assert_eq!(config.page_cache.max_pages, 1024);
        assert_eq!(config.object_cache.max_memory, 50 * 1024 * 1024);
        assert_eq!(config.query_cache.max_plans, 1000);
    }

    #[test]
    fn test_multi_layer_cache_creation() {
        let config = CacheConfig::default();
        let cache = MultiLayerCache::new(config);
        assert!(cache.is_ok());
    }

    #[test]
    fn test_cache_operations() {
        let config = CacheConfig::default();
        let mut cache = MultiLayerCache::new(config).unwrap();

        // Test page cache
        let page = cache.get_page(42);
        assert!(page.is_ok());

        // Test object cache
        let obj_key = ObjectKey::Node(1);
        let obj_data = serde_json::json!({"name": "test"});

        cache.put_object(obj_key.clone(), obj_data.clone());
        let retrieved = cache.get_object(&obj_key);
        assert_eq!(retrieved, Some(obj_data));

        // Test query cache
        let query_hash = "SELECT * FROM test";
        let result = ResultSet::default();

        cache.put_query_result(
            query_hash.to_string(),
            result.clone(),
            Duration::from_millis(50),
        );
        let cached = cache.get_query_result(query_hash);
        assert!(cached.is_some());
    }

    #[test]
    fn test_cache_stats() {
        let config = CacheConfig::default();
        let mut cache = MultiLayerCache::new(config).unwrap();

        // Perform some operations to generate stats
        let _ = cache.get_page(1);
        let _ = cache.get_page(1); // Hit

        let obj_key = ObjectKey::Node(1);
        let obj_data = serde_json::json!({"test": true});
        cache.put_object(obj_key.clone(), obj_data);
        let _ = cache.get_object(&obj_key); // Hit
        let _ = cache.get_object(&ObjectKey::Node(2)); // Miss

        // Force stats update
        cache.update_stats();

        // Check stats
        let stats = cache.stats();
        assert!(stats.total_operations() > 0);

        // Check hit rates are calculated
        assert!(stats.hit_rates.contains_key(&CacheLayer::Page));
        assert!(stats.hit_rates.contains_key(&CacheLayer::Object));
    }
}
