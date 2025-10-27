//! Performance tuning recommendations engine
//!
//! Provides AI-powered performance recommendations based on system metrics,
//! query patterns, and performance data analysis.

use crate::performance::benchmarking::BenchmarkResult;
use crate::performance::memory::MemoryStatistics;
use crate::performance::{
    CacheMetrics, Effort, Impact, OptimizationRecommendation, Priority, QueryProfile, SystemMetrics,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Performance recommendations engine
pub struct PerformanceRecommendations {
    recommendation_rules: Vec<RecommendationRule>,
    performance_history: HashMap<String, Vec<PerformanceSnapshot>>,
    config: RecommendationsConfig,
}

impl PerformanceRecommendations {
    /// Create a new recommendations engine
    pub fn new(config: RecommendationsConfig) -> Self {
        Self {
            recommendation_rules: Self::create_default_rules(),
            performance_history: HashMap::new(),
            config,
        }
    }

    /// Generate comprehensive performance recommendations
    pub async fn generate_recommendations(
        &mut self,
        system_metrics: &SystemMetrics,
        memory_stats: &MemoryStatistics,
        cache_metrics: &CacheMetrics,
        query_profiles: &[QueryProfile],
        benchmark_results: &[BenchmarkResult],
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Analyze system metrics
        recommendations.extend(self.analyze_system_metrics(system_metrics).await);

        // Analyze memory usage
        recommendations.extend(self.analyze_memory_usage(memory_stats).await);

        // Analyze cache performance
        recommendations.extend(self.analyze_cache_performance(cache_metrics).await);

        // Analyze query performance
        recommendations.extend(self.analyze_query_performance(query_profiles).await);

        // Analyze benchmark results
        recommendations.extend(self.analyze_benchmark_results(benchmark_results).await);

        // Apply recommendation rules
        recommendations = self.apply_recommendation_rules(recommendations).await;

        // Store performance snapshot
        self.store_performance_snapshot(system_metrics, memory_stats, cache_metrics)
            .await;

        // Sort by priority and impact
        recommendations.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(b.impact.cmp(&a.impact))
                .then(a.effort.cmp(&b.effort))
        });

        recommendations
    }

    /// Analyze system metrics for recommendations
    async fn analyze_system_metrics(
        &self,
        metrics: &SystemMetrics,
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // CPU usage analysis
        if metrics.cpu_usage > self.config.cpu_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "CPU Performance".to_string(),
                priority: Priority::High,
                description: format!("High CPU usage: {:.1}%", metrics.cpu_usage),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Consider CPU optimization, parallel processing, or hardware upgrade"
                        .to_string(),
            });
        }

        // Memory usage analysis
        if metrics.memory_usage > self.config.memory_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Performance".to_string(),
                priority: Priority::High,
                description: format!("High memory usage: {:.1}MB", metrics.memory_usage as f64 / 1024.0 / 1024.0),
                impact: Impact::High,
                effort: Effort::High,
                implementation: "Optimize memory allocation, implement memory pooling, or increase available memory".to_string(),
            });
        }

        // Disk usage analysis
        if metrics.disk_usage > self.config.disk_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "Storage Performance".to_string(),
                priority: Priority::Medium,
                description: format!("High disk usage: {:.1}%", metrics.disk_usage),
                impact: Impact::Medium,
                effort: Effort::Medium,
                implementation:
                    "Optimize storage usage, implement data compression, or add more storage"
                        .to_string(),
            });
        }

        recommendations
    }

    /// Analyze memory usage for recommendations
    async fn analyze_memory_usage(
        &self,
        stats: &MemoryStatistics,
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Memory pressure analysis
        if stats.memory_pressure > self.config.memory_pressure_threshold {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Pressure".to_string(),
                priority: Priority::Critical,
                description: format!(
                    "High memory pressure: {:.1}%",
                    stats.memory_pressure * 100.0
                ),
                impact: Impact::VeryHigh,
                effort: Effort::High,
                implementation:
                    "Implement aggressive memory management, reduce memory usage, or add swap space"
                        .to_string(),
            });
        }

        // Memory fragmentation analysis
        if stats.peak_memory > stats.min_memory * 2 {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Fragmentation".to_string(),
                priority: Priority::Medium,
                description: "High memory fragmentation detected".to_string(),
                impact: Impact::Medium,
                effort: Effort::High,
                implementation:
                    "Implement memory compaction or object pooling to reduce fragmentation"
                        .to_string(),
            });
        }

        recommendations
    }

    /// Analyze cache performance for recommendations
    async fn analyze_cache_performance(
        &self,
        metrics: &CacheMetrics,
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        // Cache hit rate analysis
        if metrics.hit_rate < self.config.min_cache_hit_rate {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Performance".to_string(),
                priority: Priority::High,
                description: format!("Low cache hit rate: {:.1}%", metrics.hit_rate * 100.0),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Increase cache size, optimize eviction policy, or improve data locality"
                        .to_string(),
            });
        }

        // Cache size analysis
        if metrics.cache_size > self.config.max_cache_size {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Size".to_string(),
                priority: Priority::Medium,
                description: format!(
                    "Large cache size: {:.1}MB",
                    metrics.cache_size as f64 / 1024.0 / 1024.0
                ),
                impact: Impact::Medium,
                effort: Effort::Low,
                implementation: "Consider cache partitioning or implementing cache tiers"
                    .to_string(),
            });
        }

        // Cache eviction analysis
        if metrics.evictions > self.config.max_evictions {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Evictions".to_string(),
                priority: Priority::Medium,
                description: format!("High cache eviction rate: {}", metrics.evictions),
                impact: Impact::Medium,
                effort: Effort::Low,
                implementation: "Optimize eviction policy or increase cache size".to_string(),
            });
        }

        recommendations
    }

    /// Analyze query performance for recommendations
    async fn analyze_query_performance(
        &self,
        profiles: &[QueryProfile],
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        if profiles.is_empty() {
            return recommendations;
        }

        // Slow query analysis
        let slow_queries: Vec<_> = profiles
            .iter()
            .filter(|p| p.execution_time > self.config.slow_query_threshold)
            .collect();

        if !slow_queries.is_empty() {
            recommendations.push(OptimizationRecommendation {
                category: "Query Performance".to_string(),
                priority: Priority::High,
                description: format!("{} slow queries detected", slow_queries.len()),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation: "Add indexes, optimize query patterns, or use query hints"
                    .to_string(),
            });
        }

        // Memory intensive query analysis
        let memory_intensive: Vec<_> = profiles
            .iter()
            .filter(|p| p.memory_usage > self.config.memory_intensive_threshold)
            .collect();

        if !memory_intensive.is_empty() {
            recommendations.push(OptimizationRecommendation {
                category: "Query Memory Usage".to_string(),
                priority: Priority::Medium,
                description: format!(
                    "{} memory intensive queries detected",
                    memory_intensive.len()
                ),
                impact: Impact::Medium,
                effort: Effort::High,
                implementation: "Optimize query execution plans or implement result streaming"
                    .to_string(),
            });
        }

        // CPU intensive query analysis
        let cpu_intensive: Vec<_> = profiles
            .iter()
            .filter(|p| p.cpu_usage > self.config.cpu_intensive_threshold)
            .collect();

        if !cpu_intensive.is_empty() {
            recommendations.push(OptimizationRecommendation {
                category: "Query CPU Usage".to_string(),
                priority: Priority::Medium,
                description: format!("{} CPU intensive queries detected", cpu_intensive.len()),
                impact: Impact::Medium,
                effort: Effort::High,
                implementation: "Implement parallel processing or optimize algorithms".to_string(),
            });
        }

        recommendations
    }

    /// Analyze benchmark results for recommendations
    async fn analyze_benchmark_results(
        &self,
        results: &[BenchmarkResult],
    ) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();

        if results.is_empty() {
            return recommendations;
        }

        // Throughput analysis
        let avg_throughput: f64 =
            results.iter().map(|r| r.throughput).sum::<f64>() / results.len() as f64;
        if avg_throughput < self.config.min_throughput {
            recommendations.push(OptimizationRecommendation {
                category: "System Throughput".to_string(),
                priority: Priority::High,
                description: format!("Low average throughput: {:.2} ops/sec", avg_throughput),
                impact: Impact::High,
                effort: Effort::High,
                implementation:
                    "Optimize system configuration, increase resources, or improve algorithms"
                        .to_string(),
            });
        }

        // Latency analysis
        let avg_latency: Duration =
            results.iter().map(|r| r.avg_duration).sum::<Duration>() / results.len() as u32;

        if avg_latency > self.config.max_latency {
            recommendations.push(OptimizationRecommendation {
                category: "System Latency".to_string(),
                priority: Priority::High,
                description: format!("High average latency: {:?}", avg_latency),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Optimize I/O operations, reduce network latency, or improve caching"
                        .to_string(),
            });
        }

        // Memory usage analysis
        let avg_memory: u64 =
            results.iter().map(|r| r.memory_usage).sum::<u64>() / results.len() as u64;
        if avg_memory > self.config.max_memory_per_operation {
            recommendations.push(OptimizationRecommendation {
                category: "Operation Memory Usage".to_string(),
                priority: Priority::Medium,
                description: format!(
                    "High memory usage per operation: {:.1}MB",
                    avg_memory as f64 / 1024.0 / 1024.0
                ),
                impact: Impact::Medium,
                effort: Effort::High,
                implementation: "Optimize memory allocation patterns or implement memory pooling"
                    .to_string(),
            });
        }

        recommendations
    }

    /// Apply recommendation rules to filter and prioritize recommendations
    async fn apply_recommendation_rules(
        &self,
        recommendations: Vec<OptimizationRecommendation>,
    ) -> Vec<OptimizationRecommendation> {
        let mut filtered_recommendations = Vec::new();

        for recommendation in recommendations {
            let mut should_include = true;
            let mut modified_recommendation = recommendation.clone();

            // Apply rules
            for rule in &self.recommendation_rules {
                if rule.matches(&modified_recommendation) {
                    match rule.action {
                        RuleAction::Include => {
                            // Explicitly include this recommendation
                            should_include = true;
                        }
                        RuleAction::Exclude => {
                            should_include = false;
                            break;
                        }
                        RuleAction::Modify => {
                            // Apply modifications but don't exclude
                            if let Some(ref modifier) = rule.modifier {
                                modified_recommendation = modifier(modified_recommendation);
                            }
                        }
                    }
                }
            }

            if should_include {
                filtered_recommendations.push(modified_recommendation);
            }
        }

        // Remove duplicates
        filtered_recommendations.sort_by(|a, b| a.category.cmp(&b.category));
        filtered_recommendations
            .dedup_by(|a, b| a.category == b.category && a.description == b.description);

        filtered_recommendations
    }

    /// Store performance snapshot for trend analysis
    async fn store_performance_snapshot(
        &mut self,
        system_metrics: &SystemMetrics,
        memory_stats: &MemoryStatistics,
        cache_metrics: &CacheMetrics,
    ) {
        let snapshot = PerformanceSnapshot {
            timestamp: Instant::now(),
            cpu_usage: system_metrics.cpu_usage,
            memory_usage: system_metrics.memory_usage,
            memory_pressure: memory_stats.memory_pressure,
            cache_hit_rate: cache_metrics.hit_rate,
            cache_size: cache_metrics.cache_size,
        };

        // Store in history
        self.performance_history
            .entry("system".to_string())
            .or_default()
            .push(snapshot);

        // Keep only recent snapshots
        for history in self.performance_history.values_mut() {
            if history.len() > self.config.max_history_size {
                history.drain(0..history.len() - self.config.max_history_size);
            }
        }
    }

    /// Create default recommendation rules
    fn create_default_rules() -> Vec<RecommendationRule> {
        vec![
            RecommendationRule {
                name: "exclude_low_impact".to_string(),
                condition: Box::new(|rec| {
                    rec.impact == Impact::Low && rec.effort == Effort::VeryHigh
                }),
                action: RuleAction::Exclude,
                modifier: None,
            },
            RecommendationRule {
                name: "prioritize_critical".to_string(),
                condition: Box::new(|rec| rec.priority == Priority::Critical),
                action: RuleAction::Modify,
                modifier: Some(Box::new(|mut rec| {
                    rec.priority = Priority::Critical;
                    rec
                })),
            },
        ]
    }

    /// Get performance trends
    pub async fn get_performance_trends(&self) -> PerformanceTrends {
        let mut trends = PerformanceTrends::default();

        if let Some(history) = self.performance_history.get("system") {
            if history.len() >= 2 {
                let recent = &history[history.len() - 1];
                let previous = &history[history.len() - 2];

                trends.cpu_trend = recent.cpu_usage - previous.cpu_usage;
                trends.memory_trend = recent.memory_usage as i64 - previous.memory_usage as i64;
                trends.cache_trend = recent.cache_hit_rate - previous.cache_hit_rate;
            }
        }

        trends
    }
}

