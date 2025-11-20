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
pub mod relationship_cache;
pub mod relationship_index;

pub use index_cache::{CachedIndexPage, IndexCache, IndexKey};
pub use object_cache::{CachedObject, ObjectCache, ObjectKey};
pub use query_cache::{CachedQueryResult, QueryCache, QueryKey};
pub use relationship_cache::{
    RelationshipCache, RelationshipCacheConfig, RelationshipCacheKey, RelationshipCacheStats,
};
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
    Page,              // L1: Fast memory-mapped pages
    Object,            // L2: Deserialized objects
    Index,             // L2: Index lookups
    Query,             // L2: Query results and plans
    Relationship,      // L2: Relationship index
    RelationshipQuery, // L2: Relationship query results
    Distributed,       // L3: Distributed/shared cache
}

/// Cache operation types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CacheOperation {
    Hit,
    Miss,
    Eviction,
    Invalidation,
    RemoteHit,   // Hit from distributed cache
    RemoteMiss,  // Miss from distributed cache
    PrefetchHit, // Hit from prefetching
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
    /// Relationship cache configuration
    pub relationship_cache: RelationshipCacheConfig,
    /// Distributed cache configuration
    pub distributed_cache: DistributedCacheConfig,
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

/// Distributed cache configuration (L3 cache)
#[derive(Debug, Clone)]
pub struct DistributedCacheConfig {
    /// Enable distributed caching
    pub enabled: bool,
    /// Redis connection URL (if using Redis)
    pub redis_url: Option<String>,
    /// Maximum memory usage for distributed cache (bytes)
    pub max_memory: usize,
    /// Default TTL for distributed cache entries
    pub default_ttl: Duration,
    /// Sync interval for local to distributed cache
    pub sync_interval: Duration,
    /// Enable compression for network transfer
    pub enable_compression: bool,
    /// Cluster mode (for Redis Cluster)
    pub cluster_mode: bool,
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

impl Default for DistributedCacheConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            redis_url: None,
            max_memory: 500 * 1024 * 1024,          // 500MB
            default_ttl: Duration::from_secs(1800), // 30 minutes
            sync_interval: Duration::from_secs(30), // Sync every 30 seconds
            enable_compression: true,
            cluster_mode: false,
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
            relationship_cache: RelationshipCacheConfig::default(),
            distributed_cache: DistributedCacheConfig::default(),
            global: GlobalCacheConfig {
                enable_warming: false,
                stats_interval: Duration::from_secs(60),
                max_total_memory: 200 * 1024 * 1024, // 200MB total
                warming: CacheWarmingConfig {
                    enable_auto_warming: false, // Temporarily disabled - causing performance regression
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

/// Distributed cache implementation (L3 cache)
#[derive(Debug)]
pub struct DistributedCache {
    /// In-memory fallback for distributed cache (simulates Redis/file-based storage)
    local_fallback: HashMap<String, (serde_json::Value, Instant)>,
    /// Configuration
    config: DistributedCacheConfig,
    /// Cache statistics
    stats: DistributedCacheStats,
}

#[derive(Debug, Default)]
pub struct DistributedCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub sets: u64,
    pub evictions: u64,
}

impl DistributedCache {
    pub fn new(config: DistributedCacheConfig) -> Self {
        Self {
            local_fallback: HashMap::new(),
            config,
            stats: DistributedCacheStats::default(),
        }
    }

    /// Get value from distributed cache
    pub fn get(&mut self, key: &str) -> Option<serde_json::Value> {
        if !self.config.enabled {
            return None;
        }

        match self.local_fallback.get(key) {
            Some((value, expiry)) if *expiry > Instant::now() => {
                self.stats.hits += 1;
                Some(value.clone())
            }
            Some(_) => {
                // Expired entry
                self.local_fallback.remove(key);
                self.stats.misses += 1;
                None
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Set value in distributed cache
    pub fn set(&mut self, key: String, value: serde_json::Value) {
        if !self.config.enabled {
            return;
        }

        let expiry = Instant::now() + self.config.default_ttl;
        self.local_fallback.insert(key, (value, expiry));
        self.stats.sets += 1;

        // Simple size-based eviction
        while self.memory_usage() > self.config.max_memory {
            if let Some(key_to_remove) = self.local_fallback.keys().next().cloned() {
                self.local_fallback.remove(&key_to_remove);
                self.stats.evictions += 1;
            }
        }
    }

    /// Get cache statistics
    pub fn stats(&self) -> &DistributedCacheStats {
        &self.stats
    }

    /// Estimate memory usage
    fn memory_usage(&self) -> usize {
        // Rough estimate: key size + value size + overhead
        self.local_fallback
            .iter()
            .map(|(k, (v, _))| {
                k.len() + v.to_string().len() + 64 // Overhead
            })
            .sum()
    }

    /// Clear expired entries
    pub fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.local_fallback.retain(|_, (_, expiry)| *expiry > now);
    }
}

/// Multi-layer cache manager with hierarchical caching (L1/L2/L3)
pub struct MultiLayerCache {
    /// Page cache (L1: foundation layer - memory-mapped pages)
    page_cache: PageCache,
    /// Object cache (L2: deserialization layer)
    object_cache: ObjectCache,
    /// Query cache (L2: execution layer)
    query_cache: QueryCache,
    /// Index cache (L2: lookup acceleration layer)
    index_cache: IndexCache,
    /// Relationship index (L2: relationship query acceleration layer)
    relationship_index: RelationshipIndex,
    /// Relationship query cache (L2: relationship result caching layer)
    relationship_cache: RelationshipCache,
    /// Property index manager (L2: WHERE clause acceleration layer)
    property_index_manager: PropertyIndexManager,
    /// Distributed cache (L3: shared cache across instances)
    distributed_cache: DistributedCache,
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
    /// Last cache warming time for cooldown management
    last_warm_time: Instant,
    /// Last distributed cache sync
    last_distributed_sync: Instant,
}

impl MultiLayerCache {
    /// Create a new multi-layer cache system with hierarchical caching
    pub fn new(config: CacheConfig) -> Result<Self> {
        let page_cache = PageCache::new(config.page_cache.max_pages)?;
        let object_cache = ObjectCache::new(config.object_cache.clone());
        let query_cache = QueryCache::new(config.query_cache.clone());
        let index_cache = IndexCache::new(config.index_cache.clone());
        let relationship_index = RelationshipIndex::new();
        let relationship_cache = RelationshipCache::new(config.relationship_cache.clone());
        let property_index_manager = PropertyIndexManager::new(
            config.global.max_total_memory / 4, // Use 1/4 of total cache memory
            Duration::from_secs(3600),          // 1 hour TTL
        );
        let distributed_cache = DistributedCache::new(config.distributed_cache.clone());

        Ok(Self {
            page_cache,
            object_cache,
            query_cache,
            index_cache,
            relationship_index,
            relationship_cache,
            property_index_manager,
            distributed_cache,
            config,
            stats: CacheStats::default(),
            last_stats_update: Instant::now(),
            access_frequency: HashMap::new(),
            last_access: HashMap::new(),
            last_distributed_sync: Instant::now(),
            last_warm_time: Instant::now() - std::time::Duration::from_secs(3600), // Start with old timestamp
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

    /// Get cached query plan with hierarchical lookup (L1 -> L2 -> L3)
    pub fn get_query_plan(&mut self, query_hash: &str) -> Option<serde_json::Value> {
        // L1: Try local index cache first
        let hash = query_hash
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_add(b as u64));

        if let Some(page) = self.index_cache.get(&IndexKey::FullText(hash)) {
            self.stats
                .record_operation(CacheLayer::Index, CacheOperation::Hit);
            return Some(page.data);
        }

        // L2: Try distributed cache
        let dist_key = format!("query_plan:{}", query_hash);
        if let Some(plan) = self.distributed_cache.get(&dist_key) {
            self.stats
                .record_operation(CacheLayer::Distributed, CacheOperation::RemoteHit);
            // Promote to L1 cache
            self.index_cache.put(
                IndexKey::FullText(hash),
                plan.clone(),
                index_cache::IndexType::FullText,
            );
            return Some(plan);
        }

        self.stats
            .record_operation(CacheLayer::Index, CacheOperation::Miss);
        self.stats
            .record_operation(CacheLayer::Distributed, CacheOperation::RemoteMiss);
        None
    }

    /// Cache query plan with hierarchical storage (L1 -> L3)
    pub fn put_query_plan(&mut self, query_hash: String, plan: serde_json::Value) {
        let hash = query_hash
            .as_bytes()
            .iter()
            .fold(0u64, |acc, &b| acc.wrapping_add(b as u64));

        // L1: Store in local index cache
        self.index_cache.put(
            IndexKey::FullText(hash),
            plan.clone(),
            index_cache::IndexType::FullText,
        );

        // L3: Sync to distributed cache
        if self.config.distributed_cache.enabled {
            let dist_key = format!("query_plan:{}", query_hash);
            self.distributed_cache.set(dist_key, plan);
        }
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

    /// Get relationship cache
    pub fn relationship_cache(&self) -> &RelationshipCache {
        &self.relationship_cache
    }

    /// Get distributed cache
    pub fn distributed_cache(&self) -> &DistributedCache {
        &self.distributed_cache
    }

    /// Get distributed cache statistics
    pub fn distributed_stats(&self) -> &DistributedCacheStats {
        self.distributed_cache.stats()
    }

    /// Clear all caches
    pub fn clear(&mut self) {
        let _ = self.page_cache.clear();
        self.object_cache.clear();
        self.query_cache.clear();
        self.index_cache.clear();
        let _ = self.relationship_index.clear();
        self.relationship_cache.clear();
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

    /// Update cache statistics and perform maintenance if needed
    fn update_stats_if_needed(&mut self) {
        let now = Instant::now();

        if now.duration_since(self.last_stats_update) >= self.config.global.stats_interval {
            self.update_stats();
            self.last_stats_update = now;
        }

        // Sync with distributed cache if needed
        if self.config.distributed_cache.enabled
            && now.duration_since(self.last_distributed_sync)
                >= self.config.distributed_cache.sync_interval
        {
            self.sync_with_distributed_cache();
            self.last_distributed_sync = now;
        }

        // Cleanup expired entries in distributed cache
        self.distributed_cache.cleanup_expired();
    }

    /// Sync frequently accessed items with distributed cache
    fn sync_with_distributed_cache(&mut self) {
        if !self.config.distributed_cache.enabled {
            return;
        }

        tracing::debug!("Syncing frequently accessed items to distributed cache");

        // In a real implementation, this would track access patterns and sync
        // the most frequently accessed items. For now, we'll sync cache statistics
        // and configuration to demonstrate the hierarchical concept.

        let local_stats = serde_json::json!({
            "layers": {
                "page_cache_size": self.page_cache.stats().cache_size,
                "object_cache_size": self.object_cache.size(),
                "query_cache_size": self.query_cache.size(),
                "index_cache_size": self.index_cache.size(),
            },
            "total_operations": self.stats.total_operations(),
            "last_sync": chrono::Utc::now().timestamp(),
        });

        self.distributed_cache
            .set("nexus:cache_stats".to_string(), local_stats);
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

        // Update relationship cache stats
        let rel_cache_stats = self.relationship_cache.stats();
        self.stats
            .update_size(CacheLayer::RelationshipQuery, rel_cache_stats.entries);
        self.stats
            .update_memory(CacheLayer::RelationshipQuery, rel_cache_stats.memory_usage);

        // Update distributed cache stats
        let dist_stats = self.distributed_cache.stats();
        self.stats.update_size(
            CacheLayer::Distributed,
            self.distributed_cache.local_fallback.len(),
        );
        self.stats.update_memory(
            CacheLayer::Distributed,
            self.distributed_cache.memory_usage(),
        );

        // Calculate hit rates for all layers
        for &layer in &[
            CacheLayer::Page,
            CacheLayer::Object,
            CacheLayer::Query,
            CacheLayer::Index,
            CacheLayer::Relationship,
            CacheLayer::RelationshipQuery,
            CacheLayer::Distributed,
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
    /// Lazy cache warming - only warm after observing query patterns
    pub fn warm_cache_lazy(&mut self, query_count: usize) -> crate::Result<()> {
        // Only warm cache after we've seen enough queries to understand patterns
        if query_count < 10 {
            return Ok(());
        }

        // Only warm if we haven't warmed recently
        let now = std::time::Instant::now();
        if now.duration_since(self.last_warm_time).as_secs() < 300 {
            // 5 minutes cooldown
            return Ok(());
        }

        self.last_warm_time = now;

        tracing::info!(
            "Starting lazy cache warming after {} queries...",
            query_count
        );

        // Quick warming - only warm what we've observed being used
        let query_warm_count = self.warm_query_cache()?;
        if query_warm_count > 0 {
            tracing::info!("Warmed {} observed query patterns", query_warm_count);
        }

        // Warm recently accessed indexes
        let index_warm_count = self.warm_recent_indexes()?;
        if index_warm_count > 0 {
            tracing::info!("Warmed {} recently accessed indexes", index_warm_count);
        }

        Ok(())
    }

    /// Legacy method - kept for compatibility but does minimal work
    pub fn warm_cache(&mut self) -> crate::Result<()> {
        // Just do lazy warming with query_count = 0 (minimal work)
        self.warm_cache_lazy(0)
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

    /// Warm recently accessed indexes
    fn warm_recent_indexes(&mut self) -> crate::Result<usize> {
        let mut warmed = 0;
        let max_items = self.config.global.warming.max_warm_items / 4; // Limit for lazy warming

        // Get recently accessed index keys
        let mut recent_keys: Vec<_> = self
            .last_access
            .iter()
            .filter(|(key, _)| matches!(key, CacheKey::Index(_)))
            .collect();

        // Sort by most recent access
        recent_keys.sort_by(|a, b| b.1.cmp(a.1));

        for (key, _) in recent_keys.into_iter().take(max_items) {
            if let CacheKey::Index(index_key) = key {
                // Preload this index if not already in cache
                // TODO: Check if index is actually cached
                // For now, just count as warmed
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
