//! Hash Join Implementation with Bloom Filter Optimization
//!
//! This module provides high-performance hash join algorithms
//! optimized for columnar data processing.

use crate::error::{Error, Result};
use crate::execution::columnar::ColumnarResult;
use crate::execution::joins::{JoinAlgorithm, JoinResult};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Bloom filter for fast existence checks in hash joins
pub struct BloomFilter {
    bits: Vec<u8>,
    hash_functions: usize,
    size: usize,
}

impl BloomFilter {
    /// Create a new bloom filter with optimal parameters
    pub fn new(expected_items: usize, false_positive_rate: f64) -> Self {
        let size = Self::optimal_size(expected_items, false_positive_rate);
        let hash_functions = Self::optimal_hash_functions(expected_items, size);

        Self {
            bits: vec![0u8; (size + 7) / 8], // Round up to bytes
            hash_functions,
            size,
        }
    }

    /// Calculate optimal bloom filter size
    fn optimal_size(expected_items: usize, false_positive_rate: f64) -> usize {
        let ln2_squared = std::f64::consts::LN_2 * std::f64::consts::LN_2;
        ((-(expected_items as f64)) * false_positive_rate.ln() / ln2_squared).ceil() as usize
    }

    /// Calculate optimal number of hash functions
    fn optimal_hash_functions(expected_items: usize, size: usize) -> usize {
        ((size as f64 / expected_items as f64) * std::f64::consts::LN_2).round() as usize
    }

    /// Add an item to the bloom filter
    pub fn insert<H: Hash>(&mut self, item: H) {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        for i in 0..self.hash_functions {
            item.hash(&mut hasher);
            let hash = hasher.finish() as usize;

            // Combine with seed for different hash functions
            let combined_hash = hash.wrapping_add(i * 0x9e3779b9);
            let bit_index = combined_hash % self.size;

            let byte_index = bit_index / 8;
            let bit_offset = bit_index % 8;

            if byte_index < self.bits.len() {
                self.bits[byte_index] |= 1 << bit_offset;
            }
        }
    }

    /// Check if an item might be in the filter
    pub fn might_contain<H: Hash>(&self, item: H) -> bool {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        for i in 0..self.hash_functions {
            item.hash(&mut hasher);
            let hash = hasher.finish() as usize;

            let combined_hash = hash.wrapping_add(i * 0x9e3779b9);
            let bit_index = combined_hash % self.size;

            let byte_index = bit_index / 8;
            let bit_offset = bit_index % 8;

            if byte_index >= self.bits.len() || (self.bits[byte_index] & (1 << bit_offset)) == 0 {
                return false;
            }
        }

        true
    }

    /// Get false positive rate estimate
    pub fn false_positive_rate(&self) -> f64 {
        let fill_ratio = self
            .bits
            .iter()
            .map(|&b| b.count_ones() as f64)
            .sum::<f64>()
            / (self.bits.len() as f64 * 8.0);

        // Approximation: (1 - e^(-k*n/m))^k
        let k = self.hash_functions as f64;
        let n_over_m = fill_ratio;

        (1.0 - (-k * n_over_m).exp()).powf(k)
    }
}

/// Hash join processor with bloom filter optimization
pub struct HashJoinProcessor {
    hash_table: HashMap<u64, Vec<JoinRow>>,
    bloom_filter: Option<BloomFilter>,
    left_columns: Vec<String>,
    right_columns: Vec<String>,
    join_key_left: String,
    join_key_right: String,
    use_bloom_filter: bool,
}

#[derive(Clone, Debug)]
struct JoinRow {
    values: Vec<serde_json::Value>,
}

impl HashJoinProcessor {
    /// Create a new hash join processor
    pub fn new(
        join_key_left: String,
        join_key_right: String,
        left_columns: Vec<String>,
        right_columns: Vec<String>,
        use_bloom_filter: bool,
    ) -> Self {
        Self {
            hash_table: HashMap::new(),
            bloom_filter: if use_bloom_filter {
                Some(BloomFilter::new(10000, 0.01)) // 1% false positive rate
            } else {
                None
            },
            left_columns,
            right_columns,
            join_key_left,
            join_key_right,
            use_bloom_filter,
        }
    }

