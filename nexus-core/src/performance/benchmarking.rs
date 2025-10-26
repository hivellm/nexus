//! Advanced performance benchmarking utilities
//!
//! Provides comprehensive benchmarking tools including micro-benchmarks,
//! macro-benchmarks, comparative analysis, and performance regression detection.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Advanced performance benchmark suite
pub struct PerformanceBenchmark {
    benchmark_results: RwLock<Vec<BenchmarkResult>>,
    baseline_results: RwLock<Option<BenchmarkBaseline>>,
    config: BenchmarkConfig,
}

impl PerformanceBenchmark {
    /// Create a new performance benchmark suite
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            benchmark_results: RwLock::new(Vec::new()),
            baseline_results: RwLock::new(None),
            config,
        }
    }

    /// Run comprehensive benchmark suite
    pub async fn run_full_benchmark(&self) -> Result<BenchmarkSuite, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let mut results = Vec::new();

        // Micro-benchmarks
        results.push(self.run_micro_benchmark("memory_allocation", |_| async {
            self.benchmark_memory_allocation().await
        }).await?);
        
        results.push(self.run_micro_benchmark("cache_operations", |_| async {
            self.benchmark_cache_operations().await
        }).await?);
        
        results.push(self.run_micro_benchmark("string_operations", |_| async {
            self.benchmark_string_operations().await
        }).await?);

        // Macro-benchmarks
        results.push(self.run_macro_benchmark("query_execution", |_| async {
            self.benchmark_query_execution().await
        }).await?);
        
        results.push(self.run_macro_benchmark("data_ingestion", |_| async {
            self.benchmark_data_ingestion().await
        }).await?);
        
        results.push(self.run_macro_benchmark("concurrent_operations", |_| async {
            self.benchmark_concurrent_operations().await
        }).await?);

        // System benchmarks
        results.push(self.run_system_benchmark("cpu_intensive", |_| async {
            self.benchmark_cpu_intensive_operations().await
        }).await?);
        
        results.push(self.run_system_benchmark("io_intensive", |_| async {
            self.benchmark_io_intensive_operations().await
        }).await?);

        // Store results
        {
            let mut stored_results = self.benchmark_results.write().await;
            stored_results.extend(results.clone());
        }

        Ok(BenchmarkSuite {
            results,
            total_duration: start_time.elapsed(),
            timestamp: Instant::now(),
        })
    }

    /// Run a micro-benchmark
    async fn run_micro_benchmark<F, Fut>(
        &self,
        name: &str,
        benchmark_fn: F,
    ) -> Result<BenchmarkResult, Box<dyn std::error::Error>>
    where
        F: Fn(usize) -> Fut,
        Fut: std::future::Future<Output = Result<Duration, Box<dyn std::error::Error>>>,
    {
        let iterations = self.config.micro_benchmark_iterations;
        let mut durations = Vec::new();
        let mut total_duration = Duration::new(0, 0);

        // Warmup
        for _ in 0..self.config.warmup_iterations {
            let _ = benchmark_fn(0);
        }

        // Actual benchmark
        for i in 0..iterations {
            let duration = benchmark_fn(i)?;
            durations.push(duration);
            total_duration += duration;
        }

        let avg_duration = total_duration / iterations as u32;
        let min_duration = durations.iter().min().copied().unwrap_or(Duration::new(0, 0));
        let max_duration = durations.iter().max().copied().unwrap_or(Duration::new(0, 0));

        // Calculate standard deviation
        let variance = durations
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - avg_duration.as_nanos() as f64;
                diff * diff
            })
            .sum::<f64>()
            / iterations as f64;
        let std_deviation = Duration::from_nanos(variance.sqrt() as u64);

        Ok(BenchmarkResult {
            name: name.to_string(),
            benchmark_type: BenchmarkType::Micro,
            iterations,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            std_deviation,
            throughput: if avg_duration.as_nanos() > 0 {
                iterations as f64 / avg_duration.as_secs_f64()
            } else {
                0.0
            },
            memory_usage: self.estimate_memory_usage(name).await,
            cpu_usage: self.estimate_cpu_usage(name).await,
        })
    }

    /// Run a macro-benchmark
    async fn run_macro_benchmark<F>(
        &self,
        name: &str,
        benchmark_fn: F,
    ) -> Result<BenchmarkResult, Box<dyn std::error::Error>>
    where
        F: Fn(usize) -> Result<Duration, Box<dyn std::error::Error>>,
    {
        let iterations = self.config.macro_benchmark_iterations;
        let mut durations = Vec::new();
        let mut total_duration = Duration::new(0, 0);

        // Warmup
        for _ in 0..self.config.warmup_iterations {
            let _ = benchmark_fn(0);
        }

        // Actual benchmark
        for i in 0..iterations {
            let duration = benchmark_fn(i)?;
            durations.push(duration);
            total_duration += duration;
        }

        let avg_duration = total_duration / iterations as u32;
        let min_duration = durations.iter().min().copied().unwrap_or(Duration::new(0, 0));
        let max_duration = durations.iter().max().copied().unwrap_or(Duration::new(0, 0));

        // Calculate standard deviation
        let variance = durations
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - avg_duration.as_nanos() as f64;
                diff * diff
            })
            .sum::<f64>()
            / iterations as f64;
        let std_deviation = Duration::from_nanos(variance.sqrt() as u64);

        Ok(BenchmarkResult {
            name: name.to_string(),
            benchmark_type: BenchmarkType::Macro,
            iterations,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            std_deviation,
            throughput: if avg_duration.as_nanos() > 0 {
                iterations as f64 / avg_duration.as_secs_f64()
            } else {
                0.0
            },
            memory_usage: self.estimate_memory_usage(name).await,
            cpu_usage: self.estimate_cpu_usage(name).await,
        })
    }

    /// Run a system benchmark
    async fn run_system_benchmark<F>(
        &self,
        name: &str,
        benchmark_fn: F,
    ) -> Result<BenchmarkResult, Box<dyn std::error::Error>>
    where
        F: Fn(usize) -> Result<Duration, Box<dyn std::error::Error>>,
    {
        let iterations = self.config.system_benchmark_iterations;
        let mut durations = Vec::new();
        let mut total_duration = Duration::new(0, 0);

        // Warmup
        for _ in 0..self.config.warmup_iterations {
            let _ = benchmark_fn(0);
        }

        // Actual benchmark
        for i in 0..iterations {
            let duration = benchmark_fn(i)?;
            durations.push(duration);
            total_duration += duration;
        }

        let avg_duration = total_duration / iterations as u32;
        let min_duration = durations.iter().min().copied().unwrap_or(Duration::new(0, 0));
        let max_duration = durations.iter().max().copied().unwrap_or(Duration::new(0, 0));

        // Calculate standard deviation
        let variance = durations
            .iter()
            .map(|d| {
                let diff = d.as_nanos() as f64 - avg_duration.as_nanos() as f64;
                diff * diff
            })
            .sum::<f64>()
            / iterations as f64;
        let std_deviation = Duration::from_nanos(variance.sqrt() as u64);

        Ok(BenchmarkResult {
            name: name.to_string(),
            benchmark_type: BenchmarkType::System,
            iterations,
            total_duration,
            avg_duration,
            min_duration,
            max_duration,
            std_deviation,
            throughput: if avg_duration.as_nanos() > 0 {
                iterations as f64 / avg_duration.as_secs_f64()
            } else {
                0.0
            },
            memory_usage: self.estimate_memory_usage(name).await,
            cpu_usage: self.estimate_cpu_usage(name).await,
        })
    }

    /// Set baseline results for comparison
    pub async fn set_baseline(&self, baseline: BenchmarkBaseline) {
        let mut baseline_results = self.baseline_results.write().await;
        *baseline_results = Some(baseline);
    }

    /// Compare current results with baseline
    pub async fn compare_with_baseline(&self) -> Option<BenchmarkComparison> {
        let baseline = self.baseline_results.read().await.as_ref()?;
        let current_results = self.benchmark_results.read().await;

        if current_results.is_empty() {
            return None;
        }

        let mut comparisons = Vec::new();
        let mut overall_regression = false;

        for current in current_results.iter() {
            if let Some(baseline_result) = baseline.results.get(&current.name) {
                let performance_change = if baseline_result.avg_duration.as_nanos() > 0 {
                    ((current.avg_duration.as_nanos() as f64 - baseline_result.avg_duration.as_nanos() as f64)
                        / baseline_result.avg_duration.as_nanos() as f64) * 100.0
                } else {
                    0.0
                };

                let is_regression = performance_change > self.config.regression_threshold_percent;
                if is_regression {
                    overall_regression = true;
                }

                comparisons.push(BenchmarkComparisonItem {
                    name: current.name.clone(),
                    baseline_avg: baseline_result.avg_duration,
                    current_avg: current.avg_duration,
                    performance_change,
                    is_regression,
                    severity: if performance_change > 50.0 {
                        RegressionSeverity::Critical
                    } else if performance_change > 20.0 {
                        RegressionSeverity::High
                    } else if performance_change > 10.0 {
                        RegressionSeverity::Medium
                    } else {
                        RegressionSeverity::Low
                    },
                });
            }
        }

        Some(BenchmarkComparison {
            comparisons,
            overall_regression,
            regression_count: comparisons.iter().filter(|c| c.is_regression).count(),
            total_benchmarks: comparisons.len(),
        })
    }

    /// Get benchmark statistics
    pub async fn get_benchmark_statistics(&self) -> BenchmarkStatistics {
        let results = self.benchmark_results.read().await;

        if results.is_empty() {
            return BenchmarkStatistics::default();
        }

        let total_benchmarks = results.len();
        let total_duration: Duration = results.iter().map(|r| r.total_duration).sum();
        let avg_duration = total_duration / total_benchmarks as u32;

        let mut fastest_benchmark = None;
        let mut slowest_benchmark = None;
        let mut highest_throughput = 0.0;
        let mut lowest_throughput = f64::INFINITY;

        for result in results.iter() {
            if fastest_benchmark.is_none() || result.avg_duration < fastest_benchmark.unwrap().avg_duration {
                fastest_benchmark = Some(result.name.clone());
            }
            if slowest_benchmark.is_none() || result.avg_duration > slowest_benchmark.unwrap().avg_duration {
                slowest_benchmark = Some(result.name.clone());
            }
            if result.throughput > highest_throughput {
                highest_throughput = result.throughput;
            }
            if result.throughput < lowest_throughput {
                lowest_throughput = result.throughput;
            }
        }

        BenchmarkStatistics {
            total_benchmarks,
            total_duration,
            avg_duration,
            fastest_benchmark,
            slowest_benchmark,
            highest_throughput,
            lowest_throughput,
        }
    }

    /// Clear all benchmark results
    pub async fn clear_results(&self) {
        let mut results = self.benchmark_results.write().await;
        results.clear();
    }

    // Benchmark implementations
    async fn benchmark_memory_allocation(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate memory allocation
        let mut data = Vec::with_capacity(10000);
        for i in 0..10000 {
            data.push(i);
        }
        
        // Simulate some operations
        let _sum: usize = data.iter().sum();
        
        Ok(start.elapsed())
    }

    async fn benchmark_cache_operations(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate cache operations
        let mut cache = std::collections::HashMap::new();
        for i in 0..1000 {
            cache.insert(i, i * 2);
        }
        
        // Simulate cache lookups
        for i in 0..1000 {
            let _ = cache.get(&i);
        }
        
        Ok(start.elapsed())
    }

    async fn benchmark_string_operations(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate string operations
        let mut strings = Vec::new();
        for i in 0..1000 {
            strings.push(format!("string_{}", i));
        }
        
        // Simulate string processing
        let _total_length: usize = strings.iter().map(|s| s.len()).sum();
        
        Ok(start.elapsed())
    }

    async fn benchmark_query_execution(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate query execution
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        Ok(start.elapsed())
    }

    async fn benchmark_data_ingestion(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate data ingestion
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        Ok(start.elapsed())
    }

    async fn benchmark_concurrent_operations(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate concurrent operations
        let handles: Vec<_> = (0..10)
            .map(|_| {
                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                })
            })
            .collect();
        
        for handle in handles {
            let _ = handle.await;
        }
        
        Ok(start.elapsed())
    }

    async fn benchmark_cpu_intensive_operations(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate CPU-intensive operations
        let mut sum = 0u64;
        for i in 0..1000000 {
            sum += i;
        }
        
        Ok(start.elapsed())
    }

    async fn benchmark_io_intensive_operations(&self) -> Result<Duration, Box<dyn std::error::Error>> {
        let start = Instant::now();
        
        // Simulate I/O-intensive operations
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        Ok(start.elapsed())
    }

    async fn estimate_memory_usage(&self, _name: &str) -> u64 {
        // In a real implementation, this would measure actual memory usage
        1024 * 1024 * 10 // 10MB placeholder
    }

    async fn estimate_cpu_usage(&self, _name: &str) -> f64 {
        // In a real implementation, this would measure actual CPU usage
        25.0 // 25% placeholder
    }
}

