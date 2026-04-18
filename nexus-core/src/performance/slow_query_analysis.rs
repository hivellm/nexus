//! Slow query analysis tools
//!
//! This module provides tools for analyzing slow queries:
//! - Pattern detection
//! - Performance recommendations
//! - Query optimization suggestions

use crate::performance::query_stats::{QueryRecord, QueryStatistics};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Slow query analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryAnalysis {
    /// Query pattern
    pub pattern: String,
    /// Number of occurrences
    pub occurrences: usize,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Slow query analyzer
pub struct SlowQueryAnalyzer {
    /// Minimum occurrences to consider a pattern
    min_occurrences: usize,
}

impl SlowQueryAnalyzer {
    /// Create a new slow query analyzer
    pub fn new() -> Self {
        Self { min_occurrences: 2 }
    }

    /// Analyze slow queries and generate recommendations
    pub fn analyze(&self, stats: &QueryStatistics) -> Vec<SlowQueryAnalysis> {
        let slow_queries = stats.get_slow_queries();
        let mut pattern_map: HashMap<String, Vec<&QueryRecord>> = HashMap::new();

        // Group queries by pattern
        for query in &slow_queries {
            let pattern = self.normalize_query(&query.query);
            pattern_map.entry(pattern).or_default().push(query);
        }

        // Analyze each pattern
        let mut analyses = Vec::new();
        for (pattern, queries) in pattern_map {
            if queries.len() >= self.min_occurrences {
                let total_time: u64 = queries.iter().map(|q| q.execution_time_ms).sum();
                let avg_time = total_time as f64 / queries.len() as f64;

                let recommendations = self.generate_recommendations(&pattern, &queries);

                analyses.push(SlowQueryAnalysis {
                    pattern,
                    occurrences: queries.len(),
                    avg_execution_time_ms: avg_time,
                    total_execution_time_ms: total_time,
                    recommendations,
                });
            }
        }

        // Sort by total execution time (descending)
        analyses.sort_by(|a, b| b.total_execution_time_ms.cmp(&a.total_execution_time_ms));
        analyses
    }

    /// Normalize query for pattern matching
    fn normalize_query(&self, query: &str) -> String {
        // Simple normalization: remove string literals and numbers, normalize whitespace
        let mut normalized = query.to_string();

        // Remove string literals
        normalized = regex::Regex::new(r#""[^"]*""#)
            .unwrap()
            .replace_all(&normalized, "?")
            .to_string();
        normalized = regex::Regex::new(r"'[^']*'")
            .unwrap()
            .replace_all(&normalized, "?")
            .to_string();

        // Remove numbers
        normalized = regex::Regex::new(r"\b\d+\b")
            .unwrap()
            .replace_all(&normalized, "?")
            .to_string();

        // Normalize whitespace
        normalized = regex::Regex::new(r"\s+")
            .unwrap()
            .replace_all(&normalized, " ")
            .to_string();

        normalized.trim().to_string()
    }

    /// Generate recommendations for a query pattern
    fn generate_recommendations(&self, pattern: &str, queries: &[&QueryRecord]) -> Vec<String> {
        let mut recommendations = Vec::new();

        // Check for common patterns
        let pattern_upper = pattern.to_uppercase();

        // Missing WHERE clause
        if pattern_upper.contains("MATCH") && !pattern_upper.contains("WHERE") {
            recommendations
                .push("Consider adding WHERE clause to filter results early".to_string());
        }

        // Full graph scan
        if pattern_upper.contains("MATCH (N)") || pattern_upper.contains("MATCH (N)") {
            recommendations.push(
                "Query scans entire graph - consider adding labels or properties to filter"
                    .to_string(),
            );
        }

        // No LIMIT
        if !pattern_upper.contains("LIMIT") {
            recommendations.push("Consider adding LIMIT to restrict result size".to_string());
        }

        // High average execution time
        let avg_time: f64 = queries
            .iter()
            .map(|q| q.execution_time_ms as f64)
            .sum::<f64>()
            / queries.len() as f64;
        if avg_time > 1000.0 {
            recommendations.push(format!(
                "Average execution time is {:.0}ms - consider query optimization or indexing",
                avg_time
            ));
        }

        // Check for high memory usage
        let high_memory_queries: Vec<_> = queries
            .iter()
            .filter(|q| {
                q.memory_usage
                    .map(|m| m > 10 * 1024 * 1024) // > 10MB
                    .unwrap_or(false)
            })
            .collect();
        if !high_memory_queries.is_empty() {
            recommendations.push(
                "Some queries use high memory - consider adding LIMIT or optimizing joins"
                    .to_string(),
            );
        }

        // Check for low cache hit rate
        let queries_with_cache: Vec<_> = queries
            .iter()
            .filter(|q| q.cache_hits.is_some() && q.cache_misses.is_some())
            .collect();
        if !queries_with_cache.is_empty() {
            let total_hits: u64 = queries_with_cache
                .iter()
                .map(|q| q.cache_hits.unwrap_or(0))
                .sum();
            let total_misses: u64 = queries_with_cache
                .iter()
                .map(|q| q.cache_misses.unwrap_or(0))
                .sum();
            let hit_rate = if total_hits + total_misses > 0 {
                total_hits as f64 / (total_hits + total_misses) as f64
            } else {
                0.0
            };

            if hit_rate < 0.5 {
                recommendations.push(format!(
                    "Low cache hit rate ({:.1}%) - query may benefit from plan caching",
                    hit_rate * 100.0
                ));
            }
        }

        if recommendations.is_empty() {
            recommendations
                .push("No specific recommendations - query performance is acceptable".to_string());
        }

        recommendations
    }
}

impl Default for SlowQueryAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::performance::query_stats::QueryStatistics;
    use std::time::Duration;

    #[test]
    fn test_slow_query_analyzer() {
        let analyzer = SlowQueryAnalyzer::new();
        let stats = QueryStatistics::new(100, 1000);

        // Record some slow queries
        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(150),
            true,
            None,
            100,
        );
        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(200),
            true,
            None,
            200,
        );

        let analyses = analyzer.analyze(&stats);
        assert!(!analyses.is_empty());
    }

    #[test]
    fn test_query_normalization() {
        let analyzer = SlowQueryAnalyzer::new();
        let pattern1 = analyzer.normalize_query("MATCH (n) WHERE n.age = 25 RETURN n");
        let pattern2 = analyzer.normalize_query("MATCH (n) WHERE n.age = 30 RETURN n");

        assert_eq!(pattern1, pattern2);
    }
}
