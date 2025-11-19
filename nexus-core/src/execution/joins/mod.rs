//! Advanced Join Algorithms for Query Execution
//!
//! This module implements high-performance join algorithms optimized
//! for columnar data and SIMD operations.

pub mod adaptive;
pub mod hash_join;
pub mod merge_join;

use crate::error::{Error, Result};
use crate::execution::columnar::ColumnarResult;
use std::collections::HashMap;

/// Join algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JoinAlgorithm {
    /// Hash join with optional bloom filter
    HashJoin { use_bloom_filter: bool },
    /// Merge join for sorted data
    MergeJoin,
    /// Nested loop join (fallback)
    NestedLoop,
}

/// Join result combining left and right sides
#[derive(Debug)]
pub struct JoinResult {
    pub left_columns: HashMap<String, Vec<serde_json::Value>>,
    pub right_columns: HashMap<String, Vec<serde_json::Value>>,
    pub row_count: usize,
}

impl JoinResult {
    /// Create a new join result
    pub fn new() -> Self {
        Self {
            left_columns: HashMap::new(),
            right_columns: HashMap::new(),
            row_count: 0,
        }
    }

    /// Add a column from the left side
    pub fn add_left_column(&mut self, name: String, values: Vec<serde_json::Value>) {
        self.left_columns.insert(name, values);
    }

    /// Add a column from the right side
    pub fn add_right_column(&mut self, name: String, values: Vec<serde_json::Value>) {
        self.right_columns.insert(name, values);
    }

    /// Convert to columnar result format
    pub fn to_columnar_result(self) -> Result<ColumnarResult> {
        let mut result = ColumnarResult::new();

        // Add left columns with "left_" prefix
        for (name, values) in self.left_columns {
            let col_name = format!("left_{}", name);
            result.add_column(
                col_name.clone(),
                crate::execution::columnar::DataType::Int64,
                values.len(),
            );

            let column = result.get_column_mut(&col_name).unwrap();
            for (i, value) in values.into_iter().enumerate() {
                if let serde_json::Value::Number(num) = value {
                    if let Some(n) = num.as_i64() {
                        column.push(n).unwrap();
                    }
                }
            }
        }

        // Add right columns with "right_" prefix
        for (name, values) in self.right_columns {
            let col_name = format!("right_{}", name);
            result.add_column(
                col_name.clone(),
                crate::execution::columnar::DataType::Int64,
                values.len(),
            );

            let column = result.get_column_mut(&col_name).unwrap();
            for (i, value) in values.into_iter().enumerate() {
                if let serde_json::Value::Number(num) = value {
                    if let Some(n) = num.as_i64() {
                        column.push(n).unwrap();
                    }
                }
            }
        }

        result.row_count = self.row_count;
        Ok(result)
    }
}

/// Statistics for join algorithm selection
#[derive(Debug, Clone)]
pub struct JoinStatistics {
    pub left_cardinality: usize,
    pub right_cardinality: usize,
    pub left_sorted: bool,
    pub right_sorted: bool,
    pub available_memory_mb: usize,
    pub join_key_selectivity: f64,
}

impl JoinStatistics {
    /// Create statistics from data sources
    pub fn from_data_sources(
        left_rows: usize,
        right_rows: usize,
        left_sorted: bool,
        right_sorted: bool,
    ) -> Self {
        Self {
            left_cardinality: left_rows,
            right_cardinality: right_rows,
            left_sorted,
            right_sorted,
            available_memory_mb: 512,  // Default 512MB
            join_key_selectivity: 0.1, // Default 10% selectivity
        }
    }

    /// Estimate memory usage for hash join
    pub fn estimate_hash_join_memory(&self) -> usize {
        // Rough estimate: hash table overhead + data
        let hash_table_overhead = self.right_cardinality * 16; // ~16 bytes per entry
        hash_table_overhead / (1024 * 1024) // Convert to MB
    }
}

/// Adaptive join selector
pub struct JoinSelector {
    statistics: JoinStatistics,
}

impl JoinSelector {
    /// Create a new join selector
    pub fn new(statistics: JoinStatistics) -> Self {
        Self { statistics }
    }

