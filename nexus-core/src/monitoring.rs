//! Monitoring and metrics collection
//!
//! This module provides:
//! - Performance metrics collection
//! - System health monitoring
//! - Query execution statistics
//! - Resource usage tracking
//! - Alerting and notifications

use crate::Result;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Metric type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetricType {
    /// Counter metric (monotonically increasing)
    Counter,
    /// Gauge metric (can go up or down)
    Gauge,
    /// Histogram metric (distribution of values)
    Histogram,
    /// Summary metric (quantiles and counts)
    Summary,
}

/// Metric value
#[derive(Debug, Clone)]
pub enum MetricValue {
    /// Counter value
    Counter(u64),
    /// Gauge value
    Gauge(f64),
    /// Histogram value with count and sum
    Histogram {
        count: u64,
        sum: f64,
        buckets: Vec<(f64, u64)>,
    },
    /// Summary value with quantiles
    Summary {
        count: u64,
        sum: f64,
        quantiles: Vec<(f64, f64)>,
    },
}

/// Metric sample
#[derive(Debug, Clone)]
pub struct MetricSample {
    /// Metric name
    pub name: String,
    /// Metric labels
    pub labels: HashMap<String, String>,
    /// Metric value
    pub value: MetricValue,
    /// Timestamp
    pub timestamp: u64,
}

/// Metric collector
pub struct MetricCollector {
    /// Collected metrics
    metrics: Arc<RwLock<HashMap<String, MetricSample>>>,
    /// Metric history (for time-series data)
    history: Arc<RwLock<VecDeque<MetricSample>>>,
    /// Maximum history size
    max_history_size: usize,
    /// Collection start time
    start_time: Instant,
}

impl MetricCollector {
    /// Create a new metric collector
    pub fn new(max_history_size: usize) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(HashMap::new())),
            history: Arc::new(RwLock::new(VecDeque::new())),
            max_history_size,
            start_time: Instant::now(),
        }
    }

    /// Record a counter metric
    pub fn record_counter(&self, name: &str, value: u64, labels: HashMap<String, String>) {
        let sample = MetricSample {
            name: name.to_string(),
            labels,
            value: MetricValue::Counter(value),
            timestamp: self.current_timestamp(),
        };

        self.record_sample(sample);
    }

    /// Record a gauge metric
    pub fn record_gauge(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let sample = MetricSample {
            name: name.to_string(),
            labels,
            value: MetricValue::Gauge(value),
            timestamp: self.current_timestamp(),
        };

        self.record_sample(sample);
    }

    /// Record a histogram metric
    pub fn record_histogram(
        &self,
        name: &str,
        value: f64,
        buckets: Vec<f64>,
        labels: HashMap<String, String>,
    ) {
        let mut bucket_counts = vec![0; buckets.len()];
        for (i, &bucket) in buckets.iter().enumerate() {
            if value <= bucket {
                bucket_counts[i] += 1;
            }
        }

        let sample = MetricSample {
            name: name.to_string(),
            labels,
            value: MetricValue::Histogram {
                count: 1,
                sum: value,
                buckets: buckets.into_iter().zip(bucket_counts).collect(),
            },
            timestamp: self.current_timestamp(),
        };

        self.record_sample(sample);
    }

    /// Record a summary metric
    pub fn record_summary(
        &self,
        name: &str,
        value: f64,
        quantiles: Vec<f64>,
        labels: HashMap<String, String>,
    ) {
        let sample = MetricSample {
            name: name.to_string(),
            labels,
            value: MetricValue::Summary {
                count: 1,
                sum: value,
                quantiles: quantiles.into_iter().map(|q| (q, value)).collect(),
            },
            timestamp: self.current_timestamp(),
        };

        self.record_sample(sample);
    }

    /// Record a metric sample
    fn record_sample(&self, sample: MetricSample) {
        let key = format!("{}{:?}", sample.name, sample.labels);

        // Update current metrics
        {
            let mut metrics = self.metrics.write().unwrap();
            metrics.insert(key.clone(), sample.clone());
        }

        // Add to history
        {
            let mut history = self.history.write().unwrap();
            history.push_back(sample);

            // Trim history if it exceeds max size
            while history.len() > self.max_history_size {
                history.pop_front();
            }
        }
    }

    /// Get current timestamp in milliseconds
    fn current_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Get all current metrics
    pub fn get_metrics(&self) -> HashMap<String, MetricSample> {
        self.metrics.read().unwrap().clone()
    }

    /// Get metric history
    pub fn get_history(&self) -> VecDeque<MetricSample> {
        self.history.read().unwrap().clone()
    }

    /// Get metrics by name
    pub fn get_metrics_by_name(&self, name: &str) -> Vec<MetricSample> {
        self.metrics
            .read()
            .unwrap()
            .values()
            .filter(|sample| sample.name == name)
            .cloned()
            .collect()
    }

    /// Clear all metrics
    pub fn clear_metrics(&self) {
        self.metrics.write().unwrap().clear();
        self.history.write().unwrap().clear();
    }
}

