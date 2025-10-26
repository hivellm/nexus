//! Performance optimization utilities for Nexus
//!
//! This module provides comprehensive performance optimization tools including:
//! - Query profiling and analysis
//! - Memory optimization utilities
//! - Cache optimization tools
//! - System resource monitoring
//! - Performance configuration helpers
//! - Testing and validation tools

pub mod benchmarking;
pub mod cache;
pub mod config;
pub mod memory;
pub mod metrics;
pub mod monitoring;
pub mod profiler;
pub mod recommendations;
pub mod testing;
pub mod visualization;

pub use benchmarking::PerformanceBenchmark;
pub use cache::CacheOptimizer;
pub use config::PerformanceConfig;
pub use memory::MemoryOptimizer;
pub use metrics::PerformanceMetrics;
pub use monitoring::SystemMonitor;
pub use profiler::QueryProfiler;
pub use recommendations::PerformanceRecommendations;
pub use testing::PerformanceTester;
pub use visualization::PerformanceVisualizer;

use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Performance optimization result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationResult {
    pub name: String,
    pub before_metric: f64,
    pub after_metric: f64,
    pub improvement_percent: f64,
    pub duration: Duration,
    pub recommendations: Vec<String>,
}

/// Performance optimization suite
pub struct PerformanceOptimizer {
    profiler: QueryProfiler,
    memory_optimizer: MemoryOptimizer,
    cache_optimizer: CacheOptimizer,
    system_monitor: SystemMonitor,
    config: PerformanceConfig,
}

impl PerformanceOptimizer {
    /// Create a new performance optimizer
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            profiler: QueryProfiler::new()?,
            memory_optimizer: MemoryOptimizer::new()?,
            cache_optimizer: CacheOptimizer::new()?,
            system_monitor: SystemMonitor::new()?,
            config: PerformanceConfig::default(),
        })
    }

    /// Run comprehensive performance optimization
    pub async fn optimize_all(
        &mut self,
    ) -> Result<Vec<OptimizationResult>, Box<dyn std::error::Error>> {
        let mut results = Vec::new();

        // Memory optimization
        if let Ok(result) = self.memory_optimizer.optimize().await {
            results.push(result);
        }

        // Cache optimization
        if let Ok(result) = self.cache_optimizer.optimize().await {
            results.push(result);
        }

        // System optimization
        if let Ok(result) = self.system_monitor.optimize_system().await {
            results.push(result);
        }

        Ok(results)
    }

    /// Profile a specific query
    pub async fn profile_query(
        &mut self,
        query: &str,
    ) -> Result<QueryProfile, Box<dyn std::error::Error>> {
        self.profiler.profile_query(query).await
    }

    /// Get current system performance metrics
    pub async fn get_system_metrics(&self) -> Result<SystemMetrics, Box<dyn std::error::Error>> {
        self.system_monitor.get_metrics().await
    }

    /// Update performance configuration
    pub fn update_config(&mut self, config: PerformanceConfig) {
        self.config = config;
    }
}

/// Query profile information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryProfile {
    pub query: String,
    pub execution_time: Duration,
    pub memory_usage: u64,
    pub cpu_usage: f64,
    pub io_operations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub recommendations: Vec<String>,
}

/// System performance metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub memory_available: u64,
    pub disk_usage: f64,
    pub network_io: NetworkMetrics,
    pub cache_metrics: CacheMetrics,
    pub timestamp: Instant,
}

/// Network I/O metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
}

/// Cache performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheMetrics {
    pub hit_rate: f64,
    pub miss_rate: f64,
    pub total_requests: u64,
    pub cache_size: u64,
    pub evictions: u64,
}

/// Performance optimization recommendations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationRecommendation {
    pub category: String,
    pub priority: Priority,
    pub description: String,
    pub impact: Impact,
    pub effort: Effort,
    pub implementation: String,
}

/// Priority levels for recommendations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

/// Impact levels for recommendations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Impact {
    Low,
    Medium,
    High,
    VeryHigh,
}

/// Effort levels for recommendations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effort {
    Low,
    Medium,
    High,
    VeryHigh,
}

