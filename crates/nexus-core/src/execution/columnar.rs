//! Columnar Data Structures for SIMD Operations
//!
//! This module provides columnar data representations optimized for
//! SIMD processing and vectorized query execution.
//!
//! Compare kernels route through the canonical, proptest-covered
//! implementations in [`crate::simd::compare`] — AVX-512 → AVX2 →
//! scalar dispatch based on runtime CPU features, with NaN-aware
//! IEEE-ordered semantics for floats. The tagged `ComparisonOp` here
//! maps onto the six per-type entry points in the kernel module; no
//! bespoke intrinsics live in this file.

use crate::error::{Error, Result};
use crate::simd::compare as cmp;
use std::collections::HashMap;

/// SIMD-accelerated compare + filter helpers for [`Column`].
///
/// Thin wrappers that dispatch to [`crate::simd::compare`] and present
/// a `Vec<bool>` API to the existing columnar consumers (joins, JIT
/// codegen). Internally the kernel returns a packed `Vec<u64>`
/// bitmap; [`bitmap_to_bool_vec`] expands it once on the way out.
mod simd_ops {
    use super::*;

    /// SIMD-accelerated comparison operations for numeric columns.
    pub struct SimdComparator;

    impl SimdComparator {
        pub fn new() -> Self {
            Self
        }

        /// Compare an i64 column against a scalar using the dispatched
        /// canonical kernel. Returns a boolean result per row.
        pub fn compare_scalar_i64(
            &self,
            column: &Column,
            scalar: i64,
            op: ComparisonOp,
        ) -> Vec<bool> {
            let values = column.as_slice::<i64>();
            let bitmap = match op {
                ComparisonOp::Equal => cmp::eq_i64(values, scalar),
                ComparisonOp::NotEqual => cmp::ne_i64(values, scalar),
                ComparisonOp::Greater => cmp::gt_i64(values, scalar),
                ComparisonOp::GreaterEqual => cmp::ge_i64(values, scalar),
                ComparisonOp::Less => cmp::lt_i64(values, scalar),
                ComparisonOp::LessEqual => cmp::le_i64(values, scalar),
            };
            bitmap_to_bool_vec(&bitmap, values.len())
        }

        /// Compare an f64 column against a scalar — IEEE-ordered, so
        /// any NaN operand yields `false` for eq/lt/le/gt/ge and
        /// `true` for ne, matching the scalar kernel reference.
        pub fn compare_scalar_f64(
            &self,
            column: &Column,
            scalar: f64,
            op: ComparisonOp,
        ) -> Vec<bool> {
            let values = column.as_slice::<f64>();
            let bitmap = match op {
                ComparisonOp::Equal => cmp::eq_f64(values, scalar),
                ComparisonOp::NotEqual => cmp::ne_f64(values, scalar),
                ComparisonOp::Greater => cmp::gt_f64(values, scalar),
                ComparisonOp::GreaterEqual => cmp::ge_f64(values, scalar),
                ComparisonOp::Less => cmp::lt_f64(values, scalar),
                ComparisonOp::LessEqual => cmp::le_f64(values, scalar),
            };
            bitmap_to_bool_vec(&bitmap, values.len())
        }
    }

    /// SIMD-accelerated filter operations.
    pub struct SimdFilter {
        comparator: SimdComparator,
    }

    impl SimdFilter {
        pub fn new() -> Self {
            Self {
                comparator: SimdComparator::new(),
            }
        }

