//! Adaptive Join Selection and Execution
//!
//! This module provides intelligent join algorithm selection
//! and execution based on data characteristics and runtime statistics.

use crate::error::{Error, Result};
use crate::execution::columnar::ColumnarResult;
use crate::execution::joins::hash_join::execute_hash_join;
use crate::execution::joins::merge_join::{execute_merge_join, is_sorted_by, sort_columnar_result};
use crate::execution::joins::{JoinAlgorithm, JoinResult, JoinSelector, JoinStatistics};
use std::time::Instant;

/// Adaptive join executor that automatically selects and executes the best join algorithm
pub struct AdaptiveJoinExecutor {
    enable_adaptive_selection: bool,
    enable_sorting: bool,
    max_memory_mb: usize,
}

impl AdaptiveJoinExecutor {
    /// Create a new adaptive join executor
    pub fn new() -> Self {
        Self {
            enable_adaptive_selection: true,
            enable_sorting: false, // Disabled by default to avoid expensive sorts
            max_memory_mb: 512,    // 512MB default
        }
    }

    /// Execute a join with adaptive algorithm selection
    pub fn execute_join(
        &self,
        left: &ColumnarResult,
        right: &ColumnarResult,
        join_key_left: &str,
        join_key_right: &str,
        left_columns: &[String],
        right_columns: &[String],
    ) -> Result<AdaptiveJoinResult> {
        let start_time = Instant::now();

        // Analyze data characteristics
        let stats = self.analyze_data(left, right, join_key_left, join_key_right)?;

        // Select algorithm
        let algorithm = if self.enable_adaptive_selection {
            self.select_algorithm(&stats)
        } else {
            JoinAlgorithm::HashJoin {
                use_bloom_filter: true,
            } // Default fallback
        };

        // Execute join
        let join_result = self.execute_with_algorithm(
            &algorithm,
            left,
            right,
            join_key_left,
            join_key_right,
            left_columns,
            right_columns,
        )?;

        let execution_time = start_time.elapsed();

        Ok(AdaptiveJoinResult {
            result: join_result,
            algorithm_used: algorithm,
            execution_time,
            statistics: stats,
        })
    }

    /// Analyze data characteristics for join optimization
    fn analyze_data(
        &self,
        left: &ColumnarResult,
        right: &ColumnarResult,
        join_key_left: &str,
        join_key_right: &str,
    ) -> Result<JoinStatistics> {
        // Check if data is sorted
        let left_sorted = is_sorted_by(left, join_key_left).unwrap_or(false);
        let right_sorted = is_sorted_by(right, join_key_right).unwrap_or(false);

        // Estimate selectivity (simplified - in real implementation would analyze actual data distribution)
        let selectivity = if left.row_count > 0 && right.row_count > 0 {
            // Assume 10% selectivity for demonstration
            0.1
        } else {
            0.0
        };

        Ok(JoinStatistics::from_data_sources(
            left.row_count,
            right.row_count,
            left_sorted,
            right_sorted,
        ))
    }

    /// Select the best join algorithm based on data characteristics
    fn select_algorithm(&self, stats: &JoinStatistics) -> JoinAlgorithm {
        let selector = JoinSelector::new(stats.clone());
        selector.select_algorithm()
    }

    /// Execute join with specific algorithm
    fn execute_with_algorithm(
        &self,
        algorithm: &JoinAlgorithm,
        left: &ColumnarResult,
        right: &ColumnarResult,
        join_key_left: &str,
        join_key_right: &str,
        left_columns: &[String],
        right_columns: &[String],
    ) -> Result<JoinResult> {
        match algorithm {
            JoinAlgorithm::HashJoin { use_bloom_filter } => execute_hash_join(
                left,
                right,
                join_key_left,
                join_key_right,
                left_columns,
                right_columns,
                algorithm,
            ),
            JoinAlgorithm::MergeJoin => {
                // For now, fall back to hash join as merge join requires sorted data
                // TODO: Implement proper sorting for merge join
                execute_hash_join(
                    left,
                    right,
                    join_key_left,
                    join_key_right,
                    left_columns,
                    right_columns,
                    &JoinAlgorithm::HashJoin {
                        use_bloom_filter: true,
                    },
                )
            }
            JoinAlgorithm::NestedLoop => {
                // Simple nested loop implementation (not optimized)
                self.execute_nested_loop_join(
                    left,
                    right,
                    join_key_left,
                    join_key_right,
                    left_columns,
                    right_columns,
                )
            }
        }
    }

