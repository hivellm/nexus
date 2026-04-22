//! Cache optimization utilities
//!
//! Provides tools for monitoring, analyzing, and optimizing cache performance
//! including hit rates, eviction policies, and memory usage.

use crate::performance::{
    Effort, Impact, OptimizationRecommendation, OptimizationResult, Priority,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Cache optimization utilities
pub struct CacheOptimizer {
    cache_metrics: RwLock<std::collections::HashMap<String, CacheMetrics>>,
    optimization_config: CacheConfig,
    eviction_policies: std::collections::HashMap<String, EvictionPolicy>,
}

impl CacheOptimizer {
    /// Create a new cache optimizer
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            cache_metrics: RwLock::new(std::collections::HashMap::new()),
            optimization_config: CacheConfig::default(),
            eviction_policies: std::collections::HashMap::new(),
        })
    }

    /// Run cache optimization
    pub async fn optimize(&mut self) -> Result<OptimizationResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let before_hit_rate = self.get_overall_hit_rate().await;

        // Perform cache optimization steps
        self.optimize_cache_sizes().await?;
        self.optimize_eviction_policies().await?;
        self.optimize_preloading().await?;
        self.optimize_memory_allocation().await?;

        let after_hit_rate = self.get_overall_hit_rate().await;
        let improvement = if before_hit_rate > 0.0 {
            ((after_hit_rate - before_hit_rate) / before_hit_rate) * 100.0
        } else {
            0.0
        };

        let recommendations = self.generate_cache_recommendations().await;

        Ok(OptimizationResult {
            name: "Cache Optimization".to_string(),
            before_metric: before_hit_rate,
            after_metric: after_hit_rate,
            improvement_percent: improvement,
            duration: start_time.elapsed(),
            recommendations,
        })
    }

    /// Record cache access
    pub async fn record_access(&self, cache_name: &str, _key: &str, hit: bool) {
        let mut metrics = self.cache_metrics.write().await;
        let cache_metrics = metrics
            .entry(cache_name.to_string())
            .or_insert_with(CacheMetrics::new);

        if hit {
            cache_metrics.hits += 1;
        } else {
            cache_metrics.misses += 1;
        }

        cache_metrics.last_access = Instant::now();
        cache_metrics.access_count += 1;
    }

    /// Record cache eviction
    pub async fn record_eviction(&self, cache_name: &str, reason: EvictionReason) {
        let mut metrics = self.cache_metrics.write().await;
        if let Some(cache_metrics) = metrics.get_mut(cache_name) {
            cache_metrics.evictions += 1;
            cache_metrics
                .eviction_reasons
                .entry(reason)
                .and_modify(|e| *e += 1)
                .or_insert(1);
        }
    }

    /// Get cache metrics for a specific cache
    pub async fn get_cache_metrics(&self, cache_name: &str) -> Option<CacheMetrics> {
        let metrics = self.cache_metrics.read().await;
        metrics.get(cache_name).cloned()
    }

    /// Get overall cache hit rate
    pub async fn get_overall_hit_rate(&self) -> f64 {
        let metrics = self.cache_metrics.read().await;
        let mut total_hits = 0;
        let mut total_misses = 0;

        for cache_metrics in metrics.values() {
            total_hits += cache_metrics.hits;
            total_misses += cache_metrics.misses;
        }

        if total_hits + total_misses == 0 {
            0.0
        } else {
            total_hits as f64 / (total_hits + total_misses) as f64
        }
    }

    /// Get cache performance statistics
    pub async fn get_performance_statistics(&self) -> CachePerformanceStatistics {
        let metrics = self.cache_metrics.read().await;

        let mut total_hits = 0;
        let mut total_misses = 0;
        let mut total_evictions = 0;
        let mut total_accesses = 0;
        let mut cache_count = 0;

        let mut hit_rates = Vec::new();
        let mut eviction_rates = Vec::new();

        for cache_metrics in metrics.values() {
            total_hits += cache_metrics.hits;
            total_misses += cache_metrics.misses;
            total_evictions += cache_metrics.evictions;
            total_accesses += cache_metrics.access_count;
            cache_count += 1;

            let hit_rate = if cache_metrics.hits + cache_metrics.misses > 0 {
                cache_metrics.hits as f64 / (cache_metrics.hits + cache_metrics.misses) as f64
            } else {
                0.0
            };
            hit_rates.push(hit_rate);

            let eviction_rate = if cache_metrics.access_count > 0 {
                cache_metrics.evictions as f64 / cache_metrics.access_count as f64
            } else {
                0.0
            };
            eviction_rates.push(eviction_rate);
        }

        let overall_hit_rate = if total_hits + total_misses > 0 {
            total_hits as f64 / (total_hits + total_misses) as f64
        } else {
            0.0
        };

        let avg_hit_rate = if !hit_rates.is_empty() {
            hit_rates.iter().sum::<f64>() / hit_rates.len() as f64
        } else {
            0.0
        };

        let avg_eviction_rate = if !eviction_rates.is_empty() {
            eviction_rates.iter().sum::<f64>() / eviction_rates.len() as f64
        } else {
            0.0
        };

        CachePerformanceStatistics {
            cache_count,
            total_accesses,
            total_hits,
            total_misses,
            total_evictions,
            overall_hit_rate,
            avg_hit_rate,
            avg_eviction_rate,
            min_hit_rate: hit_rates.iter().cloned().fold(f64::INFINITY, f64::min),
            max_hit_rate: hit_rates.iter().cloned().fold(0.0, f64::max),
        }
    }

    /// Get cache optimization recommendations
    pub async fn get_optimization_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();
        let stats = self.get_performance_statistics().await;

        // Low hit rate recommendations
        if stats.overall_hit_rate < self.optimization_config.min_hit_rate {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Hit Rate".to_string(),
                priority: Priority::High,
                description: format!(
                    "Low overall hit rate: {:.1}% (target: {:.1}%)",
                    stats.overall_hit_rate * 100.0,
                    self.optimization_config.min_hit_rate * 100.0
                ),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Increase cache sizes, optimize eviction policies, or improve data locality"
                        .to_string(),
            });
        }

        // High eviction rate recommendations
        if stats.avg_eviction_rate > self.optimization_config.max_eviction_rate {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Evictions".to_string(),
                priority: Priority::Medium,
                description: format!(
                    "High eviction rate: {:.1}%",
                    stats.avg_eviction_rate * 100.0
                ),
                impact: Impact::Medium,
                effort: Effort::Low,
                implementation: "Increase cache sizes or optimize eviction policies".to_string(),
            });
        }

        // Cache size recommendations
        if stats.cache_count > 0 {
            let avg_accesses_per_cache = stats.total_accesses / stats.cache_count as u64;
            if avg_accesses_per_cache > self.optimization_config.max_accesses_per_cache {
                recommendations.push(OptimizationRecommendation {
                    category: "Cache Load".to_string(),
                    priority: Priority::Medium,
                    description: format!(
                        "High cache load: {} accesses per cache",
                        avg_accesses_per_cache
                    ),
                    impact: Impact::Medium,
                    effort: Effort::High,
                    implementation: "Consider cache partitioning or load balancing".to_string(),
                });
            }
        }

        // Hit rate variance recommendations
        let hit_rate_variance = stats.max_hit_rate - stats.min_hit_rate;
        if hit_rate_variance > 0.3 {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Consistency".to_string(),
                priority: Priority::Low,
                description: format!("High hit rate variance: {:.1}%", hit_rate_variance * 100.0),
                impact: Impact::Low,
                effort: Effort::Medium,
                implementation: "Standardize cache configurations across all caches".to_string(),
            });
        }

        recommendations
    }

    /// Set cache optimization configuration
    pub fn set_config(&mut self, config: CacheConfig) {
        self.optimization_config = config;
    }

    /// Set eviction policy for a cache
    pub fn set_eviction_policy(&mut self, cache_name: String, policy: EvictionPolicy) {
        self.eviction_policies.insert(cache_name, policy);
    }

    /// Optimize cache sizes
    async fn optimize_cache_sizes(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement cache size optimization
        // This could include:
        // - Dynamic cache sizing based on hit rates
        // - Memory-based cache sizing
        // - Load-based cache sizing

        tokio::time::sleep(Duration::from_millis(15)).await; // Simulate work
        Ok(())
    }

    /// Optimize eviction policies
    async fn optimize_eviction_policies(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement eviction policy optimization
        // This could include:
        // - LRU vs LFU vs FIFO selection
        // - Adaptive eviction policies
        // - Time-based eviction tuning

        tokio::time::sleep(Duration::from_millis(10)).await; // Simulate work
        Ok(())
    }

    /// Optimize cache preloading
    async fn optimize_preloading(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement cache preloading optimization
        // This could include:
        // - Predictive preloading
        // - Access pattern analysis
        // - Preload scheduling optimization

        tokio::time::sleep(Duration::from_millis(8)).await; // Simulate work
        Ok(())
    }

    /// Optimize memory allocation for caches
    async fn optimize_memory_allocation(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement cache memory allocation optimization
        // This could include:
        // - Memory pool allocation
        // - Memory mapping optimization
        // - Cache memory partitioning

        tokio::time::sleep(Duration::from_millis(12)).await; // Simulate work
        Ok(())
    }

    /// Generate cache optimization recommendations
    async fn generate_cache_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        let stats = self.get_performance_statistics().await;

        if stats.overall_hit_rate < self.optimization_config.min_hit_rate {
            recommendations.push("Increase cache sizes to improve hit rates".to_string());
            recommendations
                .push("Consider implementing more sophisticated eviction policies".to_string());
        }

        if stats.avg_eviction_rate > self.optimization_config.max_eviction_rate {
            recommendations
                .push("Optimize eviction policies to reduce unnecessary evictions".to_string());
        }

        if stats.cache_count > 10 {
            recommendations
                .push("Consider consolidating caches to reduce management overhead".to_string());
        }

        recommendations
    }
}

