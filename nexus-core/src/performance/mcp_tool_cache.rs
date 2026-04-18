//! MCP Tool Result Cache
//!
//! This module provides caching for MCP tool results to improve performance
//! for idempotent operations like graph correlation analysis.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache entry for MCP tool results
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    /// Cached result
    result: serde_json::Value,
    /// Timestamp when entry was created
    created_at: u64,
    /// Time-to-live in seconds
    ttl_seconds: u64,
    /// Number of times this entry was accessed
    access_count: u64,
    /// Last access timestamp
    last_accessed: u64,
}

impl CacheEntry {
    /// Check if entry is expired
    fn is_expired(&self, current_time: u64) -> bool {
        current_time >= self.created_at + self.ttl_seconds
    }

    /// Record an access
    fn record_access(&mut self, current_time: u64) {
        self.access_count += 1;
        self.last_accessed = current_time;
    }
}

/// Cache key for MCP tool calls
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheKey {
    /// Tool name
    tool_name: String,
    /// Normalized arguments (sorted keys for consistency)
    arguments: serde_json::Value,
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.tool_name.hash(state);
        // Hash JSON value by serializing it
        if let Ok(json_str) = serde_json::to_string(&self.arguments) {
            json_str.hash(state);
        }
    }
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.tool_name == other.tool_name
            && serde_json::to_string(&self.arguments).unwrap_or_default()
                == serde_json::to_string(&other.arguments).unwrap_or_default()
    }
}

impl Eq for CacheKey {}

/// MCP tool result cache
pub struct McpToolCache {
    /// Cache entries
    entries: Arc<RwLock<HashMap<CacheKey, CacheEntry>>>,
    /// Default TTL in seconds
    default_ttl_seconds: u64,
    /// Maximum cache size
    max_size: usize,
    /// Cache statistics
    stats: Arc<RwLock<CacheStatistics>>,
}

/// Cache statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatistics {
    /// Total cache hits
    pub hits: u64,
    /// Total cache misses
    pub misses: u64,
    /// Total evictions
    pub evictions: u64,
    /// Current cache size
    pub current_size: usize,
    /// Maximum cache size
    pub max_size: usize,
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

