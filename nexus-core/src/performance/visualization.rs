//! Performance visualization utilities
//!
//! Provides tools for visualizing performance data including charts,
//! graphs, and performance dashboards.

use crate::performance::{
    CacheMetrics, QueryProfile, SystemMetrics
};
use crate::performance::benchmarking::BenchmarkResult;
use crate::performance::memory::MemoryStatistics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Performance visualization engine
pub struct PerformanceVisualizer {
    chart_config: ChartConfig,
    dashboard_config: DashboardConfig,
}

impl PerformanceVisualizer {
    /// Create a new performance visualizer
    pub fn new(config: VisualizationConfig) -> Self {
        Self {
            chart_config: config.chart_config,
            dashboard_config: config.dashboard_config,
        }
    }

    /// Generate performance dashboard data
    pub async fn generate_dashboard(
        &self,
        system_metrics: &[SystemMetrics],
        memory_stats: &[MemoryStatistics],
        cache_metrics: &[CacheMetrics],
        query_profiles: &[QueryProfile],
        benchmark_results: &[BenchmarkResult],
    ) -> PerformanceDashboard {
        let mut dashboard = PerformanceDashboard::default();

        // System metrics charts
        dashboard.system_charts = self.generate_system_charts(system_metrics).await;

        // Memory charts
        dashboard.memory_charts = self.generate_memory_charts(memory_stats).await;

        // Cache charts
        dashboard.cache_charts = self.generate_cache_charts(cache_metrics).await;

        // Query performance charts
        dashboard.query_charts = self.generate_query_charts(query_profiles).await;

        // Benchmark charts
        dashboard.benchmark_charts = self.generate_benchmark_charts(benchmark_results).await;

        // Performance summary
        dashboard.summary = self.generate_performance_summary(
            system_metrics,
            memory_stats,
            cache_metrics,
            query_profiles,
            benchmark_results,
        ).await;

        dashboard
    }

    /// Generate system performance charts
    async fn generate_system_charts(&self, metrics: &[SystemMetrics]) -> Vec<Chart> {
        let mut charts = Vec::new();

        if metrics.is_empty() {
            return charts;
        }

        // CPU usage chart
        let cpu_data = metrics.iter()
            .map(|m| ChartDataPoint {
                timestamp: m.timestamp,
                value: m.cpu_usage,
                label: None,
            })
            .collect();

        charts.push(Chart {
            title: "CPU Usage".to_string(),
            chart_type: ChartType::Line,
            data: cpu_data,
            x_axis_label: "Time".to_string(),
            y_axis_label: "CPU Usage (%)".to_string(),
            color: "#FF6B6B".to_string(),
        });

        // Memory usage chart
        let memory_data = metrics.iter()
            .map(|m| ChartDataPoint {
                timestamp: m.timestamp,
                value: m.memory_usage as f64 / 1024.0 / 1024.0, // Convert to MB
                label: None,
            })
            .collect();

        charts.push(Chart {
            title: "Memory Usage".to_string(),
            chart_type: ChartType::Line,
            data: memory_data,
            x_axis_label: "Time".to_string(),
            y_axis_label: "Memory Usage (MB)".to_string(),
            color: "#4ECDC4".to_string(),
        });

        // Disk usage chart
        let disk_data = metrics.iter()
            .map(|m| ChartDataPoint {
                timestamp: m.timestamp,
                value: m.disk_usage,
                label: None,
            })
            .collect();

        charts.push(Chart {
            title: "Disk Usage".to_string(),
            chart_type: ChartType::Line,
            data: disk_data,
            x_axis_label: "Time".to_string(),
            y_axis_label: "Disk Usage (%)".to_string(),
            color: "#45B7D1".to_string(),
        });

        charts
    }

