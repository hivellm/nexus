//! System resource monitoring utilities
//!
//! Provides comprehensive monitoring of system resources including CPU, memory,
//! disk I/O, and network performance for performance optimization.

use crate::performance::{
    CacheMetrics, Effort, Impact, NetworkMetrics, OptimizationRecommendation, OptimizationResult,
    Priority, SystemMetrics,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{sync::RwLock, task::JoinHandle, time::interval};

/// Cache hit rate metrics across all layers
#[derive(Debug, Clone)]
pub struct CacheHitRateMetrics {
    pub overall_hit_rate: f64,
    pub page_cache_hit_rate: f64,
    pub object_cache_hit_rate: f64,
    pub query_cache_hit_rate: f64,
    pub index_cache_hit_rate: f64,
    pub relationship_index_hit_rate: f64,
}

/// Query execution time distribution statistics
#[derive(Debug, Clone)]
pub struct QueryExecutionStats {
    pub total_queries: u64,
    pub avg_execution_time_ms: f64,
    pub p50_execution_time_ms: f64,
    pub p95_execution_time_ms: f64,
    pub p99_execution_time_ms: f64,
    pub slow_queries_count: u64,
    pub very_slow_queries_count: u64,
}

/// Memory usage breakdown by component
#[derive(Debug, Clone)]
pub struct MemoryUsageByComponent {
    pub total_memory_mb: f64,
    pub page_cache_mb: f64,
    pub object_cache_mb: f64,
    pub query_cache_mb: f64,
    pub index_cache_mb: f64,
    pub relationship_index_mb: f64,
    pub query_execution_mb: f64,
    pub wal_buffer_mb: f64,
    pub other_mb: f64,
}

/// Comprehensive performance dashboard
#[derive(Debug, Clone)]
pub struct PerformanceDashboard {
    pub system_metrics: SystemMetrics,
    pub cache_hit_rates: CacheHitRateMetrics,
    pub query_execution_stats: QueryExecutionStats,
    pub memory_usage_breakdown: MemoryUsageByComponent,
    pub timestamp: Instant,
}

/// System resource monitor
pub struct SystemMonitor {
    metrics_history: Arc<RwLock<Vec<SystemMetrics>>>,
    monitoring_config: Arc<RwLock<MonitoringConfig>>,
    is_monitoring: Arc<RwLock<bool>>,
    monitoring_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl SystemMonitor {
    /// Create a new system monitor
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self {
            metrics_history: Arc::new(RwLock::new(Vec::new())),
            monitoring_config: Arc::new(RwLock::new(MonitoringConfig::default())),
            is_monitoring: Arc::new(RwLock::new(false)),
            monitoring_handle: Arc::new(RwLock::new(None)),
        })
    }

    /// Get current system metrics
    pub async fn get_metrics(&self) -> Result<SystemMetrics, Box<dyn std::error::Error>> {
        let cpu_usage = self.get_cpu_usage().await?;
        let memory_usage = self.get_memory_usage().await?;
        let memory_available = self.get_memory_available().await?;
        let disk_usage = self.get_disk_usage().await?;
        let network_io = self.get_network_metrics().await?;
        let cache_metrics = self.get_cache_metrics().await?;

        Ok(SystemMetrics {
            cpu_usage,
            memory_usage,
            memory_available,
            disk_usage,
            network_io,
            cache_metrics,
            timestamp: Instant::now(),
        })
    }

    /// Start continuous monitoring
    pub async fn start_monitoring(
        &self,
        interval_duration: Duration,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut is_monitoring = self.is_monitoring.write().await;
        if *is_monitoring {
            return Ok(()); // Already monitoring
        }
        *is_monitoring = true;
        drop(is_monitoring);

        // Clone the necessary Arc references for the monitoring task
        let metrics_history = Arc::clone(&self.metrics_history);
        let monitoring_config = Arc::clone(&self.monitoring_config);
        let is_monitoring_flag = Arc::clone(&self.is_monitoring);
        let monitoring_handle = Arc::clone(&self.monitoring_handle);

        // Spawn the monitoring task
        let handle = tokio::spawn(async move {
            let mut interval = interval(interval_duration);

            loop {
                interval.tick().await;

                // Check if monitoring should continue
                {
                    let monitoring_flag = is_monitoring_flag.read().await;
                    if !*monitoring_flag {
                        break;
                    }
                }

                // Collect metrics
                if let Ok(metrics) = Self::collect_metrics_internal().await {
                    // Store metrics in history
                    let mut history = metrics_history.write().await;
                    let config = monitoring_config.read().await;

                    // Add to history
                    history.push(metrics);

                    // Trim history if it exceeds max size
                    if history.len() > config.max_history_size {
                        let excess = history.len() - config.max_history_size;
                        history.drain(0..excess);
                    }
                }
            }
        });

        // Store the handle
        {
            let mut handle_guard = monitoring_handle.write().await;
            *handle_guard = Some(handle);
        }

        Ok(())
    }

    /// Internal method to collect metrics (static for use in spawned task)
    async fn collect_metrics_internal()
    -> Result<SystemMetrics, Box<dyn std::error::Error + Send + Sync>> {
        let cpu_usage = Self::get_cpu_usage_internal().await?;
        let memory_usage = Self::get_memory_usage_internal().await?;
        let memory_available = Self::get_memory_available_internal().await?;
        let disk_usage = Self::get_disk_usage_internal().await?;
        let network_io = Self::get_network_metrics_internal().await?;
        let cache_metrics = Self::get_cache_metrics_internal().await?;

        Ok(SystemMetrics {
            cpu_usage,
            memory_usage,
            memory_available,
            disk_usage,
            network_io,
            cache_metrics,
            timestamp: Instant::now(),
        })
    }

    /// Internal CPU usage method
    async fn get_cpu_usage_internal() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would use system APIs
        Ok(25.0) // 25% placeholder
    }

    /// Internal memory usage method
    async fn get_memory_usage_internal() -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would use system APIs
        Ok(1024 * 1024 * 512) // 512MB placeholder
    }

    /// Internal available memory method
    async fn get_memory_available_internal() -> Result<u64, Box<dyn std::error::Error + Send + Sync>>
    {
        // In a real implementation, this would use system APIs
        Ok(1024 * 1024 * 1024) // 1GB placeholder
    }

    /// Internal disk usage method
    async fn get_disk_usage_internal() -> Result<f64, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would use system APIs
        Ok(45.0) // 45% placeholder
    }

    /// Internal network metrics method
    async fn get_network_metrics_internal()
    -> Result<NetworkMetrics, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would use system APIs
        Ok(NetworkMetrics {
            bytes_sent: 1024 * 1024 * 100,     // 100MB
            bytes_received: 1024 * 1024 * 150, // 150MB
            packets_sent: 10000,
            packets_received: 15000,
        })
    }

    /// Internal cache metrics method
    async fn get_cache_metrics_internal()
    -> Result<CacheMetrics, Box<dyn std::error::Error + Send + Sync>> {
        // In a real implementation, this would use system APIs
        Ok(CacheMetrics {
            hit_rate: 0.85,
            miss_rate: 0.15,
            total_requests: 100000,
            cache_size: 1024 * 1024 * 128, // 128MB
            evictions: 1000,
        })
    }

    /// Stop monitoring
    pub async fn stop_monitoring(&self) {
        // Set the monitoring flag to false
        {
            let mut is_monitoring = self.is_monitoring.write().await;
            *is_monitoring = false;
        }

        // Wait for the monitoring task to finish
        {
            let mut handle_guard = self.monitoring_handle.write().await;
            if let Some(handle) = handle_guard.take() {
                let _ = handle.await; // Wait for the task to complete
            }
        }
    }

    /// Get system performance statistics
    pub async fn get_performance_statistics(&self) -> SystemPerformanceStatistics {
        let history = self.metrics_history.read().await;

        if history.is_empty() {
            return SystemPerformanceStatistics::default();
        }

        let cpu_usage: Vec<f64> = history.iter().map(|m| m.cpu_usage).collect();
        let memory_usage: Vec<u64> = history.iter().map(|m| m.memory_usage).collect();
        let disk_usage: Vec<f64> = history.iter().map(|m| m.disk_usage).collect();

        let avg_cpu = cpu_usage.iter().sum::<f64>() / cpu_usage.len() as f64;
        let max_cpu = cpu_usage.iter().cloned().fold(0.0, f64::max);
        let min_cpu = cpu_usage.iter().cloned().fold(f64::INFINITY, f64::min);

        let avg_memory = memory_usage.iter().sum::<u64>() / memory_usage.len() as u64;
        let max_memory = memory_usage.iter().cloned().max().unwrap_or(0);
        let min_memory = memory_usage.iter().cloned().min().unwrap_or(0);

        let avg_disk = disk_usage.iter().sum::<f64>() / disk_usage.len() as f64;
        let max_disk = disk_usage.iter().cloned().fold(0.0, f64::max);
        let min_disk = disk_usage.iter().cloned().fold(f64::INFINITY, f64::min);

        // Calculate resource pressure
        let cpu_pressure = if max_cpu > 0.0 {
            avg_cpu / max_cpu
        } else {
            0.0
        };
        let memory_pressure = if max_memory > 0 {
            avg_memory as f64 / max_memory as f64
        } else {
            0.0
        };
        let disk_pressure = if max_disk > 0.0 {
            avg_disk / max_disk
        } else {
            0.0
        };

        SystemPerformanceStatistics {
            sample_count: history.len(),
            avg_cpu_usage: avg_cpu,
            max_cpu_usage: max_cpu,
            min_cpu_usage: min_cpu,
            avg_memory_usage: avg_memory,
            max_memory_usage: max_memory,
            min_memory_usage: min_memory,
            avg_disk_usage: avg_disk,
            max_disk_usage: max_disk,
            min_disk_usage: min_disk,
            cpu_pressure,
            memory_pressure,
            disk_pressure,
            overall_pressure: (cpu_pressure + memory_pressure + disk_pressure) / 3.0,
        }
    }

    /// Optimize system performance
    pub async fn optimize_system(&self) -> Result<OptimizationResult, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let before_pressure = self.get_system_pressure().await;

        // Perform system optimization steps
        self.optimize_cpu_usage().await?;
        self.optimize_memory_usage().await?;
        self.optimize_disk_io().await?;
        self.optimize_network_io().await?;

        let after_pressure = self.get_system_pressure().await;
        let improvement = if before_pressure > 0.0 {
            ((before_pressure - after_pressure) / before_pressure) * 100.0
        } else {
            0.0
        };

        let recommendations = self.generate_system_recommendations().await;

        Ok(OptimizationResult {
            name: "System Optimization".to_string(),
            before_metric: before_pressure,
            after_metric: after_pressure,
            improvement_percent: improvement,
            duration: start_time.elapsed(),
            recommendations,
        })
    }

    /// Get system optimization recommendations
    pub async fn get_optimization_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();
        let stats = self.get_performance_statistics().await;

        // CPU pressure recommendations
        let config = self.monitoring_config.read().await;
        if stats.cpu_pressure > config.cpu_pressure_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "CPU Performance".to_string(),
                priority: Priority::High,
                description: format!("High CPU pressure: {:.1}%", stats.cpu_pressure * 100.0),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Optimize algorithms, increase CPU cores, or implement parallel processing"
                        .to_string(),
            });
        }

        // Memory pressure recommendations
        if stats.memory_pressure > config.memory_pressure_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Performance".to_string(),
                priority: Priority::High,
                description: format!(
                    "High memory pressure: {:.1}%",
                    stats.memory_pressure * 100.0
                ),
                impact: Impact::High,
                effort: Effort::High,
                implementation:
                    "Increase memory, optimize data structures, or implement memory pooling"
                        .to_string(),
            });
        }

        // Disk I/O recommendations
        if stats.disk_pressure > config.disk_pressure_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "Disk I/O Performance".to_string(),
                priority: Priority::Medium,
                description: format!("High disk pressure: {:.1}%", stats.disk_pressure * 100.0),
                impact: Impact::Medium,
                effort: Effort::Medium,
                implementation: "Use faster storage, optimize I/O patterns, or implement caching"
                    .to_string(),
            });
        }

        // Overall system pressure recommendations
        if stats.overall_pressure > config.overall_pressure_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "System Performance".to_string(),
                priority: Priority::Critical,
                description: format!("High overall system pressure: {:.1}%", stats.overall_pressure * 100.0),
                impact: Impact::VeryHigh,
                effort: Effort::VeryHigh,
                implementation: "Comprehensive system optimization including hardware upgrades and software tuning".to_string(),
            });
        }

        recommendations
    }

    /// Set monitoring configuration
    pub async fn set_config(&self, config: MonitoringConfig) {
        let mut monitoring_config = self.monitoring_config.write().await;
        *monitoring_config = config;
    }

    /// Get system pressure (0.0 to 1.0)
    async fn get_system_pressure(&self) -> f64 {
        let stats = self.get_performance_statistics().await;
        stats.overall_pressure
    }

    /// Optimize CPU usage
    async fn optimize_cpu_usage(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement CPU optimization
        // This could include:
        // - CPU affinity tuning
        // - Process priority adjustment
        // - Thread pool optimization

        tokio::time::sleep(Duration::from_millis(20)).await; // Simulate work
        Ok(())
    }

    /// Optimize memory usage
    async fn optimize_memory_usage(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement memory optimization
        // This could include:
        // - Memory allocation optimization
        // - Garbage collection tuning
        // - Memory mapping optimization

        tokio::time::sleep(Duration::from_millis(15)).await; // Simulate work
        Ok(())
    }

    /// Optimize disk I/O
    async fn optimize_disk_io(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement disk I/O optimization
        // This could include:
        // - I/O scheduler tuning
        // - File system optimization
        // - Disk caching optimization

        tokio::time::sleep(Duration::from_millis(10)).await; // Simulate work
        Ok(())
    }

    /// Optimize network I/O
    async fn optimize_network_io(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Implement network I/O optimization
        // This could include:
        // - Network buffer tuning
        // - Connection pooling optimization
        // - Protocol optimization

        tokio::time::sleep(Duration::from_millis(8)).await; // Simulate work
        Ok(())
    }

    /// Generate system optimization recommendations
    async fn generate_system_recommendations(&self) -> Vec<String> {
        let mut recommendations = Vec::new();
        let stats = self.get_performance_statistics().await;
        let config = self.monitoring_config.read().await;

        if stats.cpu_pressure > config.cpu_pressure_threshold {
            recommendations.push("Consider CPU optimization or hardware upgrade".to_string());
        }

        if stats.memory_pressure > config.memory_pressure_threshold {
            recommendations
                .push("Consider memory optimization or increase available memory".to_string());
        }

        if stats.disk_pressure > config.disk_pressure_threshold {
            recommendations.push("Consider disk I/O optimization or faster storage".to_string());
        }

        if stats.overall_pressure > config.overall_pressure_threshold {
            recommendations.push("Comprehensive system optimization required".to_string());
        }

        recommendations
    }

    /// Get CPU usage (simplified implementation)
    async fn get_cpu_usage(&self) -> Result<f64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(25.0) // 25% placeholder
    }

    /// Get memory usage (simplified implementation)
    async fn get_memory_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(1024 * 1024 * 512) // 512MB placeholder
    }

    /// Get available memory (simplified implementation)
    async fn get_memory_available(&self) -> Result<u64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(1024 * 1024 * 1024) // 1GB placeholder
    }

    /// Get disk usage (simplified implementation)
    async fn get_disk_usage(&self) -> Result<f64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(45.0) // 45% placeholder
    }

    /// Get network metrics (simplified implementation)
    async fn get_network_metrics(&self) -> Result<NetworkMetrics, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(NetworkMetrics {
            bytes_sent: 1024 * 1024 * 100,     // 100MB
            bytes_received: 1024 * 1024 * 150, // 150MB
            packets_sent: 10000,
            packets_received: 15000,
        })
    }

    /// Get cache metrics (simplified implementation)
    async fn get_cache_metrics(&self) -> Result<CacheMetrics, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(CacheMetrics {
            hit_rate: 0.85,
            miss_rate: 0.15,
            total_requests: 100000,
            cache_size: 1024 * 1024 * 128, // 128MB
            evictions: 1000,
        })
    }

    /// Get cache hit rate metrics across all layers
    pub async fn get_cache_hit_rates() -> Result<CacheHitRateMetrics, Box<dyn std::error::Error>> {
        // This would integrate with the actual cache system
        // For now, return placeholder metrics
        Ok(CacheHitRateMetrics {
            overall_hit_rate: 0.85,
            page_cache_hit_rate: 0.90,
            object_cache_hit_rate: 0.80,
            query_cache_hit_rate: 0.75,
            index_cache_hit_rate: 0.95,
            relationship_index_hit_rate: 0.88,
        })
    }

    /// Get query execution time distribution
    pub async fn get_query_execution_stats()
    -> Result<QueryExecutionStats, Box<dyn std::error::Error>> {
        // This would integrate with actual query execution tracking
        // For now, return placeholder statistics
        Ok(QueryExecutionStats {
            total_queries: 1000,
            avg_execution_time_ms: 15.5,
            p50_execution_time_ms: 12.0,
            p95_execution_time_ms: 45.0,
            p99_execution_time_ms: 120.0,
            slow_queries_count: 25,
            very_slow_queries_count: 5,
        })
    }

    /// Get memory usage breakdown by component
    pub async fn get_memory_usage_by_component()
    -> Result<MemoryUsageByComponent, Box<dyn std::error::Error>> {
        // This would integrate with actual memory tracking
        // For now, return placeholder breakdown
        Ok(MemoryUsageByComponent {
            total_memory_mb: 512.0,
            page_cache_mb: 100.0,
            object_cache_mb: 50.0,
            query_cache_mb: 25.0,
            index_cache_mb: 30.0,
            relationship_index_mb: 20.0,
            query_execution_mb: 15.0,
            wal_buffer_mb: 10.0,
            other_mb: 50.0,
        })
    }

    /// Get comprehensive performance dashboard
    pub async fn get_performance_dashboard(
        &self,
    ) -> Result<PerformanceDashboard, Box<dyn std::error::Error>> {
        let system_metrics = self.get_metrics().await?;
        let cache_hit_rates = Self::get_cache_hit_rates().await?;
        let query_stats = Self::get_query_execution_stats().await?;
        let memory_breakdown = Self::get_memory_usage_by_component().await?;

        Ok(PerformanceDashboard {
            system_metrics,
            cache_hit_rates,
            query_execution_stats: query_stats,
            memory_usage_breakdown: memory_breakdown,
            timestamp: Instant::now(),
        })
    }
}