        /// Apply a WHERE filter against a single column. And/Or
        /// operands recurse through the same column; multi-column
        /// predicates land with the broader filter-operator wiring
        /// in a later slice of `phase3_executor-columnar-wiring`.
        pub fn apply_where_filter(&self, column: &Column, condition: &WhereCondition) -> Vec<bool> {
            match condition {
                WhereCondition::Comparison {
                    column: _,
                    op,
                    value,
                } => match column.data_type {
                    DataType::Int64 => {
                        if let serde_json::Value::Number(num) = value {
                            if let Some(int_val) = num.as_i64() {
                                return self.comparator.compare_scalar_i64(column, int_val, *op);
                            }
                        }
                        vec![true; column.len]
                    }
                    DataType::Float64 => {
                        if let serde_json::Value::Number(num) = value {
                            if let Some(float_val) = num.as_f64() {
                                return self.comparator.compare_scalar_f64(column, float_val, *op);
                            }
                        }
                        vec![true; column.len]
                    }
                    _ => vec![true; column.len],
                },
                WhereCondition::And(conditions) => {
                    let mut result = vec![true; column.len];
                    for cond in conditions {
                        let partial = self.apply_where_filter(column, cond);
                        for i in 0..result.len() {
                            result[i] = result[i] && partial[i];
                        }
                    }
                    result
                }
                WhereCondition::Or(conditions) => {
                    let mut result = vec![false; column.len];
                    for cond in conditions {
                        let partial = self.apply_where_filter(column, cond);
                        for i in 0..result.len() {
                            result[i] = result[i] || partial[i];
                        }
                    }
                    result
                }
            }
        }
    }

    /// Expand a packed bitmap (`Vec<u64>` with LSB-first bit ordering
    /// within each word — the shape [`crate::simd::compare`]
    /// kernels emit) into a `Vec<bool>` of exactly `len` entries.
    fn bitmap_to_bool_vec(bitmap: &[u64], len: usize) -> Vec<bool> {
        let mut out = Vec::with_capacity(len);
        for i in 0..len {
            let word = i >> 6; // i / 64
            let bit = i & 0x3F; // i % 64
            let set = bitmap
                .get(word)
                .map(|w| ((w >> bit) & 1) != 0)
                .unwrap_or(false);
            out.push(set);
        }
        out
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn compare_scalar_i64_matches_scalar_across_ops() {
            let values: Vec<i64> = (0..257).collect();
            let mut column = Column::with_capacity(DataType::Int64, values.len());
            for v in &values {
                column.push::<i64>(*v).unwrap();
            }
            let comp = SimdComparator::new();
            for &(op, scalar) in &[
                (ComparisonOp::Equal, 42_i64),
                (ComparisonOp::NotEqual, 42),
                (ComparisonOp::Less, 100),
                (ComparisonOp::LessEqual, 100),
                (ComparisonOp::Greater, 100),
                (ComparisonOp::GreaterEqual, 100),
            ] {
                let got = comp.compare_scalar_i64(&column, scalar, op);
                let expected: Vec<bool> = values
                    .iter()
                    .map(|&v| match op {
                        ComparisonOp::Equal => v == scalar,
                        ComparisonOp::NotEqual => v != scalar,
                        ComparisonOp::Less => v < scalar,
                        ComparisonOp::LessEqual => v <= scalar,
                        ComparisonOp::Greater => v > scalar,
                        ComparisonOp::GreaterEqual => v >= scalar,
                    })
                    .collect();
                assert_eq!(got, expected, "op={:?} scalar={}", op, scalar);
            }
        }

        #[test]
        fn compare_scalar_f64_handles_nan() {
            let values = vec![1.0, 2.0, f64::NAN, 3.5, 4.0];
            let mut column = Column::with_capacity(DataType::Float64, values.len());
            for v in &values {
                column.push::<f64>(*v).unwrap();
            }
            let comp = SimdComparator::new();
            // eq with NaN: every compare with NaN collapses to false.
            let eq = comp.compare_scalar_f64(&column, f64::NAN, ComparisonOp::Equal);
            assert_eq!(eq, vec![false, false, false, false, false]);
            // lt 3.0 should skip the NaN row.
            let lt = comp.compare_scalar_f64(&column, 3.0, ComparisonOp::Less);
            assert_eq!(lt, vec![true, true, false, false, false]);
        }

        #[test]
        fn apply_where_filter_f64_uses_dispatched_kernel() {
            let values = vec![1.0, 5.0, 10.0, 15.0, 20.0];
            let mut column = Column::with_capacity(DataType::Float64, values.len());
            for v in &values {
                column.push::<f64>(*v).unwrap();
            }
            let filter = SimdFilter::new();
            let cond = WhereCondition::Comparison {
                column: "x".into(),
                op: ComparisonOp::GreaterEqual,
                value: serde_json::json!(10.0),
            };
            let got = filter.apply_where_filter(&column, &cond);
            assert_eq!(got, vec![false, false, true, true, true]);
        }

        #[test]
        fn bitmap_to_bool_vec_round_trip() {
            // Craft a bitmap: bits 0, 2, 63, 64, 65 set.
            let mut bitmap = vec![0u64; 2];
            bitmap[0] |= 1 << 0;
            bitmap[0] |= 1 << 2;
            bitmap[0] |= 1 << 63;
            bitmap[1] |= 1 << 0;
            bitmap[1] |= 1 << 1;
            let out = bitmap_to_bool_vec(&bitmap, 66);
            assert_eq!(out.len(), 66);
            assert!(out[0]);
            assert!(!out[1]);
            assert!(out[2]);
            assert!(out[63]);
            assert!(out[64]);
            assert!(out[65]);
        }
    }
}

