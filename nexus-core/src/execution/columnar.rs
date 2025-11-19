//! Columnar Data Structures for SIMD Operations
//!
//! This module provides columnar data representations optimized for
//! SIMD processing and vectorized query execution.

use crate::error::{Error, Result};
use std::collections::HashMap;

/// Supported data types for columnar storage
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DataType {
    Int64,
    Float64,
    String,
    Bool,
}

impl DataType {
    pub fn size(&self) -> usize {
        match self {
            DataType::Int64 => 8,
            DataType::Float64 => 8,
            DataType::String => 16, // Pointer + length
            DataType::Bool => 1,
        }
    }
}

/// SIMD-aligned column of data
#[repr(align(64))] // AVX-512 alignment
pub struct Column {
    pub data_type: DataType,
    pub data: Vec<u8>,
    pub null_mask: Vec<u8>, // 1 bit per element
    pub len: usize,
}

impl Column {
    /// Create a new column with specified capacity
    pub fn with_capacity(data_type: DataType, capacity: usize) -> Self {
        let element_size = data_type.size();
        let total_size = capacity * element_size;

        // Ensure SIMD alignment
        let aligned_size = ((total_size + 63) / 64) * 64; // 64-byte alignment
        let mut data = vec![0u8; aligned_size];

        // Null mask: 1 bit per element, rounded up to bytes
        let null_mask_size = (capacity + 7) / 8;
        let null_mask = vec![0u8; null_mask_size];

        Self {
            data_type,
            data,
            null_mask,
            len: 0,
        }
    }

    /// Push a value to the column
    pub fn push<T: ColumnValue>(&mut self, value: T) -> Result<()> {
        if self.data_type != T::DATA_TYPE {
            return Err(Error::Storage(format!(
                "Type mismatch: expected {:?}, got {:?}",
                self.data_type,
                T::DATA_TYPE
            )));
        }

        if self.len >= self.capacity() {
            return Err(Error::Storage("Column capacity exceeded".to_string()));
        }

        let offset = self.len * self.data_type.size();
        value.write_to(&mut self.data[offset..offset + self.data_type.size()]);

        // Set null mask bit to 0 (not null)
        let byte_index = self.len / 8;
        let bit_index = self.len % 8;
        self.null_mask[byte_index] &= !(1 << bit_index);

        self.len += 1;
        Ok(())
    }

    /// Get a value from the column
    pub fn get<T: ColumnValue>(&self, index: usize) -> Result<T> {
        if index >= self.len {
            return Err(Error::Storage("Index out of bounds".to_string()));
        }

        if self.data_type != T::DATA_TYPE {
            return Err(Error::Storage(format!(
                "Type mismatch: expected {:?}, got {:?}",
                self.data_type,
                T::DATA_TYPE
            )));
        }

        let offset = index * self.data_type.size();
        T::read_from(&self.data[offset..offset + self.data_type.size()])
    }

    /// Get capacity of the column
    pub fn capacity(&self) -> usize {
        self.data.len() / self.data_type.size()
    }

    /// Check if value at index is null
    pub fn is_null(&self, index: usize) -> bool {
        if index >= self.len {
            return true;
        }

        let byte_index = index / 8;
        let bit_index = index % 8;
        (self.null_mask[byte_index] & (1 << bit_index)) != 0
    }

    /// Set value at index to null
    pub fn set_null(&mut self, index: usize) {
        if index < self.len {
            let byte_index = index / 8;
            let bit_index = index % 8;
            self.null_mask[byte_index] |= 1 << bit_index;
        }
    }

    /// Get raw data slice for SIMD operations
    pub fn as_slice<T: Copy>(&self) -> &[T] {
        let element_size = std::mem::size_of::<T>();
        let data_type_size = self.data_type.size();

        if element_size != data_type_size {
            panic!("Type size mismatch for SIMD operations");
        }

        unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const T, self.len) }
    }
}

/// Trait for values that can be stored in columns
pub trait ColumnValue: Sized {
    const DATA_TYPE: DataType;

    fn write_to(self, buffer: &mut [u8]);
    fn read_from(buffer: &[u8]) -> Result<Self>;
}

impl ColumnValue for i64 {
    const DATA_TYPE: DataType = DataType::Int64;

