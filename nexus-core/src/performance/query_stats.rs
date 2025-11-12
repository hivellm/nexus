//! Query statistics and slow query logging
//!
//! This module provides:
//! - Query execution time tracking
//! - Slow query logging
//! - Query statistics storage
//! - Query plan caching

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Query execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryRecord {
    /// Query text
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp when query was executed
    pub timestamp: u64,
    /// Whether query succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Number of rows returned
    pub rows_returned: usize,
    /// Memory usage in bytes (if available)
    pub memory_usage: Option<u64>,
}

/// Slow query log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowQueryLog {
    /// Query records
    pub queries: VecDeque<QueryRecord>,
    /// Maximum number of entries to keep
    pub max_entries: usize,
    /// Slow query threshold in milliseconds
    pub threshold_ms: u64,
}

impl SlowQueryLog {
    /// Create a new slow query log
    pub fn new(threshold_ms: u64, max_entries: usize) -> Self {
        Self {
            queries: VecDeque::with_capacity(max_entries),
            max_entries,
            threshold_ms,
        }
    }

    /// Add a query record if it exceeds the threshold
    pub fn add_query(&mut self, record: QueryRecord) {
        if record.execution_time_ms >= self.threshold_ms {
            if self.queries.len() >= self.max_entries {
                self.queries.pop_front();
            }
            self.queries.push_back(record);
        }
    }

    /// Get all slow queries
    pub fn get_queries(&self) -> Vec<QueryRecord> {
        self.queries.iter().cloned().collect()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.queries.clear();
    }

    /// Get count of slow queries
    pub fn count(&self) -> usize {
        self.queries.len()
    }
}

/// Query statistics tracker
pub struct QueryStatistics {
    /// Total queries executed
    pub total_queries: Arc<AtomicU64>,
    /// Successful queries
    pub successful_queries: Arc<AtomicU64>,
    /// Failed queries
    pub failed_queries: Arc<AtomicU64>,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: Arc<AtomicU64>,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: Arc<AtomicU64>,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: Arc<AtomicU64>,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: Arc<AtomicU64>,
    /// Slow query log
    pub slow_query_log: Arc<RwLock<SlowQueryLog>>,
    /// Query statistics by query pattern (normalized)
    pub query_pattern_stats: Arc<RwLock<HashMap<String, QueryPatternStats>>>,
}

/// Statistics for a query pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPatternStats {
    /// Query pattern (normalized)
    pub pattern: String,
    /// Execution count
    pub count: u64,
    /// Total execution time in milliseconds
    pub total_time_ms: u64,
    /// Average execution time in milliseconds
    pub avg_time_ms: f64,
    /// Minimum execution time in milliseconds
    pub min_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_time_ms: u64,
    /// Success count
    pub success_count: u64,
    /// Failure count
    pub failure_count: u64,
}

