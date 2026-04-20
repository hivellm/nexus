//! Performance configuration utilities
//!
//! Provides configuration management for performance optimization settings
//! including system tuning, cache configuration, and monitoring parameters.

use crate::performance::cache::CacheConfig;
use crate::performance::memory::MemoryConfig;
use crate::performance::monitoring::MonitoringConfig;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Comprehensive performance configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceConfig {
    pub memory: MemoryConfig,
    pub cache: CacheConfig,
    pub monitoring: MonitoringConfig,
    pub query: QueryConfig,
    pub system: SystemConfig,
    pub optimization: OptimizationConfig,
}

impl PerformanceConfig {
    /// Create a new performance configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a high-performance configuration
    pub fn high_performance() -> Self {
        Self {
            memory: MemoryConfig {
                max_memory_threshold: 4 * 1024 * 1024 * 1024, // 4GB
                max_heap_memory: 2 * 1024 * 1024 * 1024,      // 2GB
                max_cache_memory: 1024 * 1024 * 1024,         // 1GB
                gc_threshold: 0.9,
                memory_pressure_threshold: 0.9,
            },
            cache: CacheConfig {
                min_hit_rate: 0.9,       // 90%
                max_eviction_rate: 0.05, // 5%
                max_accesses_per_cache: 50000,
                default_cache_size: 10000,
                max_cache_size: 1000000,
                preload_enabled: true,
            },
            monitoring: MonitoringConfig {
                cpu_pressure_threshold: 0.9,      // 90%
                memory_pressure_threshold: 0.9,   // 90%
                disk_pressure_threshold: 0.95,    // 95%
                overall_pressure_threshold: 0.85, // 85%
                monitoring_interval: Duration::from_millis(500),
                max_history_size: 2000,
            },
            query: QueryConfig {
                slow_query_threshold_ms: 50,
                max_query_timeout_ms: 30000,
                query_cache_size: 1000,
                query_cache_ttl_seconds: 3600,
                parallel_query_limit: 100,
                query_optimization_enabled: true,
            },
            system: SystemConfig {
                max_workers: 16,
                worker_threads: 8,
                io_threads: 4,
                max_connections: 10000,
                connection_timeout_ms: 5000,
                keep_alive_timeout_ms: 30000,
                tcp_nodelay: true,
                tcp_keepalive: true,
            },
            optimization: OptimizationConfig {
                auto_optimization_enabled: true,
                optimization_interval: Duration::from_secs(300), // 5 minutes
                aggressive_optimization: false,
                memory_compaction_enabled: true,
                cache_warming_enabled: true,
                query_planning_cache_enabled: true,
            },
        }
    }

    /// Create a memory-optimized configuration
    pub fn memory_optimized() -> Self {
        Self {
            memory: MemoryConfig {
                max_memory_threshold: 2 * 1024 * 1024 * 1024, // 2GB
                max_heap_memory: 1024 * 1024 * 1024,          // 1GB
                max_cache_memory: 512 * 1024 * 1024,          // 512MB
                gc_threshold: 0.7,
                memory_pressure_threshold: 0.7,
            },
            cache: CacheConfig {
                min_hit_rate: 0.85,     // 85%
                max_eviction_rate: 0.1, // 10%
                max_accesses_per_cache: 20000,
                default_cache_size: 5000,
                max_cache_size: 500000,
                preload_enabled: false,
            },
            monitoring: MonitoringConfig {
                cpu_pressure_threshold: 0.8,      // 80%
                memory_pressure_threshold: 0.7,   // 70%
                disk_pressure_threshold: 0.9,     // 90%
                overall_pressure_threshold: 0.75, // 75%
                monitoring_interval: Duration::from_secs(2),
                max_history_size: 1000,
            },
            query: QueryConfig {
                slow_query_threshold_ms: 100,
                max_query_timeout_ms: 60000,
                query_cache_size: 500,
                query_cache_ttl_seconds: 1800,
                parallel_query_limit: 50,
                query_optimization_enabled: true,
            },
            system: SystemConfig {
                max_workers: 8,
                worker_threads: 4,
                io_threads: 2,
                max_connections: 5000,
                connection_timeout_ms: 10000,
                keep_alive_timeout_ms: 60000,
                tcp_nodelay: true,
                tcp_keepalive: true,
            },
            optimization: OptimizationConfig {
                auto_optimization_enabled: true,
                optimization_interval: Duration::from_secs(600), // 10 minutes
                aggressive_optimization: false,
                memory_compaction_enabled: true,
                cache_warming_enabled: false,
                query_planning_cache_enabled: true,
            },
        }
    }