impl Default for PerformanceBenchmark {
    fn default() -> Self {
        Self::new(BenchmarkConfig::default())
    }
}

/// Benchmark configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub micro_benchmark_iterations: usize,
    pub macro_benchmark_iterations: usize,
    pub system_benchmark_iterations: usize,
    pub warmup_iterations: usize,
    pub regression_threshold_percent: f64,
    pub timeout_duration: Duration,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            micro_benchmark_iterations: 10000,
            macro_benchmark_iterations: 100,
            system_benchmark_iterations: 50,
            warmup_iterations: 100,
            regression_threshold_percent: 10.0,
            timeout_duration: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Benchmark types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BenchmarkType {
    Micro,
    Macro,
    System,
}

/// Benchmark result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub name: String,
    pub benchmark_type: BenchmarkType,
    pub iterations: usize,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub min_duration: Duration,
    pub max_duration: Duration,
    pub std_deviation: Duration,
    pub throughput: f64,
    pub memory_usage: u64,
    pub cpu_usage: f64,
}

/// Benchmark suite
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkSuite {
    pub results: Vec<BenchmarkResult>,
    pub total_duration: Duration,
    pub timestamp: Instant,
}

/// Benchmark baseline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkBaseline {
    pub results: HashMap<String, BenchmarkResult>,
    pub timestamp: Instant,
    pub version: String,
}