impl Default for PerformanceRecommendations {
    fn default() -> Self {
        Self::new(RecommendationsConfig::default())
    }
}

/// Recommendations configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendationsConfig {
    pub cpu_threshold: f64,
    pub memory_threshold: u64,
    pub disk_threshold: f64,
    pub memory_pressure_threshold: f64,
    pub min_cache_hit_rate: f64,
    pub max_cache_size: u64,
    pub max_evictions: u64,
    pub slow_query_threshold: Duration,
    pub memory_intensive_threshold: u64,
    pub cpu_intensive_threshold: f64,
    pub min_throughput: f64,
    pub max_latency: Duration,
    pub max_memory_per_operation: u64,
    pub max_history_size: usize,
}

impl Default for RecommendationsConfig {
    fn default() -> Self {
        Self {
            cpu_threshold: 80.0,
            memory_threshold: 1024 * 1024 * 1024, // 1GB
            disk_threshold: 90.0,
            memory_pressure_threshold: 0.8,
            min_cache_hit_rate: 0.8,
            max_cache_size: 1024 * 1024 * 1024, // 1GB
            max_evictions: 1000,
            slow_query_threshold: Duration::from_millis(100),
            memory_intensive_threshold: 10 * 1024 * 1024, // 10MB
            cpu_intensive_threshold: 80.0,
            min_throughput: 100.0,
            max_latency: Duration::from_millis(1000),
            max_memory_per_operation: 100 * 1024 * 1024, // 100MB
            max_history_size: 1000,
        }
    }
}