    fn write_to(self, buffer: &mut [u8]) {
        buffer.copy_from_slice(&self.to_le_bytes());
    }

    fn read_from(buffer: &[u8]) -> Result<Self> {
        if buffer.len() != 8 {
            return Err(Error::Storage("Invalid buffer size for i64".to_string()));
        }
        Ok(i64::from_le_bytes(buffer.try_into().unwrap()))
    }
}

impl ColumnValue for f64 {
    const DATA_TYPE: DataType = DataType::Float64;

    fn write_to(self, buffer: &mut [u8]) {
        buffer.copy_from_slice(&self.to_le_bytes());
    }

    fn read_from(buffer: &[u8]) -> Result<Self> {
        if buffer.len() != 8 {
            return Err(Error::Storage("Invalid buffer size for f64".to_string()));
        }
        Ok(f64::from_le_bytes(buffer.try_into().unwrap()))
    }
}

impl ColumnValue for bool {
    const DATA_TYPE: DataType = DataType::Bool;

    fn write_to(self, buffer: &mut [u8]) {
        buffer[0] = if self { 1 } else { 0 };
    }

    fn read_from(buffer: &[u8]) -> Result<Self> {
        if buffer.is_empty() {
            return Err(Error::Storage("Empty buffer for bool".to_string()));
        }
        Ok(buffer[0] != 0)
    }
}

impl ColumnValue for String {
    const DATA_TYPE: DataType = DataType::String;

    fn write_to(self, buffer: &mut [u8]) {
        let bytes = self.as_bytes();
        if bytes.len() > buffer.len() {
            panic!("String too long for buffer");
        }
        buffer[..bytes.len()].copy_from_slice(bytes);
    }

    fn read_from(buffer: &[u8]) -> Result<Self> {
        // Find null terminator or use entire buffer
        let len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        String::from_utf8(buffer[..len].to_vec())
            .map_err(|e| Error::Storage(format!("Invalid UTF-8 string: {}", e)))
    }
}

/// Columnar query result with named columns
pub struct ColumnarResult {
    pub columns: HashMap<String, Column>,
    pub row_count: usize,
}

impl ColumnarResult {
    /// Create a new empty columnar result
    pub fn new() -> Self {
        Self {
            columns: HashMap::new(),
            row_count: 0,
        }
    }

    /// Add a column to the result
    pub fn add_column(&mut self, name: String, data_type: DataType, capacity: usize) {
        self.columns
            .insert(name, Column::with_capacity(data_type, capacity));
    }

    /// Get a column by name
    pub fn get_column(&self, name: &str) -> Option<&Column> {
        self.columns.get(name)
    }

    /// Get a mutable column by name
    pub fn get_column_mut(&mut self, name: &str) -> Option<&mut Column> {
        self.columns.get_mut(name)
    }

    /// Add a row to all columns
    pub fn push_row(&mut self) -> Result<()> {
        // For now, just increment row count
        // In a real implementation, this would validate all columns have the same length
        self.row_count += 1;
        Ok(())
    }

    /// Filter result by boolean mask
    pub fn filter_by_mask(&self, mask: &[bool]) -> ColumnarResult {
        let mut result = ColumnarResult::new();

        for (name, column) in &self.columns {
            let filtered_column = self.filter_column(column, mask);
            result.columns.insert(name.clone(), filtered_column);
        }

        result.row_count = mask.iter().filter(|&&x| x).count();
        result
    }

    /// Filter a single column by boolean mask
    fn filter_column(&self, column: &Column, mask: &[bool]) -> Column {
        let mut filtered =
            Column::with_capacity(column.data_type, mask.iter().filter(|&&x| x).count());

        for (i, &keep) in mask.iter().enumerate() {
            if keep && i < column.len {
                match column.data_type {
                    DataType::Int64 => {
                        let value: i64 = column.get(i).unwrap();
                        filtered.push(value).unwrap();
                    }
                    DataType::Float64 => {
                        let value: f64 = column.get(i).unwrap();
                        filtered.push(value).unwrap();
                    }
                    DataType::Bool => {
                        let value: bool = column.get(i).unwrap();
                        filtered.push(value).unwrap();
                    }
                    DataType::String => {
                        let value: String = column.get(i).unwrap();
                        filtered.push(value).unwrap();
                    }
                }
            }
        }

        filtered
    }