    /// Generate memory performance charts
    async fn generate_memory_charts(&self, stats: &[MemoryStatistics]) -> Vec<Chart> {
        let mut charts = Vec::new();

        if stats.is_empty() {
            return charts;
        }

        // Memory pressure chart
        let pressure_data = stats.iter()
            .enumerate()
            .map(|(i, s)| ChartDataPoint {
                timestamp: Instant::now(), // Placeholder
                value: s.memory_pressure * 100.0,
                label: Some(format!("Sample {}", i)),
            })
            .collect();

        charts.push(Chart {
            title: "Memory Pressure".to_string(),
            chart_type: ChartType::Bar,
            data: pressure_data,
            x_axis_label: "Sample".to_string(),
            y_axis_label: "Memory Pressure (%)".to_string(),
            color: "#FFA07A".to_string(),
        });

        // Memory usage distribution
        let mut distribution_data = Vec::new();
        if let Some(last_stat) = stats.last() {
            distribution_data.push(ChartDataPoint {
                timestamp: Instant::now(),
                value: last_stat.avg_heap_memory as f64 / 1024.0 / 1024.0,
                label: Some("Heap".to_string()),
            });
            distribution_data.push(ChartDataPoint {
                timestamp: Instant::now(),
                value: last_stat.avg_cache_memory as f64 / 1024.0 / 1024.0,
                label: Some("Cache".to_string()),
            });
        }

        charts.push(Chart {
            title: "Memory Distribution".to_string(),
            chart_type: ChartType::Pie,
            data: distribution_data,
            x_axis_label: "Memory Type".to_string(),
            y_axis_label: "Memory Usage (MB)".to_string(),
            color: "#98D8C8".to_string(),
        });

        charts
    }

    /// Generate cache performance charts
    async fn generate_cache_charts(&self, metrics: &[CacheMetrics]) -> Vec<Chart> {
        let mut charts = Vec::new();

        if metrics.is_empty() {
            return charts;
        }

        // Cache hit rate chart
        let hit_rate_data = metrics.iter()
            .enumerate()
            .map(|(i, m)| ChartDataPoint {
                timestamp: Instant::now(), // Placeholder
                value: m.hit_rate * 100.0,
                label: Some(format!("Sample {}", i)),
            })
            .collect();

        charts.push(Chart {
            title: "Cache Hit Rate".to_string(),
            chart_type: ChartType::Line,
            data: hit_rate_data,
            x_axis_label: "Sample".to_string(),
            y_axis_label: "Hit Rate (%)".to_string(),
            color: "#6BCF7F".to_string(),
        });

        // Cache size chart
        let size_data = metrics.iter()
            .enumerate()
            .map(|(i, m)| ChartDataPoint {
                timestamp: Instant::now(), // Placeholder
                value: m.cache_size as f64 / 1024.0 / 1024.0, // Convert to MB
                label: Some(format!("Sample {}", i)),
            })
            .collect();

        charts.push(Chart {
            title: "Cache Size".to_string(),
            chart_type: ChartType::Area,
            data: size_data,
            x_axis_label: "Sample".to_string(),
            y_axis_label: "Cache Size (MB)".to_string(),
            color: "#4D96FF".to_string(),
        });

        charts
    }

    /// Generate query performance charts
    async fn generate_query_charts(&self, profiles: &[QueryProfile]) -> Vec<Chart> {
        let mut charts = Vec::new();

        if profiles.is_empty() {
            return charts;
        }

        // Query execution time distribution
        let mut time_ranges = HashMap::new();
        for profile in profiles {
            let time_ms = profile.execution_time.as_millis() as f64;
            let range = if time_ms < 10.0 {
                "0-10ms"
            } else if time_ms < 50.0 {
                "10-50ms"
            } else if time_ms < 100.0 {
                "50-100ms"
            } else if time_ms < 500.0 {
                "100-500ms"
            } else {
                "500ms+"
            };
            *time_ranges.entry(range.to_string()).or_insert(0) += 1;
        }

        let mut distribution_data = Vec::new();
        for (range, count) in time_ranges {
            distribution_data.push(ChartDataPoint {
                timestamp: Instant::now(),
                value: count as f64,
                label: Some(range),
            });
        }

        charts.push(Chart {
            title: "Query Execution Time Distribution".to_string(),
            chart_type: ChartType::Bar,
            data: distribution_data,
            x_axis_label: "Time Range".to_string(),
            y_axis_label: "Query Count".to_string(),
            color: "#FF9F43".to_string(),
        });

        // Memory usage per query
        let memory_data = profiles.iter()
            .enumerate()
            .map(|(i, p)| ChartDataPoint {
                timestamp: Instant::now(),
                value: p.memory_usage as f64 / 1024.0 / 1024.0, // Convert to MB
                label: Some(format!("Query {}", i)),
            })
            .collect();

        charts.push(Chart {
            title: "Query Memory Usage".to_string(),
            chart_type: ChartType::Scatter,
            data: memory_data,
            x_axis_label: "Query".to_string(),
            y_axis_label: "Memory Usage (MB)".to_string(),
            color: "#A55EEA".to_string(),
        });

        charts
    }

