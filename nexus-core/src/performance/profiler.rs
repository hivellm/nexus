//! Query profiler for performance analysis
//!
//! Provides detailed profiling of query execution including timing, memory usage,
//! and performance bottlenecks identification.

use crate::performance::{Effort, Impact, OptimizationRecommendation, Priority, QueryProfile};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Query profiler for analyzing query performance
pub struct QueryProfiler {
    query_history: RwLock<Vec<QueryProfile>>,
    slow_query_threshold: Duration,
    memory_threshold: u64,
    cpu_threshold: f64,
}

impl QueryProfiler {
    /// Create a new query profiler
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            query_history: RwLock::new(Vec::new()),
            slow_query_threshold: Duration::from_millis(100),
            memory_threshold: 10 * 1024 * 1024, // 10MB
            cpu_threshold: 80.0,                // 80%
        })
    }

    /// Profile a query execution
    pub async fn profile_query(
        &self,
        query: &str,
    ) -> Result<QueryProfile, Box<dyn std::error::Error>> {
        let start_time = Instant::now();
        let start_memory = self.get_memory_usage()?;
        let start_cpu = self.get_cpu_usage()?;

        // Simulate query execution - in real implementation, this would execute the actual query
        let result = self.execute_query_with_profiling(query).await?;

        let execution_time = start_time.elapsed();
        let end_memory = self.get_memory_usage()?;
        let end_cpu = self.get_cpu_usage()?;

        let memory_usage = end_memory.saturating_sub(start_memory);
        let cpu_usage = (end_cpu - start_cpu).max(0.0);

        let profile = QueryProfile {
            query: query.to_string(),
            execution_time,
            memory_usage,
            cpu_usage,
            io_operations: result.io_operations,
            cache_hits: result.cache_hits,
            cache_misses: result.cache_misses,
            recommendations: self.generate_recommendations(
                query,
                execution_time,
                memory_usage,
                cpu_usage,
            ),
        };

        // Store in history
        {
            let mut history = self.query_history.write().await;
            history.push(profile.clone());

            // Keep only last 1000 queries
            let len = history.len();
            if len > 1000 {
                history.drain(0..len - 1000);
            }
        }

        Ok(profile)
    }

    /// Get query performance statistics
    pub async fn get_statistics(&self) -> QueryStatistics {
        let history = self.query_history.read().await;

        if history.is_empty() {
            return QueryStatistics::default();
        }

        let total_queries = history.len();
        let total_time: Duration = history.iter().map(|p| p.execution_time).sum();
        let avg_time = total_time / total_queries as u32;

        let slow_queries = history
            .iter()
            .filter(|p| p.execution_time > self.slow_query_threshold)
            .count();

        let memory_intensive_queries = history
            .iter()
            .filter(|p| p.memory_usage > self.memory_threshold)
            .count();

        let cpu_intensive_queries = history
            .iter()
            .filter(|p| p.cpu_usage > self.cpu_threshold)
            .count();

        let total_cache_hits: u64 = history.iter().map(|p| p.cache_hits).sum();
        let total_cache_misses: u64 = history.iter().map(|p| p.cache_misses).sum();
        let cache_hit_rate = if total_cache_hits + total_cache_misses > 0 {
            total_cache_hits as f64 / (total_cache_hits + total_cache_misses) as f64
        } else {
            0.0
        };

        QueryStatistics {
            total_queries,
            avg_execution_time: avg_time,
            slow_queries,
            memory_intensive_queries,
            cpu_intensive_queries,
            cache_hit_rate,
            total_execution_time: total_time,
        }
    }

    /// Get slow queries
    pub async fn get_slow_queries(&self, limit: usize) -> Vec<QueryProfile> {
        let history = self.query_history.read().await;
        let mut slow_queries: Vec<_> = history
            .iter()
            .filter(|p| p.execution_time > self.slow_query_threshold)
            .cloned()
            .collect();

        slow_queries.sort_by(|a, b| b.execution_time.cmp(&a.execution_time));
        slow_queries.truncate(limit);
        slow_queries
    }

    /// Get memory intensive queries
    pub async fn get_memory_intensive_queries(&self, limit: usize) -> Vec<QueryProfile> {
        let history = self.query_history.read().await;
        let mut memory_queries: Vec<_> = history
            .iter()
            .filter(|p| p.memory_usage > self.memory_threshold)
            .cloned()
            .collect();

        memory_queries.sort_by(|a, b| b.memory_usage.cmp(&a.memory_usage));
        memory_queries.truncate(limit);
        memory_queries
    }

    /// Get query patterns and recommendations
    pub async fn get_optimization_recommendations(&self) -> Vec<OptimizationRecommendation> {
        let mut recommendations = Vec::new();
        let statistics = self.get_statistics().await;

        // Slow query recommendations
        if statistics.slow_queries > 0 {
            recommendations.push(OptimizationRecommendation {
                category: "Query Performance".to_string(),
                priority: Priority::High,
                description: format!(
                    "{} slow queries detected (>{:?})",
                    statistics.slow_queries, self.slow_query_threshold
                ),
                impact: Impact::High,
                effort: Effort::Medium,
                implementation:
                    "Consider adding indexes, optimizing query patterns, or using query hints"
                        .to_string(),
            });
        }

        // Memory usage recommendations
        if statistics.memory_intensive_queries > 0 {
            recommendations.push(OptimizationRecommendation {
                category: "Memory Usage".to_string(),
                priority: Priority::Medium,
                description: format!("{} memory intensive queries detected (>{})", 
                    statistics.memory_intensive_queries, self.memory_threshold),
                impact: Impact::Medium,
                effort: Effort::High,
                implementation: "Consider query optimization, result set limiting, or memory tuning".to_string(),
            });
        }

        // Cache recommendations
        if statistics.cache_hit_rate < 0.8 {
            recommendations.push(OptimizationRecommendation {
                category: "Cache Performance".to_string(),
                priority: Priority::Medium,
                description: format!("Low cache hit rate: {:.1}%", statistics.cache_hit_rate * 100.0),
                impact: Impact::Medium,
                effort: Effort::Low,
                implementation: "Increase cache size, optimize cache eviction policy, or preload frequently accessed data".to_string(),
            });
        }

        recommendations
    }

    /// Set slow query threshold
    pub fn set_slow_query_threshold(&mut self, threshold: Duration) {
        self.slow_query_threshold = threshold;
    }

    /// Set memory threshold
    pub fn set_memory_threshold(&mut self, threshold: u64) {
        self.memory_threshold = threshold;
    }

    /// Set CPU threshold
    pub fn set_cpu_threshold(&mut self, threshold: f64) {
        self.cpu_threshold = threshold;
    }

    /// Clear query history
    pub async fn clear_history(&self) {
        let mut history = self.query_history.write().await;
        history.clear();
    }

    /// Simulate query execution with profiling
    async fn execute_query_with_profiling(
        &self,
        query: &str,
    ) -> Result<QueryExecutionResult, Box<dyn std::error::Error>> {
        // Simulate query execution time based on query complexity
        let complexity = self.analyze_query_complexity(query);
        let execution_time = Duration::from_millis(complexity * 10);

        tokio::time::sleep(execution_time).await;

        Ok(QueryExecutionResult {
            io_operations: complexity * 5,
            cache_hits: if complexity > 5 { complexity * 2 } else { 0 },
            cache_misses: if complexity > 5 { complexity } else { 0 },
        })
    }

    /// Analyze query complexity
    fn analyze_query_complexity(&self, query: &str) -> u64 {
        let query_lower = query.to_lowercase();
        let mut complexity = 1;

        // Basic complexity factors
        if query_lower.contains("match") {
            complexity += 1;
        }
        if query_lower.contains("where") {
            complexity += 1;
        }
        if query_lower.contains("order by") {
            complexity += 2;
        }
        if query_lower.contains("group by") {
            complexity += 2;
        }
        if query_lower.contains("limit") {
            complexity -= 1;
        }

        // Relationship traversal complexity
        let relationship_count =
            query_lower.matches("->").count() + query_lower.matches("<-").count();
        complexity += relationship_count as u64 * 2;

        // Vector operations complexity
        if query_lower.contains("vector") || query_lower.contains("<->") {
            complexity += 3;
        }

        // Subquery complexity
        let subquery_count = query_lower.matches("(").count();
        complexity += subquery_count as u64;

        complexity.max(1)
    }

    /// Generate performance recommendations
    fn generate_recommendations(
        &self,
        query: &str,
        execution_time: Duration,
        memory_usage: u64,
        cpu_usage: f64,
    ) -> Vec<String> {
        let mut recommendations = Vec::new();

        if execution_time > self.slow_query_threshold {
            recommendations
                .push("Consider adding indexes for better query performance".to_string());
            recommendations.push("Use LIMIT to reduce result set size".to_string());
        }

        if memory_usage > self.memory_threshold {
            recommendations.push("Consider streaming results for large datasets".to_string());
            recommendations.push("Optimize query to reduce memory footprint".to_string());
        }

        if cpu_usage > self.cpu_threshold {
            recommendations
                .push("Consider parallel processing for CPU-intensive operations".to_string());
            recommendations.push("Optimize algorithm complexity".to_string());
        }

        if query.to_lowercase().contains("vector") {
            recommendations.push("Consider using vector indexes for similarity search".to_string());
            recommendations.push("Optimize vector dimensions if possible".to_string());
        }

        if query.to_lowercase().contains("match") && !query.to_lowercase().contains("limit") {
            recommendations.push("Add LIMIT clause to prevent unbounded result sets".to_string());
        }

        recommendations
    }

    /// Get current memory usage (simplified)
    fn get_memory_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(1024 * 1024 * 100) // 100MB placeholder
    }

    /// Get current CPU usage (simplified)
    fn get_cpu_usage(&self) -> Result<f64, Box<dyn std::error::Error>> {
        // In a real implementation, this would use system APIs
        Ok(25.0) // 25% placeholder
    }
}