/// System health monitor
pub struct HealthMonitor {
    /// Health status
    status: Arc<RwLock<HealthStatus>>,
    /// Health checks
    checks: Arc<RwLock<Vec<Box<dyn HealthCheck + Send + Sync>>>>,
    /// Last check time
    last_check: Arc<RwLock<Option<Instant>>>,
    /// Check interval
    check_interval: Duration,
}

/// Health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// System is healthy
    Healthy,
    /// System is degraded
    Degraded,
    /// System is unhealthy
    Unhealthy,
    /// System status is unknown
    Unknown,
}

/// Health check trait
pub trait HealthCheck {
    /// Check the health of a component
    fn check(&self) -> Result<HealthStatus>;
    /// Get the name of the health check
    fn name(&self) -> &str;
}

/// Database health check
pub struct DatabaseHealthCheck {
    /// Database connection status
    connected: Arc<AtomicU64>,
}

impl DatabaseHealthCheck {
    pub fn new(connected: Arc<AtomicU64>) -> Self {
        Self { connected }
    }
}

impl HealthCheck for DatabaseHealthCheck {
    fn check(&self) -> Result<HealthStatus> {
        if self.connected.load(Ordering::SeqCst) == 1 {
            Ok(HealthStatus::Healthy)
        } else {
            Ok(HealthStatus::Unhealthy)
        }
    }

    fn name(&self) -> &str {
        "database"
    }
}

/// Memory health check
pub struct MemoryHealthCheck {
    /// Memory usage threshold (0.0 to 1.0)
    threshold: f64,
    /// Current memory usage
    current_usage: Arc<AtomicUsize>,
    /// Total memory
    total_memory: Arc<AtomicUsize>,
}

impl MemoryHealthCheck {
    pub fn new(
        threshold: f64,
        current_usage: Arc<AtomicUsize>,
        total_memory: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            threshold,
            current_usage,
            total_memory,
        }
    }
}

impl HealthCheck for MemoryHealthCheck {
    fn check(&self) -> Result<HealthStatus> {
        let current = self.current_usage.load(Ordering::SeqCst) as f64;
        let total = self.total_memory.load(Ordering::SeqCst) as f64;

        if total == 0.0 {
            return Ok(HealthStatus::Unknown);
        }

        let usage_ratio = current / total;

        if usage_ratio >= self.threshold {
            Ok(HealthStatus::Unhealthy)
        } else if usage_ratio >= self.threshold * 0.8 {
            Ok(HealthStatus::Degraded)
        } else {
            Ok(HealthStatus::Healthy)
        }
    }

