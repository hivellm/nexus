//! Performance metrics collection and analysis
//!
//! Provides comprehensive metrics collection, aggregation, and analysis
//! for performance monitoring and optimization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Performance metrics collector
pub struct PerformanceMetrics {
    metrics: RwLock<HashMap<String, MetricValue>>,
    counters: RwLock<HashMap<String, Counter>>,
    histograms: RwLock<HashMap<String, Histogram>>,
    gauges: RwLock<HashMap<String, Gauge>>,
    timers: RwLock<HashMap<String, Timer>>,
}

impl PerformanceMetrics {
    /// Create a new performance metrics collector
    pub fn new() -> Self {
        Self {
            metrics: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
            gauges: RwLock::new(HashMap::new()),
            timers: RwLock::new(HashMap::new()),
        }
    }

    /// Increment a counter
    pub async fn increment_counter(&self, name: &str, value: u64) {
        let mut counters = self.counters.write().await;
        let counter = counters
            .entry(name.to_string())
            .or_insert_with(Counter::new);
        counter.increment(value);
    }

    /// Set a gauge value
    pub async fn set_gauge(&self, name: &str, value: f64) {
        let mut gauges = self.gauges.write().await;
        let gauge = gauges
            .entry(name.to_string())
            .or_insert_with(Gauge::new);
        gauge.set(value);
    }

    /// Record a histogram value
    pub async fn record_histogram(&self, name: &str, value: f64) {
        let mut histograms = self.histograms.write().await;
        let histogram = histograms
            .entry(name.to_string())
            .or_insert_with(Histogram::new);
        histogram.record(value);
    }

    /// Start a timer
    pub async fn start_timer(&self, name: &str) -> TimerHandle {
        let mut timers = self.timers.write().await;
        let timer = timers
            .entry(name.to_string())
            .or_insert_with(Timer::new);
        timer.start()
    }

    /// Record a timer duration
    pub async fn record_timer(&self, name: &str, duration: Duration) {
        let mut timers = self.timers.write().await;
        let timer = timers
            .entry(name.to_string())
            .or_insert_with(Timer::new);
        timer.record(duration);
    }

    /// Get counter value
    pub async fn get_counter(&self, name: &str) -> Option<u64> {
        let counters = self.counters.read().await;
        counters.get(name).map(|c| c.value())
    }

    /// Get gauge value
    pub async fn get_gauge(&self, name: &str) -> Option<f64> {
        let gauges = self.gauges.read().await;
        gauges.get(name).map(|g| g.value())
    }

    /// Get histogram statistics
    pub async fn get_histogram_stats(&self, name: &str) -> Option<HistogramStats> {
        let histograms = self.histograms.read().await;
        histograms.get(name).map(|h| h.stats())
    }

    /// Get timer statistics
    pub async fn get_timer_stats(&self, name: &str) -> Option<TimerStats> {
        let timers = self.timers.read().await;
        timers.get(name).map(|t| t.stats())
    }

    /// Get all metrics as a summary
    pub async fn get_metrics_summary(&self) -> MetricsSummary {
        let counters = self.counters.read().await;
        let gauges = self.gauges.read().await;
        let histograms = self.histograms.read().await;
        let timers = self.timers.read().await;

        let mut counter_values = HashMap::new();
        for (name, counter) in counters.iter() {
            counter_values.insert(name.clone(), counter.value());
        }

        let mut gauge_values = HashMap::new();
        for (name, gauge) in gauges.iter() {
            gauge_values.insert(name.clone(), gauge.value());
        }

        let mut histogram_stats = HashMap::new();
        for (name, histogram) in histograms.iter() {
            histogram_stats.insert(name.clone(), histogram.stats());
        }

        let mut timer_stats = HashMap::new();
        for (name, timer) in timers.iter() {
            timer_stats.insert(name.clone(), timer.stats());
        }

        MetricsSummary {
            counters: counter_values,
            gauges: gauge_values,
            histograms: histogram_stats,
            timers: timer_stats,
            timestamp: Instant::now(),
        }
    }

