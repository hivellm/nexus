//! MCP Tool Statistics and Performance Monitoring
//!
//! This module provides:
//! - MCP tool execution time tracking
//! - Tool usage statistics
//! - Performance metrics per tool
//! - Slow tool call logging

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// MCP tool execution record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolRecord {
    /// Tool name
    pub tool_name: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp when tool was executed
    pub timestamp: u64,
    /// Whether tool call succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Input size in bytes (approximate)
    pub input_size_bytes: Option<u64>,
    /// Output size in bytes (approximate)
    pub output_size_bytes: Option<u64>,
    /// Cache hit (if applicable)
    pub cache_hit: Option<bool>,
}

/// Slow tool call log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowToolLog {
    /// Tool records
    pub records: VecDeque<McpToolRecord>,
    /// Maximum number of entries to keep
    pub max_entries: usize,
    /// Slow tool threshold in milliseconds
    pub threshold_ms: u64,
}

impl SlowToolLog {
    /// Create a new slow tool log
    pub fn new(threshold_ms: u64, max_entries: usize) -> Self {
        Self {
            records: VecDeque::with_capacity(max_entries),
            max_entries,
            threshold_ms,
        }
    }

    /// Add a tool record if it exceeds the threshold
    pub fn add_record(&mut self, record: McpToolRecord) {
        if record.execution_time_ms >= self.threshold_ms {
            if self.records.len() >= self.max_entries {
                self.records.pop_front();
            }
            self.records.push_back(record);
        }
    }

    /// Get all slow tool records
    pub fn get_records(&self) -> Vec<McpToolRecord> {
        self.records.iter().cloned().collect()
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.records.clear();
    }

    /// Get count of slow tool calls
    pub fn count(&self) -> usize {
        self.records.len()
    }
}

/// Statistics for a specific tool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    /// Tool name
    pub tool_name: String,
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
    /// Total input size in bytes
    pub total_input_bytes: u64,
    /// Total output size in bytes
    pub total_output_bytes: u64,
    /// Cache hits (if applicable)
    pub cache_hits: u64,
    /// Cache misses (if applicable)
    pub cache_misses: u64,
}

impl ToolStats {
    /// Create new tool statistics
    pub fn new(tool_name: String) -> Self {
        Self {
            tool_name,
            count: 0,
            total_time_ms: 0,
            avg_time_ms: 0.0,
            min_time_ms: u64::MAX,
            max_time_ms: 0,
            success_count: 0,
            failure_count: 0,
            total_input_bytes: 0,
            total_output_bytes: 0,
            cache_hits: 0,
            cache_misses: 0,
        }
    }

    /// Record a tool execution
    pub fn record_execution(
        &mut self,
        execution_time_ms: u64,
        success: bool,
        input_size_bytes: Option<u64>,
        output_size_bytes: Option<u64>,
        cache_hit: Option<bool>,
    ) {
        self.count += 1;
        self.total_time_ms += execution_time_ms;
        self.avg_time_ms = self.total_time_ms as f64 / self.count as f64;

        if execution_time_ms < self.min_time_ms {
            self.min_time_ms = execution_time_ms;
        }
        if execution_time_ms > self.max_time_ms {
            self.max_time_ms = execution_time_ms;
        }

        if success {
            self.success_count += 1;
        } else {
            self.failure_count += 1;
        }

        if let Some(input_size) = input_size_bytes {
            self.total_input_bytes += input_size;
        }

        if let Some(output_size) = output_size_bytes {
            self.total_output_bytes += output_size;
        }

        if let Some(hit) = cache_hit {
            if hit {
                self.cache_hits += 1;
            } else {
                self.cache_misses += 1;
            }
        }
    }
}

/// MCP tool statistics tracker
pub struct McpToolStatistics {
    /// Total tool calls executed
    pub total_calls: Arc<AtomicU64>,
    /// Successful tool calls
    pub successful_calls: Arc<AtomicU64>,
    /// Failed tool calls
    pub failed_calls: Arc<AtomicU64>,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: Arc<AtomicU64>,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: Arc<AtomicU64>,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: Arc<AtomicU64>,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: Arc<AtomicU64>,
    /// Slow tool log
    pub slow_tool_log: Arc<RwLock<SlowToolLog>>,
    /// Statistics per tool
    pub tool_stats: Arc<RwLock<HashMap<String, ToolStats>>>,
}