    /// Build phase: populate hash table with right side data
    pub fn build(&mut self, right_data: &ColumnarResult) -> Result<()> {
        let right_join_key_col = right_data.get_column(&self.join_key_right).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in right data",
                self.join_key_right
            ))
        })?;

        for row_idx in 0..right_data.row_count {
            let join_key = self.extract_join_key(right_join_key_col, row_idx)?;

            // Extract row data
            let mut row_values = Vec::new();
            for col_name in &self.right_columns {
                let column = right_data
                    .get_column(col_name)
                    .ok_or_else(|| Error::Storage(format!("Column '{}' not found", col_name)))?;

                let value = self.extract_value(column, row_idx)?;
                row_values.push(value);
            }

            let join_row = JoinRow { values: row_values };
            self.hash_table.entry(join_key).or_default().push(join_row);

            // Add to bloom filter if enabled
            if let Some(ref mut bf) = self.bloom_filter {
                bf.insert(join_key);
            }
        }

        Ok(())
    }

    /// Probe phase: iterate left side and find matches
    pub fn probe(&self, left_data: &ColumnarResult) -> Result<JoinResult> {
        let mut result = JoinResult::new();

        let left_join_key_col = left_data.get_column(&self.join_key_left).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in left data",
                self.join_key_left
            ))
        })?;

        // Initialize result columns
        for col_name in &self.left_columns {
            result.add_left_column(col_name.clone(), Vec::new());
        }
        for col_name in &self.right_columns {
            result.add_right_column(col_name.clone(), Vec::new());
        }

        for left_row_idx in 0..left_data.row_count {
            let join_key = self.extract_join_key(left_join_key_col, left_row_idx)?;

            // Bloom filter check (fast rejection)
            if let Some(ref bf) = self.bloom_filter {
                if !bf.might_contain(join_key) {
                    continue; // Definitely no match
                }
            }

            // Hash table lookup
            if let Some(right_rows) = self.hash_table.get(&join_key) {
                for right_row in right_rows {
                    result.row_count += 1;

                    // Add left side values
                    for (i, col_name) in self.left_columns.iter().enumerate() {
                        let column = left_data.get_column(col_name).ok_or_else(|| {
                            Error::Storage(format!("Column '{}' not found", col_name))
                        })?;

                        let value = self.extract_value(column, left_row_idx)?;
                        if let Some(left_col) = result.left_columns.get_mut(col_name) {
                            left_col.push(value);
                        }
                    }

                    // Add right side values
                    for (i, col_name) in self.right_columns.iter().enumerate() {
                        let value = right_row.values[i].clone();
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
    ) -> Result<u64> {
        let value = self.extract_value(column, row_idx)?;
        match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(i as u64)
                } else if let Some(f) = n.as_f64() {
                    Ok(f as u64) // Truncate for simplicity
                } else {
                    Err(Error::Storage(
                        "Unsupported number type for join key".to_string(),
                    ))
                }
            }
            _ => Err(Error::Storage("Join key must be numeric".to_string())),
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
                // For now, return placeholder
                Ok(serde_json::Value::String("".to_string()))
            }
        }
    }

    /// Get hash table statistics
    pub fn stats(&self) -> HashJoinStats {
        let mut max_chain_length = 0;
        let mut total_entries = 0;

        for bucket in self.hash_table.values() {
            max_chain_length = max_chain_length.max(bucket.len());
            total_entries += bucket.len();
        }

        HashJoinStats {
            buckets: self.hash_table.len(),
            total_entries,
            max_chain_length,
            load_factor: if self.hash_table.len() > 0 {
                total_entries as f64 / self.hash_table.len() as f64
            } else {
                0.0
            },
            bloom_filter_enabled: self.bloom_filter.is_some(),
            false_positive_rate: self
                .bloom_filter
                .as_ref()
                .map(|bf| bf.false_positive_rate())
                .unwrap_or(0.0),
        }
    }
}

/// Hash join statistics
#[derive(Debug, Clone)]
pub struct HashJoinStats {
    pub buckets: usize,
    pub total_entries: usize,
    pub max_chain_length: usize,
    pub load_factor: f64,
    pub bloom_filter_enabled: bool,
    pub false_positive_rate: f64,
}