    /// Generate benchmark performance charts
    async fn generate_benchmark_charts(&self, results: &[BenchmarkResult]) -> Vec<Chart> {
        let mut charts = Vec::new();

        if results.is_empty() {
            return charts;
        }

        // Throughput comparison
        let throughput_data = results.iter()
            .map(|r| ChartDataPoint {
                timestamp: Instant::now(),
                value: r.throughput,
                label: Some(r.name.clone()),
            })
            .collect();

        charts.push(Chart {
            title: "Benchmark Throughput".to_string(),
            chart_type: ChartType::Bar,
            data: throughput_data,
            x_axis_label: "Benchmark".to_string(),
            y_axis_label: "Throughput (ops/sec)".to_string(),
            color: "#26C6DA".to_string(),
        });

        // Latency comparison
        let latency_data = results.iter()
            .map(|r| ChartDataPoint {
                timestamp: Instant::now(),
                value: r.avg_duration.as_millis() as f64,
                label: Some(r.name.clone()),
            })
            .collect();

        charts.push(Chart {
            title: "Benchmark Latency".to_string(),
            chart_type: ChartType::Bar,
            data: latency_data,
            x_axis_label: "Benchmark".to_string(),
            y_axis_label: "Latency (ms)".to_string(),
            color: "#FF7043".to_string(),
        });

        charts
    }

    /// Generate performance summary
    async fn generate_performance_summary(
        &self,
        system_metrics: &[SystemMetrics],
        memory_stats: &[MemoryStatistics],
        cache_metrics: &[CacheMetrics],
        query_profiles: &[QueryProfile],
        benchmark_results: &[BenchmarkResult],
    ) -> PerformanceSummary {
        let mut summary = PerformanceSummary::default();

        // System summary
        if let Some(latest_system) = system_metrics.last() {
            summary.cpu_usage = latest_system.cpu_usage;
            summary.memory_usage = latest_system.memory_usage;
            summary.disk_usage = latest_system.disk_usage;
        }

        // Memory summary
        if let Some(latest_memory) = memory_stats.last() {
            summary.memory_pressure = latest_memory.memory_pressure;
            summary.peak_memory = latest_memory.peak_memory;
        }

        // Cache summary
        if let Some(latest_cache) = cache_metrics.last() {
            summary.cache_hit_rate = latest_cache.hit_rate;
            summary.cache_size = latest_cache.cache_size;
        }

        // Query summary
        if !query_profiles.is_empty() {
            let avg_execution_time: Duration = query_profiles.iter()
                .map(|p| p.execution_time)
                .sum::<Duration>() / query_profiles.len() as u32;
            summary.avg_query_time = avg_execution_time;

            let slow_queries = query_profiles.iter()
                .filter(|p| p.execution_time > Duration::from_millis(100))
                .count();
            summary.slow_queries = slow_queries;
        }

        // Benchmark summary
        if !benchmark_results.is_empty() {
            let avg_throughput: f64 = benchmark_results.iter()
                .map(|r| r.throughput)
                .sum::<f64>() / benchmark_results.len() as f64;
            summary.avg_throughput = avg_throughput;

            let avg_latency: Duration = benchmark_results.iter()
                .map(|r| r.avg_duration)
                .sum::<Duration>() / benchmark_results.len() as u32;
            summary.avg_latency = avg_latency;
        }

        // Calculate overall health score
        summary.health_score = self.calculate_health_score(&summary);

        summary
    }