/// Benchmark comparison
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparison {
    pub comparisons: Vec<BenchmarkComparisonItem>,
    pub overall_regression: bool,
    pub regression_count: usize,
    pub total_benchmarks: usize,
}

/// Benchmark comparison item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkComparisonItem {
    pub name: String,
    pub baseline_avg: Duration,
    pub current_avg: Duration,
    pub performance_change: f64,
    pub is_regression: bool,
    pub severity: RegressionSeverity,
}

/// Regression severity levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum RegressionSeverity {
    Low,
    Medium,
    High,
    Critical,
}

/// Benchmark statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BenchmarkStatistics {
    pub total_benchmarks: usize,
    pub total_duration: Duration,
    pub avg_duration: Duration,
    pub fastest_benchmark: Option<String>,
    pub slowest_benchmark: Option<String>,
    pub highest_throughput: f64,
    pub lowest_throughput: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_benchmark_creation() {
        let config = BenchmarkConfig::default();
        let benchmark = PerformanceBenchmark::new(config);
        assert!(benchmark.benchmark_results.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_micro_benchmark() {
        let config = BenchmarkConfig {
            micro_benchmark_iterations: 100,
            warmup_iterations: 10,
            ..Default::default()
        };
        let benchmark = PerformanceBenchmark::new(config);
        
        let result = benchmark.run_micro_benchmark("test", |_| {
            Ok(Duration::from_millis(1))
        }).await.unwrap();
        
        assert_eq!(result.name, "test");
        assert_eq!(result.benchmark_type, BenchmarkType::Micro);
        assert_eq!(result.iterations, 100);
    }

    #[tokio::test]
    async fn test_macro_benchmark() {
        let config = BenchmarkConfig {
            macro_benchmark_iterations: 10,
            warmup_iterations: 2,
            ..Default::default()
        };
        let benchmark = PerformanceBenchmark::new(config);
        
        let result = benchmark.run_macro_benchmark("test", |_| {
            Ok(Duration::from_millis(10))
        }).await.unwrap();
        
        assert_eq!(result.name, "test");
        assert_eq!(result.benchmark_type, BenchmarkType::Macro);
        assert_eq!(result.iterations, 10);
    }

    #[tokio::test]
    async fn test_system_benchmark() {
        let config = BenchmarkConfig {
            system_benchmark_iterations: 5,
            warmup_iterations: 1,
            ..Default::default()
        };
        let benchmark = PerformanceBenchmark::new(config);
        
        let result = benchmark.run_system_benchmark("test", |_| {
            Ok(Duration::from_millis(20))
        }).await.unwrap();
        
        assert_eq!(result.name, "test");
        assert_eq!(result.benchmark_type, BenchmarkType::System);
        assert_eq!(result.iterations, 5);
    }

    #[tokio::test]
    async fn test_benchmark_comparison() {
        let config = BenchmarkConfig::default();
        let benchmark = PerformanceBenchmark::new(config);
        
        // Create baseline
        let mut baseline_results = HashMap::new();
        baseline_results.insert("test".to_string(), BenchmarkResult {
            name: "test".to_string(),
            benchmark_type: BenchmarkType::Micro,
            iterations: 100,
            total_duration: Duration::from_millis(100),
            avg_duration: Duration::from_millis(1),
            min_duration: Duration::from_millis(1),
            max_duration: Duration::from_millis(1),
            std_deviation: Duration::from_millis(0),
            throughput: 100.0,
            memory_usage: 1024,
            cpu_usage: 25.0,
        });
        
        let baseline = BenchmarkBaseline {
            results: baseline_results,
            timestamp: Instant::now(),
            version: "1.0.0".to_string(),
        };
        
        benchmark.set_baseline(baseline).await;
        
        // Add current results
        {
            let mut results = benchmark.benchmark_results.write().await;
            results.push(BenchmarkResult {
                name: "test".to_string(),
                benchmark_type: BenchmarkType::Micro,
                iterations: 100,
                total_duration: Duration::from_millis(120),
                avg_duration: Duration::from_millis(1),
                min_duration: Duration::from_millis(1),
                max_duration: Duration::from_millis(1),
                std_deviation: Duration::from_millis(0),
                throughput: 100.0,
                memory_usage: 1024,
                cpu_usage: 25.0,
            });
        }
        
        let comparison = benchmark.compare_with_baseline().await;
        assert!(comparison.is_some());
    }

    #[tokio::test]
    async fn test_benchmark_statistics() {
        let config = BenchmarkConfig::default();
        let benchmark = PerformanceBenchmark::new(config);
        
        // Add some results
        {
            let mut results = benchmark.benchmark_results.write().await;
            results.push(BenchmarkResult {
                name: "test1".to_string(),
                benchmark_type: BenchmarkType::Micro,
                iterations: 100,
                total_duration: Duration::from_millis(100),
                avg_duration: Duration::from_millis(1),
                min_duration: Duration::from_millis(1),
                max_duration: Duration::from_millis(1),
                std_deviation: Duration::from_millis(0),
                throughput: 100.0,
                memory_usage: 1024,
                cpu_usage: 25.0,
            });
            results.push(BenchmarkResult {
                name: "test2".to_string(),
                benchmark_type: BenchmarkType::Micro,
                iterations: 100,
                total_duration: Duration::from_millis(200),
                avg_duration: Duration::from_millis(2),
                min_duration: Duration::from_millis(2),
                max_duration: Duration::from_millis(2),
                std_deviation: Duration::from_millis(0),
                throughput: 50.0,
                memory_usage: 2048,
                cpu_usage: 50.0,
            });
        }
        
        let stats = benchmark.get_benchmark_statistics().await;
        assert_eq!(stats.total_benchmarks, 2);
        assert_eq!(stats.fastest_benchmark, Some("test1".to_string()));
        assert_eq!(stats.slowest_benchmark, Some("test2".to_string()));
    }
}