    fn name(&self) -> &str {
        "memory"
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(check_interval: Duration) -> Self {
        Self {
            status: Arc::new(RwLock::new(HealthStatus::Unknown)),
            checks: Arc::new(RwLock::new(Vec::new())),
            last_check: Arc::new(RwLock::new(None)),
            check_interval,
        }
    }

    /// Add a health check
    pub fn add_check(&self, check: Box<dyn HealthCheck + Send + Sync>) {
        self.checks.write().unwrap().push(check);
    }

    /// Run all health checks
    pub fn check_health(&self) -> Result<HealthStatus> {
        let checks = self.checks.read().unwrap();
        let mut overall_status = HealthStatus::Healthy;

        for check in checks.iter() {
            match check.check() {
                Ok(HealthStatus::Unhealthy) => {
                    overall_status = HealthStatus::Unhealthy;
                    break; // Unhealthy status takes precedence
                }
                Ok(HealthStatus::Degraded) => {
                    if overall_status == HealthStatus::Healthy {
                        overall_status = HealthStatus::Degraded;
                    }
                }
                Ok(HealthStatus::Healthy) => {
                    // Keep current status
                }
                Ok(HealthStatus::Unknown) => {
                    if overall_status == HealthStatus::Healthy {
                        overall_status = HealthStatus::Unknown;
                    }
                }
                Err(_) => {
                    overall_status = HealthStatus::Unknown;
                }
            }
        }

        // Update status
        {
            let mut status = self.status.write().unwrap();
            *status = overall_status.clone();
        }

        // Update last check time
        {
            let mut last_check = self.last_check.write().unwrap();
            *last_check = Some(Instant::now());
        }

        Ok(overall_status)
    }

    /// Get current health status
    pub fn get_status(&self) -> HealthStatus {
        self.status.read().unwrap().clone()
    }

    /// Get last check time
    pub fn get_last_check(&self) -> Option<Instant> {
        *self.last_check.read().unwrap()
    }

    /// Check if health check is due
    pub fn is_check_due(&self) -> bool {
        if let Some(last_check) = self.get_last_check() {
            last_check.elapsed() >= self.check_interval
        } else {
            true
        }
    }
}

/// Performance profiler
pub struct PerformanceProfiler {
    /// Profiling data
    profiles: Arc<RwLock<HashMap<String, ProfileData>>>,
    /// Active profiles
    active_profiles: Arc<RwLock<HashMap<String, Instant>>>,
}

/// Profile data
#[derive(Debug, Clone)]
pub struct ProfileData {
    /// Total execution time
    pub total_time: Duration,
    /// Number of executions
    pub execution_count: u64,
    /// Average execution time
    pub average_time: Duration,
    /// Minimum execution time
    pub min_time: Duration,
    /// Maximum execution time
    pub max_time: Duration,
    /// Last execution time
    pub last_execution: Option<Instant>,
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceProfiler {
    /// Create a new performance profiler
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
            active_profiles: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start profiling a function
    pub fn start_profile(&self, name: &str) {
        let mut active_profiles = self.active_profiles.write().unwrap();
        active_profiles.insert(name.to_string(), Instant::now());
    }

    /// End profiling a function
    pub fn end_profile(&self, name: &str) {
        let mut active_profiles = self.active_profiles.write().unwrap();
        if let Some(start_time) = active_profiles.remove(name) {
            let duration = start_time.elapsed();
            self.record_profile(name, duration);
        }
    }

    /// Record profile data
    fn record_profile(&self, name: &str, duration: Duration) {
        let mut profiles = self.profiles.write().unwrap();

        let profile = profiles.entry(name.to_string()).or_insert(ProfileData {
            total_time: Duration::from_secs(0),
            execution_count: 0,
            average_time: Duration::from_secs(0),
            min_time: Duration::from_secs(0),
            max_time: Duration::from_secs(0),
            last_execution: None,
        });

        profile.total_time += duration;
        profile.execution_count += 1;
        profile.average_time = profile.total_time / profile.execution_count as u32;

        if profile.execution_count == 1 || duration < profile.min_time {
            profile.min_time = duration;
        }

        if profile.execution_count == 1 || duration > profile.max_time {
            profile.max_time = duration;
        }

        profile.last_execution = Some(Instant::now());
    }

    /// Get profile data
    pub fn get_profile(&self, name: &str) -> Option<ProfileData> {
        self.profiles.read().unwrap().get(name).cloned()
    }

    /// Get all profiles
    pub fn get_all_profiles(&self) -> HashMap<String, ProfileData> {
        self.profiles.read().unwrap().clone()
    }

    /// Clear all profiles
    pub fn clear_profiles(&self) {
        self.profiles.write().unwrap().clear();
        self.active_profiles.write().unwrap().clear();
    }
}

/// Query execution statistics
pub struct QueryStats {
    /// Total queries executed
    pub total_queries: AtomicU64,
    /// Successful queries
    pub successful_queries: AtomicU64,
    /// Failed queries
    pub failed_queries: AtomicU64,
    /// Total execution time
    pub total_execution_time: AtomicU64,
    /// Average execution time
    pub average_execution_time: AtomicU64,
    /// Last query time
    pub last_query_time: AtomicU64,
}

impl QueryStats {
    /// Create new query statistics
    pub fn new() -> Self {
        Self {
            total_queries: AtomicU64::new(0),
            successful_queries: AtomicU64::new(0),
            failed_queries: AtomicU64::new(0),
            total_execution_time: AtomicU64::new(0),
            average_execution_time: AtomicU64::new(0),
            last_query_time: AtomicU64::new(0),
        }
    }

    /// Record a query execution
    pub fn record_query(&self, success: bool, execution_time: Duration) {
        self.total_queries.fetch_add(1, Ordering::SeqCst);

        if success {
            self.successful_queries.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failed_queries.fetch_add(1, Ordering::SeqCst);
        }

        let time_ms = execution_time.as_millis() as u64;
        self.total_execution_time
            .fetch_add(time_ms, Ordering::SeqCst);

        let total = self.total_queries.load(Ordering::SeqCst);
        let total_time = self.total_execution_time.load(Ordering::SeqCst);
        self.average_execution_time
            .store(total_time / total.max(1), Ordering::SeqCst);

        self.last_query_time.store(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            Ordering::SeqCst,
        );
    }