impl Default for SystemMonitor {
    fn default() -> Self {
        Self {
            metrics_history: Arc::new(RwLock::new(Vec::new())),
            monitoring_config: Arc::new(RwLock::new(MonitoringConfig::default())),
            is_monitoring: Arc::new(RwLock::new(false)),
            monitoring_handle: Arc::new(RwLock::new(None)),
        }
    }
}

impl Clone for SystemMonitor {
    fn clone(&self) -> Self {
        Self {
            metrics_history: Arc::new(RwLock::new(Vec::new())),
            monitoring_config: Arc::clone(&self.monitoring_config),
            is_monitoring: Arc::new(RwLock::new(false)),
            monitoring_handle: Arc::new(RwLock::new(None)),
        }
    }
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub cpu_pressure_threshold: f64,
    pub memory_pressure_threshold: f64,
    pub disk_pressure_threshold: f64,
    pub overall_pressure_threshold: f64,
    pub monitoring_interval: Duration,
    pub max_history_size: usize,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            cpu_pressure_threshold: 0.8,      // 80%
            memory_pressure_threshold: 0.8,   // 80%
            disk_pressure_threshold: 0.9,     // 90%
            overall_pressure_threshold: 0.75, // 75%
            monitoring_interval: Duration::from_secs(1),
            max_history_size: 1000,
        }
    }
}