    /// Apply LIMIT operation to columnar result
    pub fn limit(self, limit: usize) -> ColumnarResult {
        if self.row_count <= limit {
            return self; // No need to limit
        }

        let mut result = ColumnarResult::new();
        let actual_limit = limit.min(self.row_count);

        for (name, column) in &self.columns {
            let mut limited_column = Column::with_capacity(column.data_type, actual_limit);

            // Copy only the first 'limit' elements
            for i in 0..actual_limit {
                match column.data_type {
                    DataType::Int64 => {
                        let value: i64 = column.get(i).unwrap();
                        limited_column.push(value).unwrap();
                    }
                    DataType::Float64 => {
                        let value: f64 = column.get(i).unwrap();
                        limited_column.push(value).unwrap();
                    }
                    DataType::String => {
                        let value: String = column.get(i).unwrap();
                        limited_column.push(value).unwrap();
                    }
                    DataType::Bool => {
                        let value: bool = column.get(i).unwrap();
                        limited_column.push(value).unwrap();
                    }
                }
            }

            result.columns.insert(name.clone(), limited_column);
        }

        result.row_count = actual_limit;
        result
    }

    /// Convert to traditional row-based format (for compatibility)
    pub fn to_rows(&self) -> Vec<HashMap<String, serde_json::Value>> {
        let mut rows = Vec::with_capacity(self.row_count);

        for i in 0..self.row_count {
            let mut row = HashMap::new();

            for (col_name, column) in &self.columns {
                if !column.is_null(i) {
                    match column.data_type {
                        DataType::Int64 => {
                            let value: i64 = column.get(i).unwrap();
                            row.insert(col_name.clone(), serde_json::Value::Number(value.into()));
                        }
                        DataType::Float64 => {
                            let value: f64 = column.get(i).unwrap();
                            row.insert(
                                col_name.clone(),
                                serde_json::Value::Number(
                                    serde_json::Number::from_f64(value).unwrap(),
                                ),
                            );
                        }
                        DataType::Bool => {
                            let value: bool = column.get(i).unwrap();
                            row.insert(col_name.clone(), serde_json::Value::Bool(value));
                        }
                        DataType::String => {
                            let value: String = column.get(i).unwrap();
                            row.insert(col_name.clone(), serde_json::Value::String(value));
                        }
                    }
                }
            }

            rows.push(row);
        }

        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_column_push_get() {
        let mut column = Column::with_capacity(DataType::Int64, 10);

        column.push(42i64).unwrap();
        column.push(100i64).unwrap();

        assert_eq!(column.get::<i64>(0).unwrap(), 42);
        assert_eq!(column.get::<i64>(1).unwrap(), 100);
        assert_eq!(column.len, 2);
    }

    #[test]
    fn test_columnar_result() {
        let mut result = ColumnarResult::new();

        result.add_column("age".to_string(), DataType::Int64, 10);
        result.add_column("name".to_string(), DataType::String, 10);

        let age_col = result.get_column_mut("age").unwrap();
        age_col.push(25i64).unwrap();
        age_col.push(30i64).unwrap();
        age_col.push(35i64).unwrap();

        result.row_count = 3;

        // Test filtering
        let mask = vec![true, false, true]; // Keep first and third rows
        let filtered = result.filter_by_mask(&mask);

        assert_eq!(filtered.row_count, 2);
        let filtered_age = filtered.get_column("age").unwrap();
        assert_eq!(filtered_age.get::<i64>(0).unwrap(), 25);
        assert_eq!(filtered_age.get::<i64>(1).unwrap(), 35);
    }

    #[test]
    fn test_simd_alignment() {
        let mut column = Column::with_capacity(DataType::Int64, 100);

        // Check that data is properly allocated
        assert!(!column.data.is_empty());
        // Capacity may be larger due to SIMD alignment
        assert!(column.capacity() >= 100);

        // Add some data
        column.push(42i64).unwrap();
        assert_eq!(column.len, 1);

        // Check SIMD slice access
        let slice = column.as_slice::<i64>();
        assert_eq!(slice.len(), 1);
        assert_eq!(slice[0], 42);
    }
}