impl QueryStatistics {
    /// Create new query statistics tracker
    pub fn new(slow_query_threshold_ms: u64, max_slow_queries: usize) -> Self {
        Self {
            total_queries: Arc::new(AtomicU64::new(0)),
            successful_queries: Arc::new(AtomicU64::new(0)),
            failed_queries: Arc::new(AtomicU64::new(0)),
            total_execution_time_ms: Arc::new(AtomicU64::new(0)),
            average_execution_time_ms: Arc::new(AtomicU64::new(0)),
            min_execution_time_ms: Arc::new(AtomicU64::new(u64::MAX)),
            max_execution_time_ms: Arc::new(AtomicU64::new(0)),
            slow_query_log: Arc::new(RwLock::new(SlowQueryLog::new(
                slow_query_threshold_ms,
                max_slow_queries,
            ))),
            query_pattern_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a query execution
    pub fn record_query(
        &self,
        query: &str,
        execution_time: Duration,
        success: bool,
        error: Option<String>,
        rows_returned: usize,
    ) {
        let time_ms = execution_time.as_millis() as u64;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Update global statistics
        self.total_queries.fetch_add(1, Ordering::SeqCst);
        if success {
            self.successful_queries.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failed_queries.fetch_add(1, Ordering::SeqCst);
        }

        // Update execution time statistics
        let total_time = self
            .total_execution_time_ms
            .fetch_add(time_ms, Ordering::SeqCst)
            + time_ms;
        let total_count = self.total_queries.load(Ordering::SeqCst);
        self.average_execution_time_ms
            .store(total_time / total_count.max(1), Ordering::SeqCst);

        // Update min/max
        loop {
            let current_min = self.min_execution_time_ms.load(Ordering::SeqCst);
            if time_ms < current_min {
                if self
                    .min_execution_time_ms
                    .compare_exchange(current_min, time_ms, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            } else {
                break;
            }
        }

        loop {
            let current_max = self.max_execution_time_ms.load(Ordering::SeqCst);
            if time_ms > current_max {
                if self
                    .max_execution_time_ms
                    .compare_exchange(current_max, time_ms, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    break;
                }
            } else {
                break;
            }
        }

        // Add to slow query log if applicable
        let record = QueryRecord {
            query: query.to_string(),
            execution_time_ms: time_ms,
            timestamp,
            success,
            error: error.clone(),
            rows_returned,
            memory_usage: None,
        };

        self.slow_query_log.write().unwrap().add_query(record);

        // Update pattern statistics
        let pattern = self.normalize_query(query);
        let pattern_clone = pattern.clone();
        let mut pattern_stats = self.query_pattern_stats.write().unwrap();
        let stats = pattern_stats
            .entry(pattern)
            .or_insert_with(|| QueryPatternStats {
                pattern: pattern_clone,
                count: 0,
                total_time_ms: 0,
                avg_time_ms: 0.0,
                min_time_ms: u64::MAX,
                max_time_ms: 0,
                success_count: 0,
                failure_count: 0,
            });

        stats.count += 1;
        stats.total_time_ms += time_ms;
        stats.avg_time_ms = stats.total_time_ms as f64 / stats.count as f64;
        if time_ms < stats.min_time_ms {
            stats.min_time_ms = time_ms;
        }
        if time_ms > stats.max_time_ms {
            stats.max_time_ms = time_ms;
        }
        if success {
            stats.success_count += 1;
        } else {
            stats.failure_count += 1;
        }
    }

    /// Normalize query for pattern matching (remove parameters, normalize whitespace)
    fn normalize_query(&self, query: &str) -> String {
        // Simple normalization: remove string literals and numbers, normalize whitespace
        let mut normalized = query.to_string();

        // Remove string literals (basic approach)
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

    /// Get slow queries
    pub fn get_slow_queries(&self) -> Vec<QueryRecord> {
        self.slow_query_log.read().unwrap().get_queries()
    }

    /// Get query pattern statistics
    pub fn get_pattern_stats(&self) -> HashMap<String, QueryPatternStats> {
        self.query_pattern_stats.read().unwrap().clone()
    }

    /// Get overall statistics
    pub fn get_statistics(&self) -> QueryStatisticsSummary {
        QueryStatisticsSummary {
            total_queries: self.total_queries.load(Ordering::SeqCst),
            successful_queries: self.successful_queries.load(Ordering::SeqCst),
            failed_queries: self.failed_queries.load(Ordering::SeqCst),
            total_execution_time_ms: self.total_execution_time_ms.load(Ordering::SeqCst),
            average_execution_time_ms: self.average_execution_time_ms.load(Ordering::SeqCst),
            min_execution_time_ms: {
                let min = self.min_execution_time_ms.load(Ordering::SeqCst);
                if min == u64::MAX { 0 } else { min }
            },
            max_execution_time_ms: self.max_execution_time_ms.load(Ordering::SeqCst),
            slow_query_count: self.slow_query_log.read().unwrap().count(),
        }
    }

    /// Clear all statistics
    pub fn clear(&self) {
        self.total_queries.store(0, Ordering::SeqCst);
        self.successful_queries.store(0, Ordering::SeqCst);
        self.failed_queries.store(0, Ordering::SeqCst);
        self.total_execution_time_ms.store(0, Ordering::SeqCst);
        self.average_execution_time_ms.store(0, Ordering::SeqCst);
        self.min_execution_time_ms.store(u64::MAX, Ordering::SeqCst);
        self.max_execution_time_ms.store(0, Ordering::SeqCst);
        self.slow_query_log.write().unwrap().clear();
        self.query_pattern_stats.write().unwrap().clear();
    }
}

/// Query statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryStatisticsSummary {
    /// Total queries executed
    pub total_queries: u64,
    /// Successful queries
    pub successful_queries: u64,
    /// Failed queries
    pub failed_queries: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Number of slow queries logged
    pub slow_query_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_statistics() {
        let stats = QueryStatistics::new(100, 1000);

        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(50),
            true,
            None,
            10,
        );

        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(150),
            true,
            None,
            20,
        );

        let summary = stats.get_statistics();
        assert_eq!(summary.total_queries, 2);
        assert_eq!(summary.successful_queries, 2);
        assert_eq!(summary.slow_query_count, 1); // Only the 150ms query is slow
    }

    #[test]
    fn test_slow_query_log() {
        let mut log = SlowQueryLog::new(100, 10);

        log.add_query(QueryRecord {
            query: "MATCH (n) RETURN n".to_string(),
            execution_time_ms: 50,
            timestamp: 0,
            success: true,
            error: None,
            rows_returned: 10,
            memory_usage: None,
        });

        assert_eq!(log.count(), 0); // Below threshold

        log.add_query(QueryRecord {
            query: "MATCH (n) RETURN n".to_string(),
            execution_time_ms: 150,
            timestamp: 0,
            success: true,
            error: None,
            rows_returned: 10,
            memory_usage: None,
        });

        assert_eq!(log.count(), 1); // Above threshold
    }

    #[test]
    fn test_query_statistics_empty() {
        let stats = QueryStatistics::new(100, 1000);
        let summary = stats.get_statistics();

        assert_eq!(summary.total_queries, 0);
        assert_eq!(summary.successful_queries, 0);
        assert_eq!(summary.failed_queries, 0);
        assert_eq!(summary.min_execution_time_ms, 0);
    }

    #[test]
    fn test_query_statistics_failed_queries() {
        let stats = QueryStatistics::new(100, 1000);

        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(50),
            false,
            Some("Error".to_string()),
            0,
        );

        stats.record_query("CREATE (n)", Duration::from_millis(30), true, None, 1);

        let summary = stats.get_statistics();
        assert_eq!(summary.total_queries, 2);
        assert_eq!(summary.successful_queries, 1);
        assert_eq!(summary.failed_queries, 1);
    }