/// Recommendation rule
pub struct RecommendationRule {
    pub name: String,
    pub condition: Box<dyn Fn(&OptimizationRecommendation) -> bool + Send + Sync>,
    pub action: RuleAction,
    pub modifier:
        Option<Box<dyn Fn(OptimizationRecommendation) -> OptimizationRecommendation + Send + Sync>>,
}

impl RecommendationRule {
    pub fn matches(&self, recommendation: &OptimizationRecommendation) -> bool {
        (self.condition)(recommendation)
    }

    pub fn modify_recommendation(
        &self,
        recommendation: OptimizationRecommendation,
    ) -> OptimizationRecommendation {
        if let Some(modifier) = &self.modifier {
            modifier(recommendation)
        } else {
            recommendation
        }
    }
}

/// Rule action
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleAction {
    Include,
    Exclude,
    Modify,
}

/// Performance snapshot
#[derive(Debug, Clone)]
pub struct PerformanceSnapshot {
    pub timestamp: Instant,
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub memory_pressure: f64,
    pub cache_hit_rate: f64,
    pub cache_size: u64,
}

/// Performance trends
#[derive(Debug, Clone, Default)]
pub struct PerformanceTrends {
    pub cpu_trend: f64,
    pub memory_trend: i64,
    pub cache_trend: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_recommendations_creation() {
        let config = RecommendationsConfig::default();
        let recommendations = PerformanceRecommendations::new(config);
        assert!(!recommendations.recommendation_rules.is_empty());
    }