/// Comparison operations for SIMD processing
#[derive(Debug, Clone, Copy)]
pub enum ComparisonOp {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

/// WHERE condition representation for SIMD processing
#[derive(Debug, Clone)]
pub enum WhereCondition {
    Comparison {
        column: String,
        op: ComparisonOp,
        value: serde_json::Value,
    },
    And(Vec<WhereCondition>),
    Or(Vec<WhereCondition>),
}

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

    /// Typed i64 slice — `None` when `data_type != Int64`.
    ///
    /// Safer alternative to the generic `as_slice::<i64>()` for
    /// callers that need a compile-time-checked numeric path; the
    /// generic form still exists for the SIMD hot loops that already
    /// know the dispatch target.
    pub fn as_i64_slice(&self) -> Option<&[i64]> {
        (self.data_type == DataType::Int64).then(|| self.as_slice::<i64>())
    }

    /// Typed f64 slice — `None` when `data_type != Float64`.
    pub fn as_f64_slice(&self) -> Option<&[f64]> {
        (self.data_type == DataType::Float64).then(|| self.as_slice::<f64>())
    }

    /// Typed bool slice — `None` when `data_type != Bool`.
    ///
    /// The underlying storage is one byte per bool (not packed);
    /// returning `&[bool]` is safe because Rust's `bool` repr is a
    /// single byte with values 0/1, matching the writes made by
    /// [`ColumnValue::write_to`].
    pub fn as_bool_slice(&self) -> Option<&[bool]> {
        if self.data_type != DataType::Bool {
            return None;
        }
        // SAFETY: `Self::push::<bool>` writes a single `0`/`1` byte
        // per element via `ColumnValue::write_to`, which matches the
        // bit-layout of `bool` (a single non-aliased byte). `self.len`
        // bytes within `self.data` are initialised because `push` is
        // the only producer.
        Some(unsafe { std::slice::from_raw_parts(self.data.as_ptr() as *const bool, self.len) })
    }

    /// SIMD-backed scalar compare for an `Int64` column.
    ///
    /// Returns a `Vec<bool>` of length `self.len` when `data_type ==
    /// Int64`, `None` otherwise — letting filter-path callers fall
    /// back to the row-at-a-time executor without panicking. Routes
    /// through the canonical [`crate::simd::compare`] dispatch
    /// (AVX-512 → AVX2 → NEON → scalar).
    pub fn compare_scalar_i64(&self, scalar: i64, op: ComparisonOp) -> Option<Vec<bool>> {
        (self.data_type == DataType::Int64)
            .then(|| simd_ops::SimdComparator::new().compare_scalar_i64(self, scalar, op))
    }

    /// SIMD-backed scalar compare for a `Float64` column.
    ///
    /// IEEE-ordered: any NaN operand yields `false` for eq/lt/le/gt/ge
    /// and `true` for ne, matching the scalar kernel reference in
    /// [`crate::simd::compare`]. Returns `None` when the column's
    /// dtype is not `Float64`.
    pub fn compare_scalar_f64(&self, scalar: f64, op: ComparisonOp) -> Option<Vec<bool>> {
        (self.data_type == DataType::Float64)
            .then(|| simd_ops::SimdComparator::new().compare_scalar_f64(self, scalar, op))
    }