    #[test]
    fn test_query_statistics_min_max() {
        let stats = QueryStatistics::new(100, 1000);

        stats.record_query("QUERY1", Duration::from_millis(10), true, None, 0);
        stats.record_query("QUERY2", Duration::from_millis(1000), true, None, 0);
        stats.record_query("QUERY3", Duration::from_millis(100), true, None, 0);

        let summary = stats.get_statistics();
        assert_eq!(summary.min_execution_time_ms, 10);
        assert_eq!(summary.max_execution_time_ms, 1000);
    }

    #[test]
    fn test_query_pattern_statistics() {
        let stats = QueryStatistics::new(100, 1000);

        // Record same pattern multiple times
        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(50),
            true,
            None,
            10,
        );
        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(60),
            true,
            None,
            20,
        );
        stats.record_query(
            "MATCH (n) RETURN n",
            Duration::from_millis(40),
            true,
            None,
            15,
        );

        // Record different pattern
        stats.record_query(
            "CREATE (n:Person)",
            Duration::from_millis(30),
            true,
            None,
            1,
        );

        let patterns = stats.get_pattern_stats();
        assert_eq!(patterns.len(), 2);

        // Check that patterns are normalized
        let pattern_keys: Vec<String> = patterns.keys().cloned().collect();
        assert!(pattern_keys.iter().any(|k| k.contains("MATCH")));
        assert!(pattern_keys.iter().any(|k| k.contains("CREATE")));
    }

    #[test]
    fn test_query_statistics_clear() {
        let stats = QueryStatistics::new(100, 1000);

        stats.record_query("QUERY1", Duration::from_millis(50), true, None, 10);
        stats.record_query("QUERY2", Duration::from_millis(150), true, None, 20);

        let summary_before = stats.get_statistics();
        assert_eq!(summary_before.total_queries, 2);

        stats.clear();

        let summary_after = stats.get_statistics();
        assert_eq!(summary_after.total_queries, 0);
        assert_eq!(summary_after.successful_queries, 0);
        assert_eq!(summary_after.slow_query_count, 0);
    }

    #[test]
    fn test_slow_query_log_eviction() {
        let mut log = SlowQueryLog::new(100, 3); // Max 3 entries

        // Add 5 slow queries
        for i in 0..5 {
            log.add_query(QueryRecord {
                query: format!("QUERY{}", i),
                execution_time_ms: 150,
                timestamp: i,
                success: true,
                error: None,
                rows_returned: 10,
                memory_usage: None,
            });
        }

        // Should only keep last 3
        assert_eq!(log.count(), 3);
        let queries = log.get_queries();
        assert_eq!(queries.len(), 3);
        // First two should be evicted
        assert!(!queries.iter().any(|q| q.query == "QUERY0"));
        assert!(!queries.iter().any(|q| q.query == "QUERY1"));
    }

    #[test]
    fn test_slow_query_log_clear() {
        let mut log = SlowQueryLog::new(100, 10);

        log.add_query(QueryRecord {
            query: "QUERY1".to_string(),
            execution_time_ms: 150,
            timestamp: 0,
            success: true,
            error: None,
            rows_returned: 10,
            memory_usage: None,
        });

        assert_eq!(log.count(), 1);
        log.clear();
        assert_eq!(log.count(), 0);
    }

    #[test]
    fn test_query_normalization() {
        let stats = QueryStatistics::new(100, 1000);

        // These should be normalized to the same pattern
        stats.record_query(
            "MATCH (n) WHERE n.age = 25 RETURN n",
            Duration::from_millis(50),
            true,
            None,
            1,
        );
        stats.record_query(
            "MATCH (n) WHERE n.age = 30 RETURN n",
            Duration::from_millis(60),
            true,
            None,
            1,
        );

        let patterns = stats.get_pattern_stats();
        // Should have one pattern (normalized)
        assert_eq!(patterns.len(), 1);
    }
}