    /// Get success rate
    pub fn get_success_rate(&self) -> f64 {
        let total = self.total_queries.load(Ordering::SeqCst);
        let successful = self.successful_queries.load(Ordering::SeqCst);

        if total == 0 {
            0.0
        } else {
            successful as f64 / total as f64
        }
    }

    /// Get failure rate
    pub fn get_failure_rate(&self) -> f64 {
        1.0 - self.get_success_rate()
    }
}

impl Default for QueryStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitoring dashboard
pub struct MonitoringDashboard {
    /// Metric collector
    metric_collector: Arc<MetricCollector>,
    /// Health monitor
    health_monitor: Arc<HealthMonitor>,
    /// Performance profiler
    profiler: Arc<PerformanceProfiler>,
    /// Query statistics
    query_stats: Arc<QueryStats>,
}

impl Default for MonitoringDashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl MonitoringDashboard {
    /// Create a new monitoring dashboard
    pub fn new() -> Self {
        Self {
            metric_collector: Arc::new(MetricCollector::new(10000)),
            health_monitor: Arc::new(HealthMonitor::new(Duration::from_secs(30))),
            profiler: Arc::new(PerformanceProfiler::new()),
            query_stats: Arc::new(QueryStats::new()),
        }
    }

    /// Get metric collector
    pub fn get_metric_collector(&self) -> Arc<MetricCollector> {
        self.metric_collector.clone()
    }

    /// Get health monitor
    pub fn get_health_monitor(&self) -> Arc<HealthMonitor> {
        self.health_monitor.clone()
    }

    /// Get performance profiler
    pub fn get_profiler(&self) -> Arc<PerformanceProfiler> {
        self.profiler.clone()
    }

    /// Get query statistics
    pub fn get_query_stats(&self) -> Arc<QueryStats> {
        self.query_stats.clone()
    }

    /// Get system overview
    pub fn get_system_overview(&self) -> SystemOverview {
        SystemOverview {
            health_status: self.health_monitor.get_status(),
            total_queries: self.query_stats.total_queries.load(Ordering::SeqCst),
            success_rate: self.query_stats.get_success_rate(),
            average_execution_time: self
                .query_stats
                .average_execution_time
                .load(Ordering::SeqCst),
            uptime: self
                .health_monitor
                .get_last_check()
                .map(|last_check| last_check.elapsed())
                .unwrap_or(Duration::from_secs(0)),
        }
    }
}

/// System overview
#[derive(Debug, Clone)]
pub struct SystemOverview {
    /// Current health status
    pub health_status: HealthStatus,
    /// Total queries executed
    pub total_queries: u64,
    /// Query success rate
    pub success_rate: f64,
    /// Average execution time in milliseconds
    pub average_execution_time: u64,
    /// System uptime
    pub uptime: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn test_metric_collector() {
        let collector = MetricCollector::new(1000);

        // Record a counter metric
        let mut labels = HashMap::new();
        labels.insert("component".to_string(), "database".to_string());
        collector.record_counter("queries_total", 100, labels);

        let metrics = collector.get_metrics();
        assert_eq!(metrics.len(), 1);
    }

    #[test]
    fn test_health_monitor() {
        let monitor = HealthMonitor::new(Duration::from_secs(1));

        // Add a health check
        let connected = Arc::new(AtomicU64::new(1));
        let db_check = DatabaseHealthCheck::new(connected);
        monitor.add_check(Box::new(db_check));

        // Check health
        let status = monitor.check_health().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn test_performance_profiler() {
        let profiler = PerformanceProfiler::new();

        // Profile a function
        profiler.start_profile("test_function");
        std::thread::sleep(Duration::from_millis(10));
        profiler.end_profile("test_function");

        let profile = profiler.get_profile("test_function").unwrap();
        assert_eq!(profile.execution_count, 1);
        assert!(profile.total_time >= Duration::from_millis(10));
    }

    #[test]
    fn test_query_stats() {
        let stats = QueryStats::new();

        // Record some queries
        stats.record_query(true, Duration::from_millis(100));
        stats.record_query(false, Duration::from_millis(200));
        stats.record_query(true, Duration::from_millis(150));

        assert_eq!(stats.total_queries.load(Ordering::SeqCst), 3);
        assert_eq!(stats.successful_queries.load(Ordering::SeqCst), 2);
        assert_eq!(stats.failed_queries.load(Ordering::SeqCst), 1);
        assert_eq!(stats.get_success_rate(), 2.0 / 3.0);
    }

    #[test]
    fn test_monitoring_dashboard() {
        let dashboard = MonitoringDashboard::new();
        let overview = dashboard.get_system_overview();

        assert_eq!(overview.total_queries, 0);
        assert_eq!(overview.success_rate, 0.0);
    }
}