    /// Clear all metrics
    pub async fn clear_all(&self) {
        let mut counters = self.counters.write().await;
        let mut gauges = self.gauges.write().await;
        let mut histograms = self.histograms.write().await;
        let mut timers = self.timers.write().await;

        counters.clear();
        gauges.clear();
        histograms.clear();
        timers.clear();
    }

    /// Export metrics in Prometheus format
    pub async fn export_prometheus(&self) -> String {
        let mut output = String::new();
        let summary = self.get_metrics_summary().await;

        // Export counters
        for (name, value) in summary.counters {
            output.push_str(&format!("nexus_counter_{} {}\n", name, value));
        }

        // Export gauges
        for (name, value) in summary.gauges {
            output.push_str(&format!("nexus_gauge_{} {}\n", name, value));
        }

        // Export histograms
        for (name, stats) in summary.histograms {
            output.push_str(&format!("nexus_histogram_{}_count {}\n", name, stats.count));
            output.push_str(&format!("nexus_histogram_{}_sum {}\n", name, stats.sum));
            output.push_str(&format!("nexus_histogram_{}_min {}\n", name, stats.min));
            output.push_str(&format!("nexus_histogram_{}_max {}\n", name, stats.max));
            output.push_str(&format!("nexus_histogram_{}_avg {}\n", name, stats.avg));
        }

        // Export timers
        for (name, stats) in summary.timers {
            output.push_str(&format!("nexus_timer_{}_count {}\n", name, stats.count));
            output.push_str(&format!(
                "nexus_timer_{}_total_ms {}\n",
                name, stats.total_ms
            ));
            output.push_str(&format!("nexus_timer_{}_avg_ms {}\n", name, stats.avg_ms));
            output.push_str(&format!("nexus_timer_{}_min_ms {}\n", name, stats.min_ms));
            output.push_str(&format!("nexus_timer_{}_max_ms {}\n", name, stats.max_ms));
        }

        output
    }

