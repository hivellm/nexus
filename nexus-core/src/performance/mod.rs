//! Performance optimization utilities for Nexus
//!
//! This module provides comprehensive performance optimization tools including:
//! - Query profiling and analysis
//! - Memory optimization utilities
//! - Cache optimization tools
//! - System resource monitoring
//! - Performance configuration helpers
//! - Testing and validation tools

pub mod cache;
pub mod config;
pub mod memory;
pub mod metrics;
pub mod monitoring;
pub mod profiler;
pub mod testing;

pub use cache::CacheOptimizer;
pub use config::PerformanceConfig;
pub use memory::MemoryOptimizer;
pub use metrics::PerformanceMetrics;
pub use monitoring::SystemMonitor;
pub use profiler::QueryProfiler;
pub use testing::PerformanceTester;

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
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
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