    /// Create a development configuration
    pub fn development() -> Self {
        Self {
            memory: MemoryConfig {
                max_memory_threshold: 512 * 1024 * 1024, // 512MB
                max_heap_memory: 256 * 1024 * 1024,      // 256MB
                max_cache_memory: 128 * 1024 * 1024,     // 128MB
                gc_threshold: 0.8,
                memory_pressure_threshold: 0.8,
            },
            cache: CacheConfig {
                min_hit_rate: 0.7,      // 70%
                max_eviction_rate: 0.2, // 20%
                max_accesses_per_cache: 5000,
                default_cache_size: 1000,
                max_cache_size: 10000,
                preload_enabled: false,
            },
            monitoring: MonitoringConfig {
                cpu_pressure_threshold: 0.9,      // 90%
                memory_pressure_threshold: 0.8,   // 80%
                disk_pressure_threshold: 0.95,    // 95%
                overall_pressure_threshold: 0.85, // 85%
                monitoring_interval: Duration::from_secs(5),
                max_history_size: 500,
            },
            query: QueryConfig {
                slow_query_threshold_ms: 200,
                max_query_timeout_ms: 120000,
                query_cache_size: 100,
                query_cache_ttl_seconds: 300,
                parallel_query_limit: 10,
                query_optimization_enabled: false,
            },
            system: SystemConfig {
                max_workers: 4,
                worker_threads: 2,
                io_threads: 1,
                max_connections: 1000,
                connection_timeout_ms: 30000,
                keep_alive_timeout_ms: 120000,
                tcp_nodelay: false,
                tcp_keepalive: false,
            },
            optimization: OptimizationConfig {
                auto_optimization_enabled: false,
                optimization_interval: Duration::from_secs(1800), // 30 minutes
                aggressive_optimization: false,
                memory_compaction_enabled: false,
                cache_warming_enabled: false,
                query_planning_cache_enabled: false,
            },
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Memory validation
        if self.memory.max_memory_threshold == 0 {
            errors.push("Memory threshold cannot be zero".to_string());
        }
        if self.memory.max_heap_memory > self.memory.max_memory_threshold {
            errors.push("Heap memory cannot exceed total memory threshold".to_string());
        }
        if self.memory.max_cache_memory > self.memory.max_memory_threshold {
            errors.push("Cache memory cannot exceed total memory threshold".to_string());
        }

        // Cache validation
        if self.cache.min_hit_rate < 0.0 || self.cache.min_hit_rate > 1.0 {
            errors.push("Cache hit rate must be between 0.0 and 1.0".to_string());
        }
        if self.cache.max_eviction_rate < 0.0 || self.cache.max_eviction_rate > 1.0 {
            errors.push("Cache eviction rate must be between 0.0 and 1.0".to_string());
        }
        if self.cache.default_cache_size > self.cache.max_cache_size {
            errors.push("Default cache size cannot exceed maximum cache size".to_string());
        }

        // Monitoring validation
        if self.monitoring.cpu_pressure_threshold < 0.0
            || self.monitoring.cpu_pressure_threshold > 1.0
        {
            errors.push("CPU pressure threshold must be between 0.0 and 1.0".to_string());
        }
        if self.monitoring.memory_pressure_threshold < 0.0
            || self.monitoring.memory_pressure_threshold > 1.0
        {
            errors.push("Memory pressure threshold must be between 0.0 and 1.0".to_string());
        }

        // Query validation
        if self.query.slow_query_threshold_ms == 0 {
            errors.push("Slow query threshold cannot be zero".to_string());
        }
        if self.query.max_query_timeout_ms < self.query.slow_query_threshold_ms {
            errors.push("Query timeout must be greater than slow query threshold".to_string());
        }

        // System validation
        if self.system.max_workers == 0 {
            errors.push("Maximum workers cannot be zero".to_string());
        }
        if self.system.worker_threads > self.system.max_workers {
            errors.push("Worker threads cannot exceed maximum workers".to_string());
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Get configuration summary
    pub fn summary(&self) -> ConfigurationSummary {
        ConfigurationSummary {
            memory_total_mb: self.memory.max_memory_threshold / 1024 / 1024,
            memory_heap_mb: self.memory.max_heap_memory / 1024 / 1024,
            memory_cache_mb: self.memory.max_cache_memory / 1024 / 1024,
            cache_hit_rate_target: self.cache.min_hit_rate,
            cache_max_size: self.cache.max_cache_size,
            monitoring_interval_ms: self.monitoring.monitoring_interval.as_millis() as u64,
            max_workers: self.system.max_workers,
            max_connections: self.system.max_connections,
            auto_optimization: self.optimization.auto_optimization_enabled,
        }
    }
}

/// Query performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryConfig {
    pub slow_query_threshold_ms: u64,
    pub max_query_timeout_ms: u64,
    pub query_cache_size: usize,
    pub query_cache_ttl_seconds: u64,
    pub parallel_query_limit: usize,
    pub query_optimization_enabled: bool,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            slow_query_threshold_ms: 100,
            max_query_timeout_ms: 30000,
            query_cache_size: 1000,
            query_cache_ttl_seconds: 3600,
            parallel_query_limit: 50,
            query_optimization_enabled: true,
        }
    }
}