    #[tokio::test]
    async fn test_system_metrics_analysis() {
        let config = RecommendationsConfig::default();
        let mut recommendations = PerformanceRecommendations::new(config);

        let system_metrics = SystemMetrics {
            cpu_usage: 90.0,                      // High CPU usage
            memory_usage: 2 * 1024 * 1024 * 1024, // 2GB
            memory_available: 1024 * 1024 * 1024, // 1GB
            disk_usage: 50.0,
            network_io: crate::performance::NetworkMetrics {
                bytes_sent: 1000,
                bytes_received: 2000,
                packets_sent: 10,
                packets_received: 20,
            },
            cache_metrics: crate::performance::CacheMetrics {
                hit_rate: 0.8,
                miss_rate: 0.2,
                total_requests: 1000,
                cache_size: 1024,
                evictions: 5,
            },
            timestamp: Instant::now(),
        };

        let memory_stats = MemoryStatistics {
            avg_total_memory: 2 * 1024 * 1024 * 1024, // 2GB
            peak_memory: 3 * 1024 * 1024 * 1024,      // 3GB
            min_memory: 1024 * 1024 * 1024,           // 1GB
            avg_heap_memory: 1024 * 1024 * 1024,      // 1GB
            avg_cache_memory: 512 * 1024 * 1024,      // 512MB
            memory_pressure: 0.9,                     // High pressure
            sample_count: 10,
        };

        let cache_metrics = CacheMetrics {
            hit_rate: 0.7, // Low hit rate
            miss_rate: 0.3,
            total_requests: 1000,
            cache_size: 2 * 1024 * 1024 * 1024, // 2GB
            evictions: 1500,                    // High evictions
        };

        let recs = recommendations
            .generate_recommendations(&system_metrics, &memory_stats, &cache_metrics, &[], &[])
            .await;

        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.category == "CPU Performance"));
        assert!(recs.iter().any(|r| r.category == "Memory Performance"));
        assert!(recs.iter().any(|r| r.category == "Memory Pressure"));
        assert!(recs.iter().any(|r| r.category == "Cache Performance"));
    }

    #[tokio::test]
    async fn test_query_performance_analysis() {
        let config = RecommendationsConfig::default();
        let mut recommendations = PerformanceRecommendations::new(config);

        let query_profiles = vec![QueryProfile {
            query: "MATCH (n) RETURN n".to_string(),
            execution_time: Duration::from_millis(150), // Slow query
            memory_usage: 15 * 1024 * 1024,             // 15MB - memory intensive
            cpu_usage: 90.0,                            // CPU intensive
            io_operations: 10,
            cache_hits: 8,
            cache_misses: 2,
            recommendations: vec![],
        }];

        let system_metrics = SystemMetrics {
            cpu_usage: 50.0,
            memory_usage: 512 * 1024 * 1024,      // 512MB
            memory_available: 1024 * 1024 * 1024, // 1GB
            disk_usage: 50.0,
            network_io: crate::performance::NetworkMetrics {
                bytes_sent: 1000,
                bytes_received: 2000,
                packets_sent: 10,
                packets_received: 20,
            },
            cache_metrics: crate::performance::CacheMetrics {
                hit_rate: 0.8,
                miss_rate: 0.2,
                total_requests: 1000,
                cache_size: 1024,
                evictions: 5,
            },
            timestamp: Instant::now(),
        };

        let memory_stats = MemoryStatistics::default();
        let cache_metrics = CacheMetrics {
            hit_rate: 0.8,
            miss_rate: 0.2,
            total_requests: 1000,
            cache_size: 1024,
            evictions: 5,
        };

        let recs = recommendations
            .generate_recommendations(
                &system_metrics,
                &memory_stats,
                &cache_metrics,
                &query_profiles,
                &[],
            )
            .await;

        assert!(!recs.is_empty());
        assert!(recs.iter().any(|r| r.category == "Query Performance"));
        assert!(recs.iter().any(|r| r.category == "Query Memory Usage"));
        assert!(recs.iter().any(|r| r.category == "Query CPU Usage"));
    }
}