impl McpToolStatistics {
    /// Create new MCP tool statistics tracker
    pub fn new(slow_tool_threshold_ms: u64, max_slow_tools: usize) -> Self {
        Self {
            total_calls: Arc::new(AtomicU64::new(0)),
            successful_calls: Arc::new(AtomicU64::new(0)),
            failed_calls: Arc::new(AtomicU64::new(0)),
            total_execution_time_ms: Arc::new(AtomicU64::new(0)),
            average_execution_time_ms: Arc::new(AtomicU64::new(0)),
            min_execution_time_ms: Arc::new(AtomicU64::new(u64::MAX)),
            max_execution_time_ms: Arc::new(AtomicU64::new(0)),
            slow_tool_log: Arc::new(RwLock::new(SlowToolLog::new(
                slow_tool_threshold_ms,
                max_slow_tools,
            ))),
            tool_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Record a tool execution
    #[allow(clippy::too_many_arguments)]
    pub fn record_tool_call(
        &self,
        tool_name: &str,
        execution_time: Duration,
        success: bool,
        error: Option<String>,
        input_size_bytes: Option<u64>,
        output_size_bytes: Option<u64>,
        cache_hit: Option<bool>,
    ) {
        let time_ms = execution_time.as_millis() as u64;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Update global statistics
        self.total_calls.fetch_add(1, Ordering::SeqCst);
        if success {
            self.successful_calls.fetch_add(1, Ordering::SeqCst);
        } else {
            self.failed_calls.fetch_add(1, Ordering::SeqCst);
        }

        // Update execution time statistics
        let total_time = self
            .total_execution_time_ms
            .fetch_add(time_ms, Ordering::SeqCst)
            + time_ms;
        let total_count = self.total_calls.load(Ordering::SeqCst);
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

        // Update per-tool statistics
        {
            let mut tool_stats = self.tool_stats.write().unwrap();
            let stats = tool_stats
                .entry(tool_name.to_string())
                .or_insert_with(|| ToolStats::new(tool_name.to_string()));
            stats.record_execution(
                time_ms,
                success,
                input_size_bytes,
                output_size_bytes,
                cache_hit,
            );
        }

        // Add to slow tool log if exceeds threshold
        {
            let record = McpToolRecord {
                tool_name: tool_name.to_string(),
                execution_time_ms: time_ms,
                timestamp,
                success,
                error,
                input_size_bytes,
                output_size_bytes,
                cache_hit,
            };
            self.slow_tool_log.write().unwrap().add_record(record);
        }
    }

    /// Get statistics for a specific tool
    pub fn get_tool_stats(&self, tool_name: &str) -> Option<ToolStats> {
        self.tool_stats.read().unwrap().get(tool_name).cloned()
    }

    /// Get all tool statistics
    pub fn get_all_tool_stats(&self) -> Vec<ToolStats> {
        self.tool_stats.read().unwrap().values().cloned().collect()
    }

    /// Get slow tool records
    pub fn get_slow_tools(&self) -> Vec<McpToolRecord> {
        self.slow_tool_log.read().unwrap().get_records()
    }

    /// Get overall statistics summary
    pub fn get_statistics(&self) -> McpToolStatisticsSummary {
        McpToolStatisticsSummary {
            total_calls: self.total_calls.load(Ordering::SeqCst),
            successful_calls: self.successful_calls.load(Ordering::SeqCst),
            failed_calls: self.failed_calls.load(Ordering::SeqCst),
            total_execution_time_ms: self.total_execution_time_ms.load(Ordering::SeqCst),
            average_execution_time_ms: self.average_execution_time_ms.load(Ordering::SeqCst),
            min_execution_time_ms: {
                let min = self.min_execution_time_ms.load(Ordering::SeqCst);
                if min == u64::MAX { 0 } else { min }
            },
            max_execution_time_ms: self.max_execution_time_ms.load(Ordering::SeqCst),
            slow_tool_count: self.slow_tool_log.read().unwrap().count(),
        }
    }

    /// Clear all statistics
    pub fn clear(&self) {
        self.total_calls.store(0, Ordering::SeqCst);
        self.successful_calls.store(0, Ordering::SeqCst);
        self.failed_calls.store(0, Ordering::SeqCst);
        self.total_execution_time_ms.store(0, Ordering::SeqCst);
        self.average_execution_time_ms.store(0, Ordering::SeqCst);
        self.min_execution_time_ms.store(u64::MAX, Ordering::SeqCst);
        self.max_execution_time_ms.store(0, Ordering::SeqCst);
        self.slow_tool_log.write().unwrap().clear();
        self.tool_stats.write().unwrap().clear();
    }
}

/// MCP tool statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolStatisticsSummary {
    /// Total tool calls executed
    pub total_calls: u64,
    /// Successful tool calls
    pub successful_calls: u64,
    /// Failed tool calls
    pub failed_calls: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Number of slow tool calls logged
    pub slow_tool_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_mcp_tool_statistics_creation() {
        let stats = McpToolStatistics::new(100, 1000);
        let summary = stats.get_statistics();
        assert_eq!(summary.total_calls, 0);
        assert_eq!(summary.successful_calls, 0);
        assert_eq!(summary.failed_calls, 0);
    }

    #[test]
    fn test_record_tool_call() {
        let stats = McpToolStatistics::new(100, 1000);
        stats.record_tool_call(
            "graph_correlation_generate",
            Duration::from_millis(50),
            true,
            None,
            Some(1024),
            Some(2048),
            Some(false),
        );

        let summary = stats.get_statistics();
        assert_eq!(summary.total_calls, 1);
        assert_eq!(summary.successful_calls, 1);
        assert_eq!(summary.failed_calls, 0);
        assert_eq!(summary.total_execution_time_ms, 50);
        assert_eq!(summary.average_execution_time_ms, 50);
    }

    #[test]
    fn test_tool_stats() {
        let stats = McpToolStatistics::new(100, 1000);
        stats.record_tool_call(
            "graph_correlation_generate",
            Duration::from_millis(50),
            true,
            None,
            Some(1024),
            Some(2048),
            Some(false),
        );
        stats.record_tool_call(
            "graph_correlation_generate",
            Duration::from_millis(75),
            true,
            None,
            Some(2048),
            Some(4096),
            Some(true),
        );

        let tool_stats = stats.get_tool_stats("graph_correlation_generate");
        assert!(tool_stats.is_some());
        let tool_stats = tool_stats.unwrap();
        assert_eq!(tool_stats.count, 2);
        assert_eq!(tool_stats.total_time_ms, 125);
        assert_eq!(tool_stats.avg_time_ms, 62.5);
        assert_eq!(tool_stats.min_time_ms, 50);
        assert_eq!(tool_stats.max_time_ms, 75);
        assert_eq!(tool_stats.success_count, 2);
        assert_eq!(tool_stats.cache_hits, 1);
        assert_eq!(tool_stats.cache_misses, 1);
    }

    #[test]
    fn test_slow_tool_logging() {
        let stats = McpToolStatistics::new(100, 1000);
        stats.record_tool_call(
            "graph_correlation_generate",
            Duration::from_millis(150),
            true,
            None,
            None,
            None,
            None,
        );

        let slow_tools = stats.get_slow_tools();
        assert_eq!(slow_tools.len(), 1);
        assert_eq!(slow_tools[0].tool_name, "graph_correlation_generate");
        assert_eq!(slow_tools[0].execution_time_ms, 150);
    }

    #[test]
    fn test_clear_statistics() {
        let stats = McpToolStatistics::new(100, 1000);
        stats.record_tool_call(
            "graph_correlation_generate",
            Duration::from_millis(50),
            true,
            None,
            None,
            None,
            None,
        );
        stats.clear();

        let summary = stats.get_statistics();
        assert_eq!(summary.total_calls, 0);
        assert_eq!(summary.successful_calls, 0);
    }
}