/// Execute hash join between two columnar results
pub fn execute_hash_join(
    left: &ColumnarResult,
    right: &ColumnarResult,
    join_key_left: &str,
    join_key_right: &str,
    left_columns: &[String],
    right_columns: &[String],
    algorithm: &JoinAlgorithm,
) -> Result<JoinResult> {
    let use_bloom_filter = matches!(
        algorithm,
        JoinAlgorithm::HashJoin {
            use_bloom_filter: true
        }
    );

    let mut processor = HashJoinProcessor::new(
        join_key_left.to_string(),
        join_key_right.to_string(),
        left_columns.to_vec(),
        right_columns.to_vec(),
        use_bloom_filter,
    );

    // Build phase (right side)
    processor.build(right)?;

    // Probe phase (left side)
    processor.probe(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::columnar::{ColumnarResult, DataType};

    #[test]
    fn test_bloom_filter() {
        let mut bf = BloomFilter::new(1000, 0.01);

        // Insert some values
        bf.insert(42u64);
        bf.insert(100u64);
        bf.insert(200u64);

        // Check containment
        assert!(bf.might_contain(42u64));
        assert!(bf.might_contain(100u64));
        assert!(bf.might_contain(200u64));
        assert!(!bf.might_contain(999u64)); // Definitely not present

        // False positive rate should be reasonable
        let fp_rate = bf.false_positive_rate();
        assert!(fp_rate < 0.1); // Less than 10%
    }

    #[test]
    fn test_hash_join_simple() {
        // Create left data
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 3);
        left.add_column("name".to_string(), DataType::Int64, 3);

        {
            let id_col = left.get_column_mut("id").unwrap();
            id_col.push(1i64).unwrap();
            id_col.push(2i64).unwrap();
            id_col.push(3i64).unwrap();
        }

        {
            let name_col = left.get_column_mut("name").unwrap();
            name_col.push(10i64).unwrap();
            name_col.push(20i64).unwrap();
            name_col.push(30i64).unwrap();
        }

        left.row_count = 3;

        // Create right data
        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);
        right.add_column("score".to_string(), DataType::Int64, 2);

        {
            let id_col = right.get_column_mut("id").unwrap();
            id_col.push(1i64).unwrap();
            id_col.push(2i64).unwrap();
        }

        {
            let score_col = right.get_column_mut("score").unwrap();
            score_col.push(100i64).unwrap();
            score_col.push(200i64).unwrap();
        }

        right.row_count = 2;

        // Execute hash join
        let result = execute_hash_join(
            &left,
            &right,
            "id",
            "id",
            &["id".to_string(), "name".to_string()],
            &["score".to_string()],
            &JoinAlgorithm::HashJoin {
                use_bloom_filter: false,
            },
        )
        .unwrap();

        // Should have 2 joined rows (ids 1 and 2 match)
        assert_eq!(result.row_count, 2);
        assert_eq!(result.left_columns["id"].len(), 2);
        assert_eq!(result.right_columns["score"].len(), 2);
    }

    #[test]
    fn test_hash_join_with_bloom_filter() {
        // Similar test but with bloom filter enabled
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
        id_col.push(3i64).unwrap(); // No match for id=2
        right.row_count = 2;

        let result = execute_hash_join(
            &left,
            &right,
            "id",
            "id",
            &["id".to_string()],
            &["id".to_string()],
            &JoinAlgorithm::HashJoin {
                use_bloom_filter: true,
            },
        )
        .unwrap();

        // Should have 1 joined row (only id=1 matches)
        assert_eq!(result.row_count, 1);
    }

    #[test]
    fn test_hash_join_stats() {
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(1i64).unwrap(); // Duplicate key
        id_col.push(2i64).unwrap();
        left.row_count = 3;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 1);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        right.row_count = 1;

        let mut processor = HashJoinProcessor::new(
            "id".to_string(),
            "id".to_string(),
            vec!["id".to_string()],
            vec!["id".to_string()],
            false,
        );

        processor.build(&right).unwrap();
        let _result = processor.probe(&left).unwrap();

        let stats = processor.stats();
        assert_eq!(stats.buckets, 1); // One unique key
        assert_eq!(stats.total_entries, 1);
        assert_eq!(stats.max_chain_length, 1);
        assert!(!stats.bloom_filter_enabled);
    }
}