impl McpToolCache {
    /// Create a new MCP tool cache
    pub fn new(default_ttl_seconds: u64, max_size: usize) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            default_ttl_seconds,
            max_size,
            stats: Arc::new(RwLock::new(CacheStatistics {
                hits: 0,
                misses: 0,
                evictions: 0,
                current_size: 0,
                max_size,
            })),
        }
    }

    /// Normalize arguments for consistent caching
    fn normalize_arguments(args: &serde_json::Value) -> serde_json::Value {
        // Sort object keys for consistency
        if let Some(obj) = args.as_object() {
            let mut sorted: Vec<_> = obj.iter().collect();
            sorted.sort_by_key(|(k, _)| *k);
            let normalized: serde_json::Map<String, serde_json::Value> = sorted
                .into_iter()
                .map(|(k, v)| (k.clone(), Self::normalize_arguments(v)))
                .collect();
            serde_json::Value::Object(normalized)
        } else if let Some(arr) = args.as_array() {
            serde_json::Value::Array(arr.iter().map(Self::normalize_arguments).collect())
        } else {
            args.clone()
        }
    }

    /// Create cache key from tool name and arguments
    fn create_key(tool_name: &str, arguments: &serde_json::Value) -> CacheKey {
        CacheKey {
            tool_name: tool_name.to_string(),
            arguments: Self::normalize_arguments(arguments),
        }
    }

    /// Get current timestamp
    fn current_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    /// Get cached result if available
    pub fn get(&self, tool_name: &str, arguments: &serde_json::Value) -> Option<serde_json::Value> {
        let key = Self::create_key(tool_name, arguments);
        let current_time = Self::current_timestamp();

        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        if let Some(entry) = entries.get_mut(&key) {
            if entry.is_expired(current_time) {
                // Entry expired, remove it
                entries.remove(&key);
                stats.current_size = entries.len();
                stats.misses += 1;
                return None;
            }

            // Entry found and valid
            entry.record_access(current_time);
            stats.hits += 1;
            Some(entry.result.clone())
        } else {
            stats.misses += 1;
            None
        }
    }

    /// Store result in cache
    pub fn put(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        result: serde_json::Value,
        ttl_seconds: Option<u64>,
    ) {
        let key = Self::create_key(tool_name, arguments);
        let ttl = ttl_seconds.unwrap_or(self.default_ttl_seconds);
        let current_time = Self::current_timestamp();

        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        // Check if cache is full and needs eviction
        if entries.len() >= self.max_size && !entries.contains_key(&key) {
            // Evict least recently used entry
            self.evict_lru(&mut entries, &mut stats);
        }

        let entry = CacheEntry {
            result,
            created_at: current_time,
            ttl_seconds: ttl,
            access_count: 0,
            last_accessed: current_time,
        };

        entries.insert(key, entry);
        stats.current_size = entries.len();
    }

    /// Evict least recently used entry
    fn evict_lru(&self, entries: &mut HashMap<CacheKey, CacheEntry>, stats: &mut CacheStatistics) {
        if entries.is_empty() {
            return;
        }

        let mut lru_key: Option<CacheKey> = None;
        let mut lru_time = u64::MAX;

        for (key, entry) in entries.iter() {
            if entry.last_accessed < lru_time {
                lru_time = entry.last_accessed;
                lru_key = Some(key.clone());
            }
        }

        if let Some(key) = lru_key {
            entries.remove(&key);
            stats.evictions += 1;
        }
    }

    /// Invalidate cache entry
    pub fn invalidate(&self, tool_name: &str, arguments: &serde_json::Value) {
        let key = Self::create_key(tool_name, arguments);
        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        if entries.remove(&key).is_some() {
            stats.current_size = entries.len();
        }
    }

    /// Invalidate all entries for a specific tool
    pub fn invalidate_tool(&self, tool_name: &str) {
        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        entries.retain(|key, _| key.tool_name != tool_name);
        stats.current_size = entries.len();
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        entries.clear();
        stats.current_size = 0;
        stats.hits = 0;
        stats.misses = 0;
        stats.evictions = 0;
    }

    /// Clean expired entries
    pub fn clean_expired(&self) -> usize {
        let current_time = Self::current_timestamp();
        let mut entries = self.entries.write().unwrap();
        let mut stats = self.stats.write().unwrap();

        let initial_size = entries.len();
        entries.retain(|_, entry| !entry.is_expired(current_time));
        let removed = initial_size - entries.len();
        stats.current_size = entries.len();

        removed
    }

    /// Get cache statistics
    pub fn get_statistics(&self) -> CacheStatistics {
        let stats = self.stats.read().unwrap();
        let entries = self.entries.read().unwrap();
        CacheStatistics {
            hits: stats.hits,
            misses: stats.misses,
            evictions: stats.evictions,
            current_size: entries.len(),
            max_size: stats.max_size,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_cache_put_and_get() {
        let cache = McpToolCache::new(3600, 100);
        let args = json!({"graph_type": "Call", "files": {}});
        let result = json!({"status": "success", "graph": {}});

        cache.put("graph_correlation_generate", &args, result.clone(), None);
        let cached = cache.get("graph_correlation_generate", &args);

        assert!(cached.is_some());
        assert_eq!(cached.unwrap(), result);
    }

    #[test]
    fn test_cache_miss() {
        let cache = McpToolCache::new(3600, 100);
        let args = json!({"graph_type": "Call"});

        let cached = cache.get("graph_correlation_generate", &args);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_expiration() {
        let cache = McpToolCache::new(1, 100); // 1 second TTL
        let args = json!({"graph_type": "Call"});
        let result = json!({"status": "success"});

        cache.put("graph_correlation_generate", &args, result.clone(), Some(1));

        // Wait for expiration (simulate by using very short TTL)
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let cached = cache.get("graph_correlation_generate", &args);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_invalidation() {
        let cache = McpToolCache::new(3600, 100);
        let args = json!({"graph_type": "Call"});
        let result = json!({"status": "success"});

        cache.put("graph_correlation_generate", &args, result.clone(), None);
        cache.invalidate("graph_correlation_generate", &args);

        let cached = cache.get("graph_correlation_generate", &args);
        assert!(cached.is_none());
    }

    #[test]
    fn test_cache_statistics() {
        let cache = McpToolCache::new(3600, 100);
        let args = json!({"graph_type": "Call"});
        let result = json!({"status": "success"});

        // Miss
        cache.get("graph_correlation_generate", &args);
        // Put
        cache.put("graph_correlation_generate", &args, result, None);
        // Hit
        cache.get("graph_correlation_generate", &args);

        let stats = cache.get_statistics();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!(stats.hit_rate() > 0.0);
    }

    #[test]
    fn test_cache_eviction() {
        let cache = McpToolCache::new(3600, 2); // Max size 2
        let args1 = json!({"graph_type": "Call"});
        let args2 = json!({"graph_type": "Dependency"});
        let args3 = json!({"graph_type": "DataFlow"});

        cache.put("graph_correlation_generate", &args1, json!({}), None);
        cache.put("graph_correlation_generate", &args2, json!({}), None);
        cache.put("graph_correlation_generate", &args3, json!({}), None);

        let stats = cache.get_statistics();
        assert_eq!(stats.current_size, 2);
        assert!(stats.evictions > 0);
    }

    #[test]
    fn test_cache_clean_expired() {
        let cache = McpToolCache::new(1, 100);
        let args1 = json!({"graph_type": "Call"});
        let args2 = json!({"graph_type": "Dependency"});

        cache.put("graph_correlation_generate", &args1, json!({}), Some(1));
        cache.put("graph_correlation_generate", &args2, json!({}), Some(3600));

        std::thread::sleep(std::time::Duration::from_millis(1100));

        let removed = cache.clean_expired();
        assert_eq!(removed, 1);

        let stats = cache.get_statistics();
        assert_eq!(stats.current_size, 1);
    }
}