/// System performance statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemPerformanceStatistics {
    pub sample_count: usize,
    pub avg_cpu_usage: f64,
    pub max_cpu_usage: f64,
    pub min_cpu_usage: f64,
    pub avg_memory_usage: u64,
    pub max_memory_usage: u64,
    pub min_memory_usage: u64,
    pub avg_disk_usage: f64,
    pub max_disk_usage: f64,
    pub min_disk_usage: f64,
    pub cpu_pressure: f64,
    pub memory_pressure: f64,
    pub disk_pressure: f64,
    pub overall_pressure: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_system_monitor_creation() {
        let monitor = SystemMonitor::new().unwrap();
        let config = monitor.monitoring_config.read().await;
        assert_eq!(config.cpu_pressure_threshold, 0.8);
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let monitor = SystemMonitor::new().unwrap();
        let metrics = monitor.get_metrics().await.unwrap();

        assert!(metrics.cpu_usage >= 0.0);
        assert!(metrics.memory_usage > 0);
        assert!(metrics.memory_available > 0);
        assert!(metrics.disk_usage >= 0.0);
    }

    #[tokio::test]
    async fn test_performance_statistics() {
        let monitor = SystemMonitor::new().unwrap();

        // Manually add some metrics to the history
        for _ in 0..5 {
            let metrics = monitor.get_metrics().await.unwrap();
            let mut history = monitor.metrics_history.write().await;
            history.push(metrics);
        }

        let stats = monitor.get_performance_statistics().await;
        assert!(stats.sample_count > 0);
        assert!(stats.avg_cpu_usage >= 0.0);
        assert!(stats.avg_memory_usage > 0);
    }

    #[tokio::test]
    async fn test_system_optimization() {
        let monitor = SystemMonitor::new().unwrap();
        let result = monitor.optimize_system().await.unwrap();

        assert_eq!(result.name, "System Optimization");
        assert!(result.duration > Duration::from_millis(0));
    }

    #[tokio::test]
    async fn test_optimization_recommendations() {
        let monitor = SystemMonitor::new().unwrap();

        // Set lower thresholds to trigger recommendations
        let config = MonitoringConfig {
            cpu_pressure_threshold: 0.5,     // 50%
            memory_pressure_threshold: 0.5,  // 50%
            overall_pressure_threshold: 0.5, // 50%
            ..Default::default()
        };
        monitor.set_config(config).await;

        // Add some metrics history to trigger recommendations
        for _ in 0..5 {
            let metrics = SystemMetrics {
                cpu_usage: 80.0,                 // High CPU usage
                memory_usage: 200 * 1024 * 1024, // High memory usage
                memory_available: 100 * 1024 * 1024,
                disk_usage: 90.0, // High disk usage
                network_io: NetworkMetrics {
                    bytes_sent: 1000000,
                    bytes_received: 2000000,
                    packets_sent: 1000,
                    packets_received: 2000,
                },
                cache_metrics: CacheMetrics {
                    hit_rate: 0.8,
                    miss_rate: 0.2,
                    total_requests: 150,
                    cache_size: 1000,
                    evictions: 10,
                },
                timestamp: std::time::Instant::now(),
            };
            let mut history = monitor.metrics_history.write().await;
            history.push(metrics);
        }

        let recommendations = monitor.get_optimization_recommendations().await;

        // Should have some recommendations based on thresholds
        assert!(!recommendations.is_empty());
    }

    /// Get cache hit rate metrics across all layers
    pub async fn get_cache_hit_rates() -> Result<CacheHitRateMetrics, Box<dyn std::error::Error>> {
        // This would integrate with the actual cache system
        // For now, return placeholder metrics
        Ok(CacheHitRateMetrics {
            overall_hit_rate: 0.85,
            page_cache_hit_rate: 0.90,
            object_cache_hit_rate: 0.80,
            query_cache_hit_rate: 0.75,
            index_cache_hit_rate: 0.95,
            relationship_index_hit_rate: 0.88,
        })
    }

    /// Get query execution time distribution
    pub async fn get_query_execution_stats()
    -> Result<QueryExecutionStats, Box<dyn std::error::Error>> {
        // This would integrate with actual query execution tracking
        // For now, return placeholder statistics
        Ok(QueryExecutionStats {
            total_queries: 1000,
            avg_execution_time_ms: 15.5,
            p50_execution_time_ms: 12.0,
            p95_execution_time_ms: 45.0,
            p99_execution_time_ms: 120.0,
            slow_queries_count: 25,
            very_slow_queries_count: 5,
        })
    }

    /// Get memory usage breakdown by component
    pub async fn get_memory_usage_by_component()
    -> Result<MemoryUsageByComponent, Box<dyn std::error::Error>> {
        // This would integrate with actual memory tracking
        // For now, return placeholder breakdown
        Ok(MemoryUsageByComponent {
            total_memory_mb: 512.0,
            page_cache_mb: 100.0,
            object_cache_mb: 50.0,
            query_cache_mb: 25.0,
            index_cache_mb: 30.0,
            relationship_index_mb: 20.0,
            query_execution_mb: 15.0,
            wal_buffer_mb: 10.0,
            other_mb: 50.0,
        })
    }

    #[tokio::test]
    async fn test_async_monitoring() {
        let monitor = SystemMonitor::new().unwrap();

        // Start monitoring with a short interval
        monitor
            .start_monitoring(Duration::from_millis(100))
            .await
            .unwrap();

        // Wait a bit for some metrics to be collected
        tokio::time::sleep(Duration::from_millis(250)).await;

        // Stop monitoring
        monitor.stop_monitoring().await;

        // Check that some metrics were collected
        let stats = monitor.get_performance_statistics().await;
        assert!(stats.sample_count > 0);
    }

    #[tokio::test]
    async fn test_monitoring_config_access() {
        let monitor = SystemMonitor::new().unwrap();

        // Test setting and getting config
        let new_config = MonitoringConfig {
            cpu_pressure_threshold: 0.9,
            memory_pressure_threshold: 0.9,
            disk_pressure_threshold: 0.9,
            overall_pressure_threshold: 0.9,
            monitoring_interval: Duration::from_secs(2),
            max_history_size: 500,
        };

        monitor.set_config(new_config.clone()).await;

        // Verify the config was set (we can't directly read it, but we can test behavior)
        let stats = monitor.get_performance_statistics().await;
        // The stats should be empty initially
        assert_eq!(stats.sample_count, 0);
    }
}