/// System performance configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemConfig {
    pub max_workers: usize,
    pub worker_threads: usize,
    pub io_threads: usize,
    pub max_connections: usize,
    pub connection_timeout_ms: u64,
    pub keep_alive_timeout_ms: u64,
    pub tcp_nodelay: bool,
    pub tcp_keepalive: bool,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            max_workers: 8,
            worker_threads: 4,
            io_threads: 2,
            max_connections: 5000,
            connection_timeout_ms: 5000,
            keep_alive_timeout_ms: 30000,
            tcp_nodelay: true,
            tcp_keepalive: true,
        }
    }
}

/// Optimization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationConfig {
    pub auto_optimization_enabled: bool,
    pub optimization_interval: Duration,
    pub aggressive_optimization: bool,
    pub memory_compaction_enabled: bool,
    pub cache_warming_enabled: bool,
    pub query_planning_cache_enabled: bool,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            auto_optimization_enabled: true,
            optimization_interval: Duration::from_secs(600), // 10 minutes
            aggressive_optimization: false,
            memory_compaction_enabled: true,
            cache_warming_enabled: true,
            query_planning_cache_enabled: true,
        }
    }
}

/// Configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigurationSummary {
    pub memory_total_mb: u64,
    pub memory_heap_mb: u64,
    pub memory_cache_mb: u64,
    pub cache_hit_rate_target: f64,
    pub cache_max_size: usize,
    pub monitoring_interval_ms: u64,
    pub max_workers: usize,
    pub max_connections: usize,
    pub auto_optimization: bool,
}

/// Configuration presets
pub struct ConfigurationPresets;

impl ConfigurationPresets {
    /// Get all available presets
    pub fn list() -> Vec<ConfigurationPreset> {
        vec![
            ConfigurationPreset {
                name: "default".to_string(),
                description: "Balanced configuration for general use".to_string(),
                config: PerformanceConfig::default(),
            },
            ConfigurationPreset {
                name: "high_performance".to_string(),
                description: "Optimized for maximum performance".to_string(),
                config: PerformanceConfig::high_performance(),
            },
            ConfigurationPreset {
                name: "memory_optimized".to_string(),
                description: "Optimized for memory efficiency".to_string(),
                config: PerformanceConfig::memory_optimized(),
            },
            ConfigurationPreset {
                name: "development".to_string(),
                description: "Configuration for development and testing".to_string(),
                config: PerformanceConfig::development(),
            },
        ]
    }

    /// Get a preset by name
    pub fn get(name: &str) -> Option<PerformanceConfig> {
        match name {
            "default" => Some(PerformanceConfig::default()),
            "high_performance" => Some(PerformanceConfig::high_performance()),
            "memory_optimized" => Some(PerformanceConfig::memory_optimized()),
            "development" => Some(PerformanceConfig::development()),
            _ => None,
        }
    }
}

/// Configuration preset
#[derive(Debug, Clone)]
pub struct ConfigurationPreset {
    pub name: String,
    pub description: String,
    pub config: PerformanceConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_configuration() {
        let config = PerformanceConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_high_performance_configuration() {
        let config = PerformanceConfig::high_performance();
        assert!(config.validate().is_ok());
        assert!(config.memory.max_memory_threshold > 1024 * 1024 * 1024);
    }

    #[test]
    fn test_memory_optimized_configuration() {
        let config = PerformanceConfig::memory_optimized();
        assert!(config.validate().is_ok());
        assert!(config.memory.max_memory_threshold < 4 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_development_configuration() {
        let config = PerformanceConfig::development();
        assert!(config.validate().is_ok());
        assert!(!config.optimization.auto_optimization_enabled);
    }

    #[test]
    fn test_configuration_validation() {
        let mut config = PerformanceConfig::default();
        config.memory.max_memory_threshold = 0;

        let result = config.validate();
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains(&"Memory threshold cannot be zero".to_string())
        );
    }

    #[test]
    fn test_configuration_summary() {
        let config = PerformanceConfig::default();
        let summary = config.summary();

        assert!(summary.memory_total_mb > 0);
        assert!(summary.cache_hit_rate_target > 0.0);
        assert!(summary.max_workers > 0);
    }

    #[test]
    fn test_configuration_presets() {
        let presets = ConfigurationPresets::list();
        assert!(!presets.is_empty());

        let high_perf = ConfigurationPresets::get("high_performance");
        assert!(high_perf.is_some());

        let unknown = ConfigurationPresets::get("unknown");
        assert!(unknown.is_none());
    }
}