    /// Calculate overall health score (0-100)
    fn calculate_health_score(&self, summary: &PerformanceSummary) -> f64 {
        let mut score = 100.0;

        // CPU penalty
        if summary.cpu_usage > 90.0 {
            score -= 20.0;
        } else if summary.cpu_usage > 80.0 {
            score -= 10.0;
        }

        // Memory penalty
        if summary.memory_pressure > 0.9 {
            score -= 25.0;
        } else if summary.memory_pressure > 0.8 {
            score -= 15.0;
        }

        // Cache penalty
        if summary.cache_hit_rate < 0.7 {
            score -= 15.0;
        } else if summary.cache_hit_rate < 0.8 {
            score -= 10.0;
        }

        // Query performance penalty
        if summary.avg_query_time > Duration::from_millis(1000) {
            score -= 20.0;
        } else if summary.avg_query_time > Duration::from_millis(500) {
            score -= 10.0;
        }

        // Slow queries penalty
        if summary.slow_queries > 10 {
            score -= 15.0;
        } else if summary.slow_queries > 5 {
            score -= 10.0;
        }

        score.max(0.0_f64)
    }

    /// Export dashboard as JSON
    pub async fn export_dashboard_json(&self, dashboard: &PerformanceDashboard) -> String {
        serde_json::to_string_pretty(dashboard).unwrap_or_else(|_| "{}".to_string())
    }

    /// Export dashboard as HTML
    pub async fn export_dashboard_html(&self, dashboard: &PerformanceDashboard) -> String {
        let mut html = String::new();
        
        html.push_str("<!DOCTYPE html>\n");
        html.push_str("<html>\n<head>\n");
        html.push_str("<title>Nexus Performance Dashboard</title>\n");
        html.push_str("<script src=\"https://cdn.jsdelivr.net/npm/chart.js\"></script>\n");
        html.push_str("</head>\n<body>\n");
        html.push_str("<h1>Nexus Performance Dashboard</h1>\n");

        // System charts
        html.push_str("<h2>System Performance</h2>\n");
        for chart in &dashboard.system_charts {
            html.push_str(&format!("<div><h3>{}</h3><canvas id=\"{}\"></canvas></div>\n", 
                chart.title, chart.title.replace(" ", "_").to_lowercase()));
        }

        // Memory charts
        html.push_str("<h2>Memory Performance</h2>\n");
        for chart in &dashboard.memory_charts {
            html.push_str(&format!("<div><h3>{}</h3><canvas id=\"{}\"></canvas></div>\n", 
                chart.title, chart.title.replace(" ", "_").to_lowercase()));
        }

        // Cache charts
        html.push_str("<h2>Cache Performance</h2>\n");
        for chart in &dashboard.cache_charts {
            html.push_str(&format!("<div><h3>{}</h3><canvas id=\"{}\"></canvas></div>\n", 
                chart.title, chart.title.replace(" ", "_").to_lowercase()));
        }

        // Query charts
        html.push_str("<h2>Query Performance</h2>\n");
        for chart in &dashboard.query_charts {
            html.push_str(&format!("<div><h3>{}</h3><canvas id=\"{}\"></canvas></div>\n", 
                chart.title, chart.title.replace(" ", "_").to_lowercase()));
        }

        // Benchmark charts
        html.push_str("<h2>Benchmark Performance</h2>\n");
        for chart in &dashboard.benchmark_charts {
            html.push_str(&format!("<div><h3>{}</h3><canvas id=\"{}\"></canvas></div>\n", 
                chart.title, chart.title.replace(" ", "_").to_lowercase()));
        }

        html.push_str("</body>\n</html>");
        html
    }
}

impl Default for PerformanceVisualizer {
    fn default() -> Self {
        Self::new(VisualizationConfig::default())
    }
}

/// Visualization configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationConfig {
    pub chart_config: ChartConfig,
    pub dashboard_config: DashboardConfig,
}

impl Default for VisualizationConfig {
    fn default() -> Self {
        Self {
            chart_config: ChartConfig::default(),
            dashboard_config: DashboardConfig::default(),
        }
    }
}

/// Chart configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartConfig {
    pub width: u32,
    pub height: u32,
    pub show_legend: bool,
    pub show_grid: bool,
    pub animation_duration: u32,
}

impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            width: 800,
            height: 400,
            show_legend: true,
            show_grid: true,
            animation_duration: 1000,
        }
    }
}

/// Dashboard configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    pub refresh_interval: Duration,
    pub max_data_points: usize,
    pub theme: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(30),
            max_data_points: 1000,
            theme: "light".to_string(),
        }
    }
}