impl Default for CacheOptimizer {
    fn default() -> Self {
        Self {
            cache_metrics: RwLock::new(std::collections::HashMap::new()),
            optimization_config: CacheConfig::default(),
            eviction_policies: std::collections::HashMap::new(),
        }
    }
}

/// Cache configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    pub min_hit_rate: f64,
    pub max_eviction_rate: f64,
    pub max_accesses_per_cache: u64,
    pub default_cache_size: usize,
    pub max_cache_size: usize,
    pub preload_enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            min_hit_rate: 0.8,      // 80%
            max_eviction_rate: 0.1, // 10%
            max_accesses_per_cache: 10000,
            default_cache_size: 1000,
            max_cache_size: 100000,
            preload_enabled: true,
        }
    }
}

/// Cache metrics
#[derive(Debug, Clone)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub access_count: u64,
    pub last_access: Instant,
    pub eviction_reasons: std::collections::HashMap<EvictionReason, u64>,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self {
            hits: 0,
            misses: 0,
            evictions: 0,
            access_count: 0,
            last_access: Instant::now(),
            eviction_reasons: std::collections::HashMap::new(),
        }
    }

    pub fn hit_rate(&self) -> f64 {
        if self.hits + self.misses == 0 {
            0.0
        } else {
            self.hits as f64 / (self.hits + self.misses) as f64
        }
    }

    pub fn eviction_rate(&self) -> f64 {
        if self.access_count == 0 {
            0.0
        } else {
            self.evictions as f64 / self.access_count as f64
        }
    }
}