    /// Materialise a dense numeric column from a slice of executor
    /// row maps — the first consumer lives in
    /// [`crate::executor::Executor::execute_filter`]'s columnar
    /// fast path (see `phase3_executor-columnar-wiring` §3.2).
    ///
    /// Reads `row[variable]` — which must be a `Value::Object`
    /// (node or relationship) — then looks up `property` first at
    /// the top level and then under a nested `"properties"` map,
    /// mirroring [`crate::executor::Executor::extract_property`]. A
    /// `None` return signals the caller to fall back to the
    /// row-at-a-time path: that happens the first time any row
    /// yields a missing property, `Value::Null`, or a `Number` that
    /// can't be coerced into `dtype`. Keeping the NULL / type-
    /// mismatch semantics on the row path preserves byte-for-byte
    /// parity with the scalar executor.
    ///
    /// Only `DataType::Int64` and `DataType::Float64` are supported
    /// today; string / bool columnar filtering is tracked by a
    /// later slice of the same phase-3 task (§3.3 keeps those on
    /// the row path unchanged).
    pub fn materialise_from_rows(
        rows: &[HashMap<String, serde_json::Value>],
        variable: &str,
        property: &str,
        dtype: DataType,
    ) -> Option<Column> {
        if !matches!(dtype, DataType::Int64 | DataType::Float64) {
            return None;
        }
        let mut column = Column::with_capacity(dtype, rows.len());
        for row in rows {
            let entity = row.get(variable)?;
            let value = extract_property_from_entity(entity, property);
            match (dtype, &value) {
                (DataType::Int64, serde_json::Value::Number(n)) => {
                    let i = n.as_i64()?;
                    column.push::<i64>(i).ok()?;
                }
                (DataType::Float64, serde_json::Value::Number(n)) => {
                    let f = n.as_f64()?;
                    column.push::<f64>(f).ok()?;
                }
                _ => return None,
            }
        }
        Some(column)
    }
}