impl Default for PerformanceOptimizer {
    fn default() -> Self {
        Self::new().unwrap_or_else(|_| {
            // Fallback implementation
            Self {
                profiler: QueryProfiler::new().unwrap_or_else(|_| QueryProfiler::default()),
                memory_optimizer: MemoryOptimizer::new()
                    .unwrap_or_else(|_| MemoryOptimizer::default()),
                cache_optimizer: CacheOptimizer::new()
                    .unwrap_or_else(|_| CacheOptimizer::default()),
                system_monitor: SystemMonitor::new().unwrap_or_else(|_| SystemMonitor::default()),
                config: PerformanceConfig::default(),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_optimization_result_creation() {
        let result = OptimizationResult {
            name: "test_optimization".to_string(),
            before_metric: 100.0,
            after_metric: 80.0,
            improvement_percent: 20.0,
            duration: Duration::from_millis(100),
            recommendations: vec!["Use index".to_string(), "Optimize query".to_string()],
        };

        assert_eq!(result.name, "test_optimization");
        assert_eq!(result.before_metric, 100.0);
        assert_eq!(result.after_metric, 80.0);
        assert_eq!(result.improvement_percent, 20.0);
        assert_eq!(result.duration, Duration::from_millis(100));
        assert_eq!(result.recommendations.len(), 2);
    }

    #[test]
    fn test_optimization_result_serialization() {
        let result = OptimizationResult {
            name: "test_optimization".to_string(),
            before_metric: 100.0,
            after_metric: 80.0,
            improvement_percent: 20.0,
            duration: Duration::from_millis(100),
            recommendations: vec!["Use index".to_string()],
        };

        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: OptimizationResult = serde_json::from_str(&serialized).unwrap();

        assert_eq!(result.name, deserialized.name);
        assert_eq!(result.before_metric, deserialized.before_metric);
        assert_eq!(result.after_metric, deserialized.after_metric);
        assert_eq!(result.improvement_percent, deserialized.improvement_percent);
        assert_eq!(result.recommendations, deserialized.recommendations);
    }

    #[test]
    fn test_performance_optimizer_default() {
        let _optimizer = PerformanceOptimizer::default();
        // Should not panic and create a valid instance
        // Basic test that default() works - no assertion needed
    }

    #[test]
    fn test_query_profile_creation() {
        let profile = QueryProfile {
            query: "MATCH (n) RETURN n".to_string(),
            execution_time: Duration::from_millis(50),
            memory_usage: 1024,
            cpu_usage: 25.5,
            io_operations: 10,
            cache_hits: 8,
            cache_misses: 2,
            recommendations: vec!["Add index".to_string()],
        };

        assert_eq!(profile.query, "MATCH (n) RETURN n");
        assert_eq!(profile.execution_time, Duration::from_millis(50));
        assert_eq!(profile.memory_usage, 1024);
        assert_eq!(profile.cpu_usage, 25.5);
        assert_eq!(profile.io_operations, 10);
        assert_eq!(profile.cache_hits, 8);
        assert_eq!(profile.cache_misses, 2);
        assert_eq!(profile.recommendations.len(), 1);
    }

    #[test]
    fn test_query_profile_serialization() {
        let profile = QueryProfile {
            query: "MATCH (n) RETURN n".to_string(),
            execution_time: Duration::from_millis(50),
            memory_usage: 1024,
            cpu_usage: 25.5,
            io_operations: 10,
            cache_hits: 8,
            cache_misses: 2,
            recommendations: vec!["Add index".to_string()],
        };

        let serialized = serde_json::to_string(&profile).unwrap();
        let deserialized: QueryProfile = serde_json::from_str(&serialized).unwrap();

        assert_eq!(profile.query, deserialized.query);
        assert_eq!(profile.memory_usage, deserialized.memory_usage);
        assert_eq!(profile.cpu_usage, deserialized.cpu_usage);
        assert_eq!(profile.io_operations, deserialized.io_operations);
        assert_eq!(profile.cache_hits, deserialized.cache_hits);
        assert_eq!(profile.cache_misses, deserialized.cache_misses);
        assert_eq!(profile.recommendations, deserialized.recommendations);
    }

    #[test]
    fn test_system_metrics_creation() {
        let network_metrics = NetworkMetrics {
            bytes_sent: 1000,
            bytes_received: 2000,
            packets_sent: 10,
            packets_received: 20,
        };

        let cache_metrics = CacheMetrics {
            hit_rate: 0.8,
            miss_rate: 0.2,
            total_requests: 1000,
            cache_size: 1024,
            evictions: 5,
        };

        let metrics = SystemMetrics {
            cpu_usage: 25.5,
            memory_usage: 1024 * 1024,
            memory_available: 2048 * 1024,
            disk_usage: 50.0,
            network_io: network_metrics,
            cache_metrics,
            timestamp: Instant::now(),
        };

        assert_eq!(metrics.cpu_usage, 25.5);
        assert_eq!(metrics.memory_usage, 1024 * 1024);
        assert_eq!(metrics.memory_available, 2048 * 1024);
        assert_eq!(metrics.disk_usage, 50.0);
        assert_eq!(metrics.network_io.bytes_sent, 1000);
        assert_eq!(metrics.network_io.bytes_received, 2000);
        assert_eq!(metrics.cache_metrics.hit_rate, 0.8);
        assert_eq!(metrics.cache_metrics.miss_rate, 0.2);
    }

    #[test]
    fn test_network_metrics_serialization() {
        let network_metrics = NetworkMetrics {
            bytes_sent: 1000,
            bytes_received: 2000,
            packets_sent: 10,
            packets_received: 20,
        };

        let serialized = serde_json::to_string(&network_metrics).unwrap();
        let deserialized: NetworkMetrics = serde_json::from_str(&serialized).unwrap();

        assert_eq!(network_metrics.bytes_sent, deserialized.bytes_sent);
        assert_eq!(network_metrics.bytes_received, deserialized.bytes_received);
        assert_eq!(network_metrics.packets_sent, deserialized.packets_sent);
        assert_eq!(
            network_metrics.packets_received,
            deserialized.packets_received
        );
    }

    #[test]
    fn test_cache_metrics_serialization() {
        let cache_metrics = CacheMetrics {
            hit_rate: 0.8,
            miss_rate: 0.2,
            total_requests: 1000,
            cache_size: 1024,
            evictions: 5,
        };

        let serialized = serde_json::to_string(&cache_metrics).unwrap();
        let deserialized: CacheMetrics = serde_json::from_str(&serialized).unwrap();

        assert_eq!(cache_metrics.hit_rate, deserialized.hit_rate);
        assert_eq!(cache_metrics.miss_rate, deserialized.miss_rate);
        assert_eq!(cache_metrics.total_requests, deserialized.total_requests);
        assert_eq!(cache_metrics.cache_size, deserialized.cache_size);
        assert_eq!(cache_metrics.evictions, deserialized.evictions);
    }

    #[test]
    fn test_optimization_recommendation_creation() {
        let recommendation = OptimizationRecommendation {
            category: "Query".to_string(),
            priority: Priority::High,
            description: "Add index for better performance".to_string(),
            impact: Impact::High,
            effort: Effort::Medium,
            implementation: "CREATE INDEX ON (n:Label)".to_string(),
        };

        assert_eq!(recommendation.category, "Query");
        assert_eq!(recommendation.priority, Priority::High);
        assert_eq!(
            recommendation.description,
            "Add index for better performance"
        );
        assert_eq!(recommendation.impact, Impact::High);
        assert_eq!(recommendation.effort, Effort::Medium);
        assert_eq!(recommendation.implementation, "CREATE INDEX ON (n:Label)");
    }

    #[test]
    fn test_optimization_recommendation_serialization() {
        let recommendation = OptimizationRecommendation {
            category: "Query".to_string(),
            priority: Priority::High,
            description: "Add index for better performance".to_string(),
            impact: Impact::High,
            effort: Effort::Medium,
            implementation: "CREATE INDEX ON (n:Label)".to_string(),
        };

        let serialized = serde_json::to_string(&recommendation).unwrap();
        let deserialized: OptimizationRecommendation = serde_json::from_str(&serialized).unwrap();

        assert_eq!(recommendation.category, deserialized.category);
        assert_eq!(recommendation.priority, deserialized.priority);
        assert_eq!(recommendation.description, deserialized.description);
        assert_eq!(recommendation.impact, deserialized.impact);
        assert_eq!(recommendation.effort, deserialized.effort);
        assert_eq!(recommendation.implementation, deserialized.implementation);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(Priority::Low < Priority::Medium);
        assert!(Priority::Medium < Priority::High);
        assert!(Priority::High < Priority::Critical);
        assert_eq!(Priority::High, Priority::High);
    }

    #[test]
    fn test_impact_ordering() {
        assert!(Impact::Low < Impact::Medium);
        assert!(Impact::Medium < Impact::High);
        assert!(Impact::High < Impact::VeryHigh);
        assert_eq!(Impact::High, Impact::High);
    }

    #[test]
    fn test_effort_ordering() {
        assert!(Effort::Low < Effort::Medium);
        assert!(Effort::Medium < Effort::High);
        assert!(Effort::High < Effort::VeryHigh);
        assert_eq!(Effort::High, Effort::High);
    }
}