    /// Execute nested loop join (fallback algorithm)
    fn execute_nested_loop_join(
        &self,
        left: &ColumnarResult,
        right: &ColumnarResult,
        join_key_left: &str,
        join_key_right: &str,
        left_columns: &[String],
        right_columns: &[String],
    ) -> Result<JoinResult> {
        let mut result = JoinResult::new();

        // Initialize result columns
        for col_name in left_columns {
            result.add_left_column(col_name.clone(), Vec::new());
        }
        for col_name in right_columns {
            result.add_right_column(col_name.clone(), Vec::new());
        }

        // Get join key columns
        let left_join_col = left.get_column(join_key_left).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in left data",
                join_key_left
            ))
        })?;

        let right_join_col = right.get_column(join_key_right).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in right data",
                join_key_right
            ))
        })?;

        // Nested loop: O(n*m) - very inefficient but works as fallback
        for left_idx in 0..left.row_count {
            let left_key = self.extract_join_key(left_join_col, left_idx)?;

            for right_idx in 0..right.row_count {
                let right_key = self.extract_join_key(right_join_col, right_idx)?;

                if left_key == right_key {
                    // Keys match - add joined row
                    result.row_count += 1;

                    // Add left side values
                    for col_name in left_columns {
                        let column = left.get_column(col_name).ok_or_else(|| {
                            Error::Storage(format!("Column '{}' not found", col_name))
                        })?;

                        let value = self.extract_value(column, left_idx)?;
                        if let Some(left_col) = result.left_columns.get_mut(col_name) {
                            left_col.push(value);
                        }
                    }

                    // Add right side values
                    for col_name in right_columns {
                        let column = right.get_column(col_name).ok_or_else(|| {
                            Error::Storage(format!("Column '{}' not found", col_name))
                        })?;

                        let value = self.extract_value(column, right_idx)?;
                        if let Some(right_col) = result.right_columns.get_mut(col_name) {
                            right_col.push(value);
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Extract join key from column at given row
    fn extract_join_key(
        &self,
        column: &crate::execution::columnar::Column,
        row_idx: usize,
    ) -> Result<i64> {
        match column.data_type {
            crate::execution::columnar::DataType::Int64 => column.get::<i64>(row_idx),
            _ => Err(Error::Storage("Join keys must be integers".to_string())),
        }
    }

    /// Extract value from column at given row
    fn extract_value(
        &self,
        column: &crate::execution::columnar::Column,
        row_idx: usize,
    ) -> Result<serde_json::Value> {
        match column.data_type {
            crate::execution::columnar::DataType::Int64 => {
                let val = column.get::<i64>(row_idx)?;
                Ok(serde_json::Value::Number(val.into()))
            }
            crate::execution::columnar::DataType::Float64 => {
                let val = column.get::<f64>(row_idx)?;
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(val).unwrap_or(0.into()),
                ))
            }
            crate::execution::columnar::DataType::Bool => {
                let val = column.get::<bool>(row_idx)?;
                Ok(serde_json::Value::Bool(val))
            }
            crate::execution::columnar::DataType::String => {
                // Placeholder for string support
                Ok(serde_json::Value::String("".to_string()))
            }
        }
    }

    /// Configure executor options
    pub fn with_adaptive_selection(mut self, enabled: bool) -> Self {
        self.enable_adaptive_selection = enabled;
        self
    }

    pub fn with_sorting(mut self, enabled: bool) -> Self {
        self.enable_sorting = enabled;
        self
    }

    pub fn with_max_memory_mb(mut self, mb: usize) -> Self {
        self.max_memory_mb = mb;
        self
    }
}

/// Result of an adaptive join execution
#[derive(Debug)]
pub struct AdaptiveJoinResult {
    pub result: JoinResult,
    pub algorithm_used: JoinAlgorithm,
    pub execution_time: std::time::Duration,
    pub statistics: JoinStatistics,
}

impl AdaptiveJoinResult {
    /// Get performance metrics
    pub fn metrics(&self) -> JoinMetrics {
        JoinMetrics {
            row_count: self.result.row_count,
            execution_time_ms: self.execution_time.as_millis() as f64,
            throughput_rows_per_sec: if self.execution_time.as_secs_f64() > 0.0 {
                self.result.row_count as f64 / self.execution_time.as_secs_f64()
            } else {
                0.0
            },
            algorithm: self.algorithm_used.clone(),
            left_cardinality: self.statistics.left_cardinality,
            right_cardinality: self.statistics.right_cardinality,
        }
    }
}

/// Performance metrics for join operations
#[derive(Debug, Clone)]
pub struct JoinMetrics {
    pub row_count: usize,
    pub execution_time_ms: f64,
    pub throughput_rows_per_sec: f64,
    pub algorithm: JoinAlgorithm,
    pub left_cardinality: usize,
    pub right_cardinality: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::columnar::{ColumnarResult, DataType};

    #[test]
    fn test_adaptive_join_executor() {
        let executor = AdaptiveJoinExecutor::new();

        // Create test data
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        id_col.push(3i64).unwrap();
        left.row_count = 3;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        right.row_count = 2;

        // Execute adaptive join
        let result = executor
            .execute_join(
                &left,
                &right,
                "id",
                "id",
                &["id".to_string()],
                &["id".to_string()],
            )
            .unwrap();

        // Should use hash join for small datasets
        match result.algorithm_used {
            JoinAlgorithm::HashJoin { .. } => {} // Expected
            _ => panic!("Expected HashJoin, got {:?}", result.algorithm_used),
        }

        assert_eq!(result.result.row_count, 2); // 2 matching rows
        assert!(result.execution_time.as_micros() > 0);
    }

    #[test]
    fn test_join_metrics() {
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        left.row_count = 2;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(3i64).unwrap();
        right.row_count = 2;

        let executor = AdaptiveJoinExecutor::new();
        let result = executor
            .execute_join(
                &left,
                &right,
                "id",
                "id",
                &["id".to_string()],
                &["id".to_string()],
            )
            .unwrap();

        let metrics = result.metrics();
        assert_eq!(metrics.row_count, 1); // Only id=1 matches
        assert_eq!(metrics.left_cardinality, 2);
        assert_eq!(metrics.right_cardinality, 2);
        assert!(metrics.execution_time_ms >= 0.0);
        assert!(metrics.throughput_rows_per_sec >= 0.0);
    }

    #[test]
    fn test_nested_loop_fallback() {
        let executor = AdaptiveJoinExecutor::new().with_adaptive_selection(false);

        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        left.row_count = 2;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        right.row_count = 2;

        let result = executor
            .execute_join(
                &left,
                &right,
                "id",
                "id",
                &["id".to_string()],
                &["id".to_string()],
            )
            .unwrap();

        // Should use hash join as default when adaptive selection is disabled
        match result.algorithm_used {
            JoinAlgorithm::HashJoin { .. } => {} // Expected
            _ => panic!("Expected HashJoin, got {:?}", result.algorithm_used),
        }
    }

    #[test]
    fn test_data_analysis() {
        let executor = AdaptiveJoinExecutor::new();

        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        id_col.push(3i64).unwrap();
        left.row_count = 3;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        right.row_count = 2;

        let stats = executor.analyze_data(&left, &right, "id", "id").unwrap();
        assert_eq!(stats.left_cardinality, 3);
        assert_eq!(stats.right_cardinality, 2);
        assert!(stats.left_sorted); // Data was inserted in sorted order
        assert!(stats.right_sorted);
    }
}
