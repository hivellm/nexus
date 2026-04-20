//! Arithmetic coercions for `parser::Expression` binary ops over JSON
//! values. Every op normalises both sides to f64 (or integer where
//! possible) and re-boxes back into `Value::Number`.

use super::super::engine::Executor;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    pub(in crate::executor) fn add_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null + number or number + null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        // Check if both values are strings - then concatenate
        if let (Value::String(l_str), Value::String(r_str)) = (left, right) {
            return Ok(Value::String(format!("{}{}", l_str, r_str)));
        }

        // Check if both values are arrays - then concatenate
        if let (Value::Array(l_arr), Value::Array(r_arr)) = (left, right) {
            let mut result = l_arr.clone();
            result.extend(r_arr.iter().cloned());
            return Ok(Value::Array(result));
        }

        // Check for datetime + duration arithmetic
        if let Some(result) = self.try_datetime_add(left, right)? {
            return Ok(result);
        }

        // Check for duration + duration arithmetic
        if let Some(result) = self.try_duration_add(left, right)? {
            return Ok(result);
        }

        // Otherwise, treat as numeric addition
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l + r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite sum".to_string(),
            })
    }

    pub(in crate::executor) fn subtract_values(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Value> {
        // Handle null values - null - number or number - null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        // Check for datetime - duration arithmetic
        if let Some(result) = self.try_datetime_subtract(left, right)? {
            return Ok(result);
        }

        // Check for datetime - datetime (returns duration)
        if let Some(result) = self.try_datetime_diff(left, right)? {
            return Ok(result);
        }

        // Check for duration - duration arithmetic
        if let Some(result) = self.try_duration_subtract(left, right)? {
            return Ok(result);
        }

        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l - r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite difference".to_string(),
            })
    }

    pub(in crate::executor) fn multiply_values(
        &self,
        left: &Value,
        right: &Value,
    ) -> Result<Value> {
        // Handle null values - null * number or number * null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        serde_json::Number::from_f64(l * r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite product".to_string(),
            })
    }

    pub(in crate::executor) fn divide_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null / number or number / null = null (Neo4j behavior)
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }
        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;
        if r == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "division by zero".to_string(),
            });
        }
        serde_json::Number::from_f64(l / r)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite quotient".to_string(),
            })
    }

    pub(in crate::executor) fn power_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null ^ anything or anything ^ null = null
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        let base = self.value_to_number(left)?;
        let exp = self.value_to_number(right)?;
        let result = base.powf(exp);

        serde_json::Number::from_f64(result)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite power result".to_string(),
            })
    }

    pub(in crate::executor) fn modulo_values(&self, left: &Value, right: &Value) -> Result<Value> {
        // Handle null values - null % anything or anything % null = null
        if left.is_null() || right.is_null() {
            return Ok(Value::Null);
        }

        let l = self.value_to_number(left)?;
        let r = self.value_to_number(right)?;

        if r == 0.0 {
            return Err(Error::TypeMismatch {
                expected: "non-zero".to_string(),
                actual: "modulo by zero".to_string(),
            });
        }

        // Use f64::rem_euclid for modulo operation
        let result = l.rem_euclid(r);

        serde_json::Number::from_f64(result)
            .map(Value::Number)
            .ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "non-finite modulo result".to_string(),
            })
    }
}
