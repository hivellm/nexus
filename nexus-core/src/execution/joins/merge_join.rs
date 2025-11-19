//! Merge Join Implementation for Sorted Data
//!
//! This module provides efficient merge join algorithms for
//! pre-sorted columnar data sources.

use crate::error::{Error, Result};
use crate::execution::columnar::ColumnarResult;
use crate::execution::joins::JoinResult;

/// Merge join processor for sorted data
pub struct MergeJoinProcessor {
    left_columns: Vec<String>,
    right_columns: Vec<String>,
    join_key_left: String,
    join_key_right: String,
}

impl MergeJoinProcessor {
    /// Create a new merge join processor
    pub fn new(
        join_key_left: String,
        join_key_right: String,
        left_columns: Vec<String>,
        right_columns: Vec<String>,
    ) -> Self {
        Self {
            left_columns,
            right_columns,
            join_key_left,
            join_key_right,
        }
    }

    /// Execute merge join on sorted data
    pub fn execute(&self, left: &ColumnarResult, right: &ColumnarResult) -> Result<JoinResult> {
        let mut result = JoinResult::new();

        // Initialize result columns
        for col_name in &self.left_columns {
            result.add_left_column(col_name.clone(), Vec::new());
        }
        for col_name in &self.right_columns {
            result.add_right_column(col_name.clone(), Vec::new());
        }

        // Get join key columns
        let left_join_col = left.get_column(&self.join_key_left).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in left data",
                self.join_key_left
            ))
        })?;

        let right_join_col = right.get_column(&self.join_key_right).ok_or_else(|| {
            Error::Storage(format!(
                "Join key column '{}' not found in right data",
                self.join_key_right
            ))
        })?;

        // Merge join algorithm
        let mut left_idx = 0;
        let mut right_idx = 0;

        while left_idx < left.row_count && right_idx < right.row_count {
            let left_key = self.extract_join_key(left_join_col, left_idx)?;
            let right_key = self.extract_join_key(right_join_col, right_idx)?;

            if left_key == right_key {
                // Keys match - find all matching rows
                let left_start = left_idx;
                while left_idx < left.row_count {
                    let curr_key = self.extract_join_key(left_join_col, left_idx)?;
                    if curr_key != left_key {
                        break;
                    }
                    left_idx += 1;
                }

                let right_start = right_idx;
                while right_idx < right.row_count {
                    let curr_key = self.extract_join_key(right_join_col, right_idx)?;
                    if curr_key != right_key {
                        break;
                    }
                    right_idx += 1;
                }

                // Cross product of matching groups
                for l_idx in left_start..left_idx {
                    for r_idx in right_start..right_idx {
                        self.add_joined_row(&mut result, left, right, l_idx, r_idx)?;
                    }
                }
            } else if left_key < right_key {
                // Advance left pointer
                left_idx += 1;
            } else {
                // Advance right pointer
                right_idx += 1;
            }
        }

        Ok(result)
    }

    /// Add a joined row to the result
    fn add_joined_row(
        &self,
        result: &mut JoinResult,
        left: &ColumnarResult,
        right: &ColumnarResult,
        left_idx: usize,
        right_idx: usize,
    ) -> Result<()> {
        result.row_count += 1;

        // Add left side values
        for col_name in &self.left_columns {
            let column = left
                .get_column(col_name)
                .ok_or_else(|| Error::Storage(format!("Column '{}' not found", col_name)))?;

            let value = self.extract_value(column, left_idx)?;
            if let Some(left_col) = result.left_columns.get_mut(col_name) {
                left_col.push(value);
            }
        }

        // Add right side values
        for col_name in &self.right_columns {
            let column = right
                .get_column(col_name)
                .ok_or_else(|| Error::Storage(format!("Column '{}' not found", col_name)))?;

            let value = self.extract_value(column, right_idx)?;
            if let Some(right_col) = result.right_columns.get_mut(col_name) {
                right_col.push(value);
            }
        }

        Ok(())
    }

    /// Extract join key from column at given row
    fn extract_join_key(
        &self,
        column: &crate::execution::columnar::Column,
        row_idx: usize,
    ) -> Result<i64> {
        match column.data_type {
            crate::execution::columnar::DataType::Int64 => column.get::<i64>(row_idx),
            _ => Err(Error::Storage(
                "Merge join requires integer join keys".to_string(),
            )),
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
}

/// Execute merge join between two sorted columnar results
pub fn execute_merge_join(
    left: &ColumnarResult,
    right: &ColumnarResult,
    join_key_left: &str,
    join_key_right: &str,
    left_columns: &[String],
    right_columns: &[String],
) -> Result<JoinResult> {
    let processor = MergeJoinProcessor::new(
        join_key_left.to_string(),
        join_key_right.to_string(),
        left_columns.to_vec(),
        right_columns.to_vec(),
    );

    processor.execute(left, right)
}

/// Sort a columnar result by a key column (in-place sorting simulation)
pub fn sort_columnar_result(data: &mut ColumnarResult, sort_key: &str) -> Result<()> {
    if data.row_count == 0 {
        return Ok(());
    }

    let sort_col = data
        .get_column(sort_key)
        .ok_or_else(|| Error::Storage(format!("Sort key column '{}' not found", sort_key)))?;

    // Extract sort keys
    let mut indices: Vec<usize> = (0..data.row_count).collect();
    let mut sort_keys = Vec::with_capacity(data.row_count);

    for i in 0..data.row_count {
        match sort_col.data_type {
            crate::execution::columnar::DataType::Int64 => {
                sort_keys.push(sort_col.get::<i64>(i)? as i128);
            }
            _ => {
                return Err(Error::Storage(
                    "Sorting only supported for integer columns".to_string(),
                ));
            }
        }
    }

    // Sort indices by sort keys
    indices.sort_by_key(|&i| sort_keys[i]);

    // This is a simplified version - in a real implementation,
    // we would need to reorder all columns according to the sorted indices
    // For now, this is just a demonstration

    Ok(())
}

/// Check if a columnar result is sorted by a key column
pub fn is_sorted_by(data: &ColumnarResult, sort_key: &str) -> Result<bool> {
    let sort_col = data
        .get_column(sort_key)
        .ok_or_else(|| Error::Storage(format!("Sort key column '{}' not found", sort_key)))?;

    for i in 1..data.row_count {
        match sort_col.data_type {
            crate::execution::columnar::DataType::Int64 => {
                let prev = sort_col.get::<i64>(i - 1)?;
                let curr = sort_col.get::<i64>(i)?;
                if prev > curr {
                    return Ok(false);
                }
            }
            _ => {
                return Err(Error::Storage(
                    "Sort checking only supported for integer columns".to_string(),
                ));
            }
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::columnar::{ColumnarResult, DataType};

    #[test]
    fn test_merge_join_simple() {
        // Create sorted left data
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 3);
        left.add_column("value".to_string(), DataType::Int64, 3);

        {
            let id_col = left.get_column_mut("id").unwrap();
            id_col.push(1i64).unwrap();
            id_col.push(2i64).unwrap();
            id_col.push(3i64).unwrap();
        }

        {
            let value_col = left.get_column_mut("value").unwrap();
            value_col.push(10i64).unwrap();
            value_col.push(20i64).unwrap();
            value_col.push(30i64).unwrap();
        }

        left.row_count = 3;

        // Create sorted right data
        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 3);
        right.add_column("score".to_string(), DataType::Int64, 3);

        {
            let id_col = right.get_column_mut("id").unwrap();
            id_col.push(1i64).unwrap();
            id_col.push(2i64).unwrap();
            id_col.push(4i64).unwrap();
        }

        {
            let score_col = right.get_column_mut("score").unwrap();
            score_col.push(100i64).unwrap();
            score_col.push(200i64).unwrap();
            score_col.push(400i64).unwrap();
        }

        right.row_count = 3;

        // Execute merge join
        let result = execute_merge_join(
            &left,
            &right,
            "id",
            "id",
            &["id".to_string(), "value".to_string()],
            &["score".to_string()],
        )
        .unwrap();

        // Should have 2 joined rows (ids 1 and 2 match)
        assert_eq!(result.row_count, 2);
        assert_eq!(result.left_columns["id"].len(), 2);
        assert_eq!(result.right_columns["score"].len(), 2);
    }

    #[test]
    fn test_merge_join_duplicates() {
        // Create left data with duplicates
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 4);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(1i64).unwrap(); // Duplicate
        id_col.push(2i64).unwrap();
        id_col.push(3i64).unwrap();
        left.row_count = 4;

        // Create right data with duplicates
        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        id_col.push(2i64).unwrap(); // Duplicate
        right.row_count = 3;

        let result = execute_merge_join(
            &left,
            &right,
            "id",
            "id",
            &["id".to_string()],
            &["id".to_string()],
        )
        .unwrap();

        // Should have 4 joined rows: 2 (left) x 1 (right) + 1 (left) x 2 (right)
        assert_eq!(result.row_count, 4);
    }

    #[test]
    fn test_is_sorted_by() {
        let mut data = ColumnarResult::new();
        data.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = data.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        id_col.push(3i64).unwrap();
        data.row_count = 3;

        assert!(is_sorted_by(&data, "id").unwrap());
    }

    #[test]
    fn test_is_not_sorted_by() {
        let mut data = ColumnarResult::new();
        data.add_column("id".to_string(), DataType::Int64, 3);

        let id_col = data.get_column_mut("id").unwrap();
        id_col.push(3i64).unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        data.row_count = 3;

        assert!(!is_sorted_by(&data, "id").unwrap());
    }

    #[test]
    fn test_merge_join_empty() {
        let left = ColumnarResult::new();
        let right = ColumnarResult::new();

        let result = execute_merge_join(&left, &right, "id", "id", &[], &[]).unwrap();

        assert_eq!(result.row_count, 0);
    }

    #[test]
    fn test_merge_join_no_matches() {
        // Create data with no matching keys
        let mut left = ColumnarResult::new();
        left.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = left.get_column_mut("id").unwrap();
        id_col.push(1i64).unwrap();
        id_col.push(2i64).unwrap();
        left.row_count = 2;

        let mut right = ColumnarResult::new();
        right.add_column("id".to_string(), DataType::Int64, 2);

        let id_col = right.get_column_mut("id").unwrap();
        id_col.push(3i64).unwrap();
        id_col.push(4i64).unwrap();
        right.row_count = 2;

        let result = execute_merge_join(
            &left,
            &right,
            "id",
            "id",
            &["id".to_string()],
            &["id".to_string()],
        )
        .unwrap();

        assert_eq!(result.row_count, 0);
    }
}