impl Default for CacheMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Eviction policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionPolicy {
    Lru,  // Least Recently Used
    Lfu,  // Least Frequently Used
    Fifo, // First In, First Out
    Random,
    Ttl, // Time To Live
}

/// Eviction reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum EvictionReason {
    SizeLimit,
    TtlExpired,
    MemoryPressure,
    Manual,
    PolicyChange,
}

/// Cache performance statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePerformanceStatistics {
    pub cache_count: usize,
    pub total_accesses: u64,
    pub total_hits: u64,
    pub total_misses: u64,
    pub total_evictions: u64,
    pub overall_hit_rate: f64,
    pub avg_hit_rate: f64,
    pub avg_eviction_rate: f64,
    pub min_hit_rate: f64,
    pub max_hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_optimizer_creation() {
        let optimizer = CacheOptimizer::new().unwrap();
        assert_eq!(optimizer.optimization_config.min_hit_rate, 0.8);
    }

    #[tokio::test]
    async fn test_cache_optimization() {
        let mut optimizer = CacheOptimizer::new().unwrap();
        let result = optimizer.optimize().await.unwrap();

        assert_eq!(result.name, "Cache Optimization");
        assert!(result.duration > Duration::from_millis(0));
    }

    #[tokio::test]
    async fn test_cache_access_recording() {
        let optimizer = CacheOptimizer::new().unwrap();

        optimizer.record_access("test_cache", "key1", true).await;
        optimizer.record_access("test_cache", "key2", false).await;
        optimizer.record_access("test_cache", "key1", true).await;

        let metrics = optimizer.get_cache_metrics("test_cache").await.unwrap();
        assert_eq!(metrics.hits, 2);
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hit_rate(), 2.0 / 3.0);
    }

    #[tokio::test]
    async fn test_cache_eviction_recording() {
        let optimizer = CacheOptimizer::new().unwrap();

        // First record an access to create the cache metrics
        optimizer.record_access("test_cache", "key1", true).await;

        optimizer
            .record_eviction("test_cache", EvictionReason::SizeLimit)
            .await;
        optimizer
            .record_eviction("test_cache", EvictionReason::TtlExpired)
            .await;

        let metrics = optimizer.get_cache_metrics("test_cache").await.unwrap();
        assert_eq!(metrics.evictions, 2);
        assert_eq!(
            metrics.eviction_reasons.get(&EvictionReason::SizeLimit),
            Some(&1)
        );
        assert_eq!(
            metrics.eviction_reasons.get(&EvictionReason::TtlExpired),
            Some(&1)
        );
    }

    #[tokio::test]
    async fn test_performance_statistics() {
        let optimizer = CacheOptimizer::new().unwrap();

        // Record some accesses
        optimizer.record_access("cache1", "key1", true).await;
        optimizer.record_access("cache1", "key2", false).await;
        optimizer.record_access("cache2", "key3", true).await;

        let stats = optimizer.get_performance_statistics().await;
        assert_eq!(stats.cache_count, 2);
        assert_eq!(stats.total_hits, 2);
        assert_eq!(stats.total_misses, 1);
        assert_eq!(stats.overall_hit_rate, 2.0 / 3.0);
    }

    #[tokio::test]
    async fn test_optimization_recommendations() {
        let optimizer = CacheOptimizer::new().unwrap();
        let recommendations = optimizer.get_optimization_recommendations().await;

        // Should have some recommendations based on default thresholds
        assert!(!recommendations.is_empty());
    }
}