impl Default for QueryProfiler {
    fn default() -> Self {
        Self {
            query_history: RwLock::new(Vec::new()),
            slow_query_threshold: Duration::from_millis(100),
            memory_threshold: 10 * 1024 * 1024,
            cpu_threshold: 80.0,
        }
    }
}

/// Query execution result
#[derive(Debug, Clone)]
struct QueryExecutionResult {
    io_operations: u64,
    cache_hits: u64,
    cache_misses: u64,
}

/// Query performance statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryStatistics {
    pub total_queries: usize,
    pub avg_execution_time: Duration,
    pub slow_queries: usize,
    pub memory_intensive_queries: usize,
    pub cpu_intensive_queries: usize,
    pub cache_hit_rate: f64,
    pub total_execution_time: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_query_profiler_creation() {
        let profiler = QueryProfiler::new().unwrap();
        assert_eq!(profiler.slow_query_threshold, Duration::from_millis(100));
        assert_eq!(profiler.memory_threshold, 10 * 1024 * 1024);
        assert_eq!(profiler.cpu_threshold, 80.0);
    }

    #[tokio::test]
    async fn test_query_profiling() {
        let profiler = QueryProfiler::new().unwrap();
        let query = "MATCH (n:User) WHERE n.id = 1 RETURN n.name";

        let profile = profiler.profile_query(query).await.unwrap();
        assert_eq!(profile.query, query);
        assert!(!profile.recommendations.is_empty());
    }

    #[tokio::test]
    async fn test_query_statistics() {
        let profiler = QueryProfiler::new().unwrap();

        // Profile a few queries
        profiler
            .profile_query("MATCH (n:User) RETURN n")
            .await
            .unwrap();
        profiler
            .profile_query("MATCH (n:User)-[:FOLLOWS]->(m:User) RETURN n, m")
            .await
            .unwrap();

        let stats = profiler.get_statistics().await;
        assert_eq!(stats.total_queries, 2);
        assert!(stats.avg_execution_time > Duration::from_millis(0));
    }

    #[tokio::test]
    async fn test_slow_query_detection() {
        let mut profiler = QueryProfiler::new().unwrap();
        profiler.set_slow_query_threshold(Duration::from_millis(50));

        // Profile a complex query that should be slow
        profiler
            .profile_query(
                "MATCH (n:User)-[:FOLLOWS]->(m:User)-[:FOLLOWS]->(o:User) RETURN n, m, o",
            )
            .await
            .unwrap();

        let slow_queries = profiler.get_slow_queries(10).await;
        assert!(!slow_queries.is_empty());
    }

    #[tokio::test]
    async fn test_optimization_recommendations() {
        let profiler = QueryProfiler::new().unwrap();

        // Profile some queries to generate recommendations
        profiler
            .profile_query("MATCH (n:User) RETURN n")
            .await
            .unwrap();

        let recommendations = profiler.get_optimization_recommendations().await;
        assert!(!recommendations.is_empty());
    }
}