/// Performance dashboard
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceDashboard {
    pub system_charts: Vec<Chart>,
    pub memory_charts: Vec<Chart>,
    pub cache_charts: Vec<Chart>,
    pub query_charts: Vec<Chart>,
    pub benchmark_charts: Vec<Chart>,
    pub summary: PerformanceSummary,
}

/// Chart data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chart {
    pub title: String,
    pub chart_type: ChartType,
    pub data: Vec<ChartDataPoint>,
    pub x_axis_label: String,
    pub y_axis_label: String,
    pub color: String,
}

/// Chart data point
#[derive(Debug, Clone)]
pub struct ChartDataPoint {
    pub timestamp: Instant,
    pub value: f64,
    pub label: Option<String>,
}

/// Chart types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChartType {
    Line,
    Bar,
    Area,
    Pie,
    Scatter,
}

/// Performance summary
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub disk_usage: f64,
    pub memory_pressure: f64,
    pub peak_memory: u64,
    pub cache_hit_rate: f64,
    pub cache_size: u64,
    pub avg_query_time: Duration,
    pub slow_queries: usize,
    pub avg_throughput: f64,
    pub avg_latency: Duration,
    pub health_score: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_visualizer_creation() {
        let config = VisualizationConfig::default();
        let visualizer = PerformanceVisualizer::new(config);
        assert_eq!(visualizer.chart_config.width, 800);
        assert_eq!(visualizer.chart_config.height, 400);
    }

    #[tokio::test]
    async fn test_dashboard_generation() {
        let visualizer = PerformanceVisualizer::default();
        
        let system_metrics = vec![
            SystemMetrics {
                cpu_usage: 50.0,
                memory_usage: 1024 * 1024 * 1024, // 1GB
                memory_available: 1024 * 1024 * 1024, // 1GB
                disk_usage: 60.0,
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
            },
        ];

        let memory_stats = vec![MemoryStatistics::default()];
        let cache_metrics = vec![CacheMetrics {
            hit_rate: 0.8,
            miss_rate: 0.2,
            total_requests: 1000,
            cache_size: 1024,
            evictions: 5,
        }];
        let query_profiles = vec![];
        let benchmark_results = vec![];

        let dashboard = visualizer.generate_dashboard(
            &system_metrics,
            &memory_stats,
            &cache_metrics,
            &query_profiles,
            &benchmark_results,
        ).await;

        assert!(!dashboard.system_charts.is_empty());
        assert!(!dashboard.memory_charts.is_empty());
        assert!(!dashboard.cache_charts.is_empty());
        assert!(dashboard.summary.cpu_usage > 0.0);
    }

    #[tokio::test]
    async fn test_health_score_calculation() {
        let visualizer = PerformanceVisualizer::default();
        
        let summary = PerformanceSummary {
            cpu_usage: 95.0, // High CPU
            memory_usage: 1024 * 1024 * 1024,
            disk_usage: 50.0,
            memory_pressure: 0.95, // High pressure
            peak_memory: 1024 * 1024 * 1024,
            cache_hit_rate: 0.6, // Low hit rate
            cache_size: 1024,
            avg_query_time: Duration::from_millis(1500), // Slow queries
            slow_queries: 15, // Many slow queries
            avg_throughput: 100.0,
            avg_latency: Duration::from_millis(100),
            health_score: 0.0, // Will be calculated
        };

        let health_score = visualizer.calculate_health_score(&summary);
        assert!(health_score < 50.0); // Should be low due to poor performance
    }

    #[tokio::test]
    async fn test_export_dashboard_json() {
        let visualizer = PerformanceVisualizer::default();
        let dashboard = PerformanceDashboard::default();
        
        let json = visualizer.export_dashboard_json(&dashboard).await;
        assert!(!json.is_empty());
        assert!(json.contains("system_charts"));
    }

    #[tokio::test]
    async fn test_export_dashboard_html() {
        let visualizer = PerformanceVisualizer::default();
        let dashboard = PerformanceDashboard::default();
        
        let html = visualizer.export_dashboard_html(&dashboard).await;
        assert!(!html.is_empty());
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Nexus Performance Dashboard"));
    }
}