    /// Get performance insights
    pub async fn get_performance_insights(&self) -> Vec<PerformanceInsight> {
        let mut insights = Vec::new();
        let summary = self.get_metrics_summary().await;

        // Analyze counters
        for (name, value) in summary.counters {
            if name.contains("error") && value > 0 {
                insights.push(PerformanceInsight {
                    category: "Errors".to_string(),
                    severity: InsightSeverity::Warning,
                    message: format!("{} errors detected: {}", name, value),
                    recommendation: "Investigate error sources and implement error handling"
                        .to_string(),
                });
            }
        }

        // Analyze gauges
        for (name, value) in summary.gauges {
            if name.contains("memory") && value > 1000.0 {
                insights.push(PerformanceInsight {
                    category: "Memory".to_string(),
                    severity: InsightSeverity::Info,
                    message: format!("High memory usage: {} = {}", name, value),
                    recommendation: "Consider memory optimization or increasing available memory"
                        .to_string(),
                });
            }
        }

        // Analyze histograms
        for (name, stats) in summary.histograms {
            if name.contains("latency") && stats.avg > 1000.0 {
                insights.push(PerformanceInsight {
                    category: "Latency".to_string(),
                    severity: InsightSeverity::Warning,
                    message: format!("High average latency: {} = {:.2}ms", name, stats.avg),
                    recommendation: "Optimize operations to reduce latency".to_string(),
                });
            }
        }

        // Analyze timers
        for (name, stats) in summary.timers {
            if name.contains("query") && stats.avg_ms > 100.0 {
                insights.push(PerformanceInsight {
                    category: "Query Performance".to_string(),
                    severity: InsightSeverity::Warning,
                    message: format!(
                        "Slow queries detected: {} avg = {:.2}ms",
                        name, stats.avg_ms
                    ),
                    recommendation: "Optimize query execution or add indexes".to_string(),
                });
            }
        }

        insights
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Metric value types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricValue {
    Counter(u64),
    Gauge(f64),
    Histogram(HistogramStats),
    Timer(TimerStats),
}

/// Counter metric
#[derive(Debug, Clone)]
pub struct Counter {
    value: u64,
}

impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn increment(&mut self, amount: u64) {
        self.value += amount;
    }

    pub fn value(&self) -> u64 {
        self.value
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

/// Gauge metric
#[derive(Debug, Clone)]
pub struct Gauge {
    value: f64,
}

impl Gauge {
    pub fn new() -> Self {
        Self { value: 0.0 }
    }

    pub fn set(&mut self, value: f64) {
        self.value = value;
    }

    pub fn value(&self) -> f64 {
        self.value
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

/// Histogram metric
#[derive(Debug, Clone)]
pub struct Histogram {
    values: Vec<f64>,
    sum: f64,
    count: u64,
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            values: Vec::new(),
            sum: 0.0,
            count: 0,
        }
    }

    pub fn record(&mut self, value: f64) {
        self.values.push(value);
        self.sum += value;
        self.count += 1;
    }

    pub fn stats(&self) -> HistogramStats {
        if self.values.is_empty() {
            return HistogramStats {
                count: 0,
                sum: 0.0,
                min: 0.0,
                max: 0.0,
                avg: 0.0,
            };
        }

        let min = self.values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = self.values.iter().cloned().fold(0.0, f64::max);
        let avg = self.sum / self.count as f64;

        HistogramStats {
            count: self.count,
            sum: self.sum,
            min,
            max,
            avg,
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer metric
#[derive(Debug, Clone)]
pub struct Timer {
    durations: Vec<Duration>,
    total_duration: Duration,
    count: u64,
}

impl Timer {
    pub fn new() -> Self {
        Self {
            durations: Vec::new(),
            total_duration: Duration::new(0, 0),
            count: 0,
        }
    }

    pub fn start(&self) -> TimerHandle {
        TimerHandle::new()
    }

    pub fn record(&mut self, duration: Duration) {
        self.durations.push(duration);
        self.total_duration += duration;
        self.count += 1;
    }

    pub fn stats(&self) -> TimerStats {
        if self.durations.is_empty() {
            return TimerStats {
                count: 0,
                total_ms: 0.0,
                avg_ms: 0.0,
                min_ms: 0.0,
                max_ms: 0.0,
            };
        }

        let min_ms = self
            .durations
            .iter()
            .map(|d| d.as_millis() as f64)
            .fold(f64::INFINITY, f64::min);
        let max_ms = self
            .durations
            .iter()
            .map(|d| d.as_millis() as f64)
            .fold(0.0, f64::max);
        let avg_ms = self.total_duration.as_millis() as f64 / self.count as f64;

        TimerStats {
            count: self.count,
            total_ms: self.total_duration.as_millis() as f64,
            avg_ms,
            min_ms,
            max_ms,
        }
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

/// Timer handle for measuring durations
pub struct TimerHandle {
    start_time: Instant,
}

impl TimerHandle {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

impl Default for TimerHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// Histogram statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramStats {
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub avg: f64,
}

/// Timer statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerStats {
    pub count: u64,
    pub total_ms: f64,
    pub avg_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

/// Metrics summary
#[derive(Debug, Clone)]
pub struct MetricsSummary {
    pub counters: std::collections::HashMap<String, u64>,
    pub gauges: std::collections::HashMap<String, f64>,
    pub histograms: std::collections::HashMap<String, HistogramStats>,
    pub timers: std::collections::HashMap<String, TimerStats>,
    pub timestamp: Instant,
}

/// Performance insight
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceInsight {
    pub category: String,
    pub severity: InsightSeverity,
    pub message: String,
    pub recommendation: String,
}

/// Insight severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum InsightSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_metrics_creation() {
        let metrics = PerformanceMetrics::new();
        assert!(metrics.get_metrics_summary().await.counters.is_empty());
    }

    #[tokio::test]
    async fn test_counter_operations() {
        let metrics = PerformanceMetrics::new();

        metrics.increment_counter("test_counter", 5).await;
        metrics.increment_counter("test_counter", 3).await;

        assert_eq!(metrics.get_counter("test_counter").await, Some(8));
    }

    #[tokio::test]
    async fn test_gauge_operations() {
        let metrics = PerformanceMetrics::new();

        metrics.set_gauge("test_gauge", 42.5).await;

        assert_eq!(metrics.get_gauge("test_gauge").await, Some(42.5));
    }

    #[tokio::test]
    async fn test_histogram_operations() {
        let metrics = PerformanceMetrics::new();

        metrics.record_histogram("test_histogram", 10.0).await;
        metrics.record_histogram("test_histogram", 20.0).await;
        metrics.record_histogram("test_histogram", 30.0).await;

        let stats = metrics.get_histogram_stats("test_histogram").await.unwrap();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.sum, 60.0);
        assert_eq!(stats.min, 10.0);
        assert_eq!(stats.max, 30.0);
        assert_eq!(stats.avg, 20.0);
    }

    #[tokio::test]
    async fn test_timer_operations() {
        let metrics = PerformanceMetrics::new();

        metrics
            .record_timer("test_timer", Duration::from_millis(100))
            .await;
        metrics
            .record_timer("test_timer", Duration::from_millis(200))
            .await;

        let stats = metrics.get_timer_stats("test_timer").await.unwrap();
        assert_eq!(stats.count, 2);
        assert_eq!(stats.total_ms, 300.0);
        assert_eq!(stats.avg_ms, 150.0);
        assert_eq!(stats.min_ms, 100.0);
        assert_eq!(stats.max_ms, 200.0);
    }

    #[tokio::test]
    async fn test_timer_handle() {
        let handle = TimerHandle::new();
        tokio::time::sleep(Duration::from_millis(10)).await;
        let elapsed = handle.elapsed();
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[tokio::test]
    async fn test_metrics_summary() {
        let metrics = PerformanceMetrics::new();

        metrics.increment_counter("counter1", 10).await;
        metrics.set_gauge("gauge1", 25.5).await;

        let summary = metrics.get_metrics_summary().await;
        assert_eq!(summary.counters.get("counter1"), Some(&10));
        assert_eq!(summary.gauges.get("gauge1"), Some(&25.5));
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let metrics = PerformanceMetrics::new();

        metrics.increment_counter("test_counter", 5).await;
        metrics.set_gauge("test_gauge", 42.0).await;

        let prometheus_output = metrics.export_prometheus().await;
        assert!(prometheus_output.contains("nexus_counter_test_counter 5"));
        assert!(prometheus_output.contains("nexus_gauge_test_gauge 42"));
    }

    #[tokio::test]
    async fn test_performance_insights() {
        let metrics = PerformanceMetrics::new();

        // Add some metrics that should generate insights
        metrics.increment_counter("error_count", 5).await;
        metrics.set_gauge("memory_usage", 1500.0).await;
        metrics.record_histogram("latency", 1500.0).await;
        metrics
            .record_timer("query_time", Duration::from_millis(150))
            .await;

        let insights = metrics.get_performance_insights().await;
        assert!(!insights.is_empty());

        // Check for specific insights
        let has_error_insight = insights.iter().any(|i| i.category == "Errors");
        let has_memory_insight = insights.iter().any(|i| i.category == "Memory");
        let has_latency_insight = insights.iter().any(|i| i.category == "Latency");
        let has_query_insight = insights.iter().any(|i| i.category == "Query Performance");

        assert!(has_error_insight);
        assert!(has_memory_insight);
        assert!(has_latency_insight);
        assert!(has_query_insight);
    }

    #[tokio::test]
    async fn test_clear_all() {
        let metrics = PerformanceMetrics::new();

        metrics.increment_counter("test_counter", 5).await;
        metrics.set_gauge("test_gauge", 42.0).await;

        metrics.clear_all().await;

        let summary = metrics.get_metrics_summary().await;
        assert!(summary.counters.is_empty());
        assert!(summary.gauges.is_empty());
    }
}