/// Mirror of [`crate::executor::Executor::extract_property`] kept
/// local to the columnar module so the filter fast path doesn't need
/// to pull the executor into scope. Any divergence from the canonical
/// extractor is caught by the byte-for-byte parity test in
/// `executor::operators::filter`.
fn extract_property_from_entity(entity: &serde_json::Value, property: &str) -> serde_json::Value {
    if let serde_json::Value::Object(obj) = entity {
        if let Some(value) = obj.get(property) {
            if property == "_nexus_id"
                || (property != "_nexus_type"
                    && property != "_source"
                    && property != "_target"
                    && property != "_element_id")
            {
                return value.clone();
            }
        }
        if let Some(serde_json::Value::Object(props)) = obj.get("properties") {
            if let Some(value) = props.get(property) {
                return value.clone();
            }
        }
    }
    serde_json::Value::Null
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

    /// Apply SIMD-accelerated WHERE filter to columnar result.
    ///
    /// The underlying compare kernels (`crate::simd::compare`) pick
    /// the best tier at runtime (AVX-512 → AVX2 → NEON → scalar), so
    /// this entry point is architecture-agnostic — no per-arch `cfg`
    /// gate is required.
    pub fn apply_simd_where_filter(&self, condition: &WhereCondition) -> ColumnarResult {
        use self::simd_ops::SimdFilter;

        let filter = SimdFilter::new();
        let mut masks = HashMap::new();

        // Apply filter to each relevant column
        for (col_name, column) in &self.columns {
            let mask = filter.apply_where_filter(column, condition);
            masks.insert(col_name.clone(), mask);
        }

        // Combine masks for all conditions (simplified - assumes single column for now)
        let mut final_mask = vec![true; self.row_count];
        for mask in masks.values() {
            for i in 0..final_mask.len().min(mask.len()) {
                final_mask[i] = final_mask[i] && mask[i];
            }
        }

        self.filter_by_mask(&final_mask)
    }

    /// Scalar fallback for WHERE filtering
    pub fn apply_scalar_where_filter(&self, condition: &WhereCondition) -> ColumnarResult {
        let mut mask = vec![true; self.row_count];

        match condition {
            WhereCondition::Comparison {
                column: col_name,
                op,
                value,
            } => {
                if let Some(column) = self.get_column(col_name) {
                    for i in 0..self.row_count {
                        if column.is_null(i) {
                            mask[i] = false;
                            continue;
                        }

                        let matches = match column.data_type {
                            DataType::Int64 => {
                                if let serde_json::Value::Number(num) = value {
                                    if let Some(int_val) = num.as_i64() {
                                        let col_val: i64 = column.get(i).unwrap_or(0);
                                        match op {
                                            ComparisonOp::Equal => col_val == int_val,
                                            ComparisonOp::NotEqual => col_val != int_val,
                                            ComparisonOp::Greater => col_val > int_val,
                                            ComparisonOp::GreaterEqual => col_val >= int_val,
                                            ComparisonOp::Less => col_val < int_val,
                                            ComparisonOp::LessEqual => col_val <= int_val,
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            }
                            DataType::Float64 => {
                                if let serde_json::Value::Number(num) = value {
                                    if let Some(float_val) = num.as_f64() {
                                        let col_val: f64 = column.get(i).unwrap_or(0.0);
                                        match op {
                                            ComparisonOp::Equal => {
                                                (col_val - float_val).abs() < f64::EPSILON
                                            }
                                            ComparisonOp::NotEqual => {
                                                (col_val - float_val).abs() >= f64::EPSILON
                                            }
                                            ComparisonOp::Greater => col_val > float_val,
                                            ComparisonOp::GreaterEqual => col_val >= float_val,
                                            ComparisonOp::Less => col_val < float_val,
                                            ComparisonOp::LessEqual => col_val <= float_val,
                                        }
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            }
                            DataType::Bool => {
                                if let serde_json::Value::Bool(bool_val) = value {
                                    let col_val: bool = column.get(i).unwrap_or(false);
                                    match op {
                                        ComparisonOp::Equal => col_val == *bool_val,
                                        ComparisonOp::NotEqual => col_val != *bool_val,
                                        _ => false, // Other ops don't make sense for bool
                                    }
                                } else {
                                    false
                                }
                            }
                            DataType::String => {
                                if let serde_json::Value::String(str_val) = value {
                                    let col_val: String = column.get(i).unwrap_or_default();
                                    match op {
                                        ComparisonOp::Equal => col_val == *str_val,
                                        ComparisonOp::NotEqual => col_val != *str_val,
                                        _ => false, // String comparisons limited for now
                                    }
                                } else {
                                    false
                                }
                            }
                        };

                        mask[i] = matches;
                    }
                }
            }
            WhereCondition::And(conditions) => {
                for cond in conditions {
                    let partial = self.apply_scalar_where_filter(cond);
                    let partial_mask = vec![true; self.row_count]; // Simplified
                    for i in 0..mask.len() {
                        mask[i] = mask[i] && partial_mask[i];
                    }
                }
            }
            WhereCondition::Or(conditions) => {
                for cond in conditions {
                    let partial = self.apply_scalar_where_filter(cond);
                    let partial_mask = vec![false; self.row_count]; // Simplified
                    for i in 0..mask.len() {
                        mask[i] = mask[i] || partial_mask[i];
                    }
                }
            }
        }

        self.filter_by_mask(&mask)
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
    fn typed_accessor_i64_returns_slice_of_expected_length() {
        let mut column = Column::with_capacity(DataType::Int64, 8);
        for v in [1_i64, 2, 3, 4, 5] {
            column.push(v).unwrap();
        }
        let slice = column.as_i64_slice().expect("typed access");
        assert_eq!(slice, &[1, 2, 3, 4, 5]);
    }

    #[test]
    fn typed_accessor_rejects_wrong_dtype() {
        let column = Column::with_capacity(DataType::Int64, 4);
        assert!(column.as_f64_slice().is_none());
        assert!(column.as_bool_slice().is_none());
        assert!(column.as_i64_slice().is_some());
    }

    #[test]
    fn typed_accessor_f64_round_trips_values() {
        let mut column = Column::with_capacity(DataType::Float64, 4);
        for v in [1.5_f64, -2.25, f64::NAN, 0.0] {
            column.push(v).unwrap();
        }
        let slice = column.as_f64_slice().unwrap();
        assert_eq!(slice.len(), 4);
        assert_eq!(slice[0], 1.5);
        assert_eq!(slice[1], -2.25);
        assert!(slice[2].is_nan());
        assert_eq!(slice[3], 0.0);
    }

    #[test]
    fn executor_config_default_columnar_threshold() {
        use crate::executor::types::ExecutorConfig;
        let cfg = ExecutorConfig::default();
        assert_eq!(cfg.columnar_threshold, 4096);
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