    /// Select the optimal join algorithm
    pub fn select_algorithm(&self) -> JoinAlgorithm {
        let stats = &self.statistics;

        // Merge join for sorted data (lowest cost)
        if stats.left_sorted && stats.right_sorted {
            return JoinAlgorithm::MergeJoin;
        }

        // Hash join for large datasets with sufficient memory
        let hash_memory_mb = stats.estimate_hash_join_memory();
        if stats.left_cardinality > 1000
            && stats.right_cardinality > 1000
            && hash_memory_mb < stats.available_memory_mb
        {
            return JoinAlgorithm::HashJoin {
                use_bloom_filter: stats.join_key_selectivity < 0.5,
            };
        }

        // Nested loop as fallback
        JoinAlgorithm::NestedLoop
    }

    /// Estimate cost of each algorithm
    pub fn estimate_costs(&self) -> HashMap<JoinAlgorithm, f64> {
        let mut costs = HashMap::new();
        let stats = &self.statistics;

        // Nested loop: O(n*m)
        let nested_cost = (stats.left_cardinality * stats.right_cardinality) as f64;
        costs.insert(JoinAlgorithm::NestedLoop, nested_cost);

        // Hash join: O(n+m) with build/probe phases
        let hash_cost = (stats.left_cardinality + stats.right_cardinality) as f64 * 1.5;
        costs.insert(
            JoinAlgorithm::HashJoin {
                use_bloom_filter: false,
            },
            hash_cost,
        );

        // Merge join: O(n+m) for sorted data
        let merge_cost = (stats.left_cardinality + stats.right_cardinality) as f64;
        costs.insert(
            JoinAlgorithm::MergeJoin,
            if stats.left_sorted && stats.right_sorted {
                merge_cost
            } else {
                merge_cost * 10.0 // Much more expensive if sorting needed
            },
        );

        costs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_join_selector_merge_join() {
        let stats = JoinStatistics::from_data_sources(1000, 1000, true, true);
        let selector = JoinSelector::new(stats);

        let algorithm = selector.select_algorithm();
        assert_eq!(algorithm, JoinAlgorithm::MergeJoin);
    }

    #[test]
    fn test_join_selector_hash_join() {
        let stats = JoinStatistics::from_data_sources(10000, 10000, false, false);
        let selector = JoinSelector::new(stats);

        let algorithm = selector.select_algorithm();
        match algorithm {
            JoinAlgorithm::HashJoin { .. } => {} // Expected
            _ => panic!("Expected HashJoin, got {:?}", algorithm),
        }
    }

    #[test]
    fn test_join_selector_nested_loop() {
        let stats = JoinStatistics::from_data_sources(10, 10, false, false);
        let selector = JoinSelector::new(stats);

        let algorithm = selector.select_algorithm();
        assert_eq!(algorithm, JoinAlgorithm::NestedLoop);
    }

    #[test]
    fn test_cost_estimation() {
        let stats = JoinStatistics::from_data_sources(1000, 1000, true, true);
        let selector = JoinSelector::new(stats);

        let costs = selector.estimate_costs();

        // Merge join should be cheapest for sorted data
        let merge_cost = costs[&JoinAlgorithm::MergeJoin];
        let hash_cost = costs[&JoinAlgorithm::HashJoin {
            use_bloom_filter: false,
        }];
        let nested_cost = costs[&JoinAlgorithm::NestedLoop];

        assert!(merge_cost < hash_cost);
        assert!(hash_cost < nested_cost);
    }

    #[test]
    fn test_join_result_to_columnar() {
        let mut join_result = JoinResult::new();

        join_result.add_left_column(
            "id".to_string(),
            vec![
                serde_json::Value::Number(1.into()),
                serde_json::Value::Number(2.into()),
            ],
        );

        join_result.add_right_column(
            "name".to_string(),
            vec![
                serde_json::Value::String("Alice".to_string()),
                serde_json::Value::String("Bob".to_string()),
            ],
        );

        join_result.row_count = 2;

        let columnar = join_result.to_columnar_result().unwrap();

        assert_eq!(columnar.row_count, 2);
        assert!(columnar.get_column("left_id").is_some());
        assert!(columnar.get_column("right_name").is_some());
    }
}

// Re-export main types
pub use adaptive::{AdaptiveJoinExecutor, AdaptiveJoinResult, JoinMetrics};
pub use hash_join::{BloomFilter, HashJoinProcessor, HashJoinStats, execute_hash_join};
pub use merge_join::{MergeJoinProcessor, execute_merge_join, is_sorted_by, sort_columnar_result};
