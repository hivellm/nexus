//! Columnar fast-path helpers powering the `execute_aggregate_with_projections`
//! §4 reduce kernels (SUM / MIN / MAX / AVG on dense numeric columns).
//!
//! Every function returns `None` on the first row that cannot be coerced,
//! falling through to the scalar path unchanged.

use super::super::super::engine::Executor;
use super::super::super::types::{Aggregation, Row};
use serde_json::Value;

impl Executor {
    // ── §4 columnar-reduce helpers ────────────────────────────────────
    //
    // These power the fast path in `execute_aggregate_with_projections`
    // for groupless `SUM` / `MIN` / `MAX` / `AVG` on dense numeric
    // columns. The matching scalar arms stay the authoritative fallback
    // for every other shape (strings, mixed dtypes, NULL columns, etc.)
    // — the materialisers below return `None` on the first row that
    // can't be coerced so the row path keeps its semantics untouched.

    /// For each aggregation, return `Some(value)` when the SIMD
    /// reduce kernel can handle it over `rows`, `None` when the caller
    /// must fall through to the scalar path. The returned `Vec` is
    /// positionally aligned with `aggregations`.
    pub(in crate::executor) fn compute_columnar_agg_cache(
        &self,
        rows: &[Row],
        aggregations: &[Aggregation],
        columns_for_lookup: &[String],
    ) -> Vec<Option<Value>> {
        aggregations
            .iter()
            .map(|agg| match agg {
                Aggregation::Sum { column, .. } => {
                    self.try_columnar_sum(rows, column, columns_for_lookup)
                }
                Aggregation::Avg { column, .. } => {
                    self.try_columnar_avg(rows, column, columns_for_lookup)
                }
                Aggregation::Min { column, .. } => {
                    self.try_columnar_min(rows, column, columns_for_lookup)
                }
                Aggregation::Max { column, .. } => {
                    self.try_columnar_max(rows, column, columns_for_lookup)
                }
                _ => None,
            })
            .collect()
    }

    /// Materialise every row's value at `column` as `Vec<f64>`.
    ///
    /// Returns `None` the first time a row's value is missing,
    /// `Value::Null`, or a JSON value that can't be coerced into
    /// `f64` — matching the scalar executor's strictness. Integer
    /// JSON numbers widen into `f64` to match `value_to_number`, so
    /// SUM / AVG accumulate with the exact same precision regardless
    /// of input dtype.
    fn materialize_f64_column(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Vec<f64>> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let val = self.extract_value_from_row(row, column, columns_for_lookup)?;
            let Value::Number(n) = &val else { return None };
            let f = n.as_f64()?;
            out.push(f);
        }
        Some(out)
    }

    /// Materialise every row's value at `column` as `Vec<i64>`.
    ///
    /// Strict — refuses the first row whose JSON number has a
    /// fractional part (i.e. stored as `Number::from_f64`). This is
    /// what makes the `MIN` / `MAX` fast path safe: when this
    /// returns `Some`, every input was an integer-form `Value::Number`
    /// and wrapping the `i64` kernel result via `Number::from(i64)`
    /// produces byte-for-byte identical output to the scalar path
    /// (which also keeps the original integer-form `Value`).
    fn materialize_i64_column(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Vec<i64>> {
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let val = self.extract_value_from_row(row, column, columns_for_lookup)?;
            let Value::Number(n) = &val else { return None };
            let i = n.as_i64()?;
            out.push(i);
        }
        Some(out)
    }

    fn try_columnar_sum(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Value> {
        let floats = self.materialize_f64_column(rows, column, columns_for_lookup)?;
        let sum = crate::simd::reduce::sum_f64(&floats);
        // Mirror the scalar path: return an integer `Value::Number`
        // when the sum has no fractional part, otherwise a float.
        Some(if sum.fract() == 0.0 && sum.is_finite() {
            Value::Number(serde_json::Number::from(sum as i64))
        } else {
            Value::Number(serde_json::Number::from_f64(sum).unwrap_or(serde_json::Number::from(0)))
        })
    }

    fn try_columnar_avg(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Value> {
        let floats = self.materialize_f64_column(rows, column, columns_for_lookup)?;
        if floats.is_empty() {
            return Some(Value::Null);
        }
        let sum = crate::simd::reduce::sum_f64(&floats);
        let avg = sum / floats.len() as f64;
        Some(Value::Number(
            serde_json::Number::from_f64(avg).unwrap_or(serde_json::Number::from(0)),
        ))
    }

    fn try_columnar_min(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Value> {
        // Pure-integer column: scalar keeps the original integer
        // `Value::Number`; wrapping the `i64` kernel result matches
        // that exactly.
        if let Some(ints) = self.materialize_i64_column(rows, column, columns_for_lookup) {
            let min_i = crate::simd::reduce::min_i64(&ints)?;
            return Some(Value::Number(serde_json::Number::from(min_i)));
        }
        // Float / mixed column: find the numeric minimum with the
        // SIMD kernel, then do a second pass to recover the original
        // `Value` from the first row that matches — mirrors the
        // scalar's "first occurrence wins" strict-less-than loop.
        let floats = self.materialize_f64_column(rows, column, columns_for_lookup)?;
        let min_f = crate::simd::reduce::min_f64(&floats)?;
        for row in rows {
            let val = self.extract_value_from_row(row, column, columns_for_lookup)?;
            if let Ok(num) = self.value_to_number(&val) {
                if num == min_f {
                    return Some(val);
                }
            }
        }
        None
    }

    fn try_columnar_max(
        &self,
        rows: &[Row],
        column: &str,
        columns_for_lookup: &[String],
    ) -> Option<Value> {
        if let Some(ints) = self.materialize_i64_column(rows, column, columns_for_lookup) {
            let max_i = crate::simd::reduce::max_i64(&ints)?;
            return Some(Value::Number(serde_json::Number::from(max_i)));
        }
        let floats = self.materialize_f64_column(rows, column, columns_for_lookup)?;
        let max_f = crate::simd::reduce::max_f64(&floats)?;
        for row in rows {
            let val = self.extract_value_from_row(row, column, columns_for_lookup)?;
            if let Ok(num) = self.value_to_number(&val) {
                if num == max_f {
                    return Some(val);
                }
            }
        }
        None
    }
}
