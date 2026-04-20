//! Arithmetic coercions for `parser::Expression` binary ops over JSON
//! values. Every op normalises both sides to f64 (or integer where
//! possible) and re-boxes back into `Value::Number`.

use super::super::engine::Executor;
use crate::{Error, Result};
use serde_json::Value;

/// phase6 §4 — return the operands as `(i64, i64)` iff both values are
/// JSON numbers whose storage is integer (not float). Following Cypher /
/// openCypher rules, arithmetic that has only integer operands returns
/// an integer; the moment a float is introduced the whole expression
/// promotes to float. Pre-fix every operator unconditionally called
/// `value_to_number` (f64), so `RETURN 1 + 2 * 3` produced
/// `Number::Float(7.0)` instead of `Number::Int(7)`.
fn both_as_i64(left: &Value, right: &Value) -> Option<(i64, i64)> {
    let l = left.as_i64()?;
    let r = right.as_i64()?;
    // `as_i64` returns `Some` for both Number::PosInt / Number::NegInt
    // and for Number::Float values that happen to be whole (`1.0`,
    // `2.0`). Guard against the float case so `1.0 + 2` stays a float
    // per Cypher promotion rules.
    if left.as_f64()?.fract() != 0.0 || right.as_f64()?.fract() != 0.0 {
        return None;
    }
    if matches!(left, Value::Number(n) if n.is_f64())
        || matches!(right, Value::Number(n) if n.is_f64())
    {
        return None;
    }
    Some((l, r))
}

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

        // phase6 §4 — preserve integer typing when both operands are ints.
        if let Some((li, ri)) = both_as_i64(left, right) {
            if let Some(sum) = li.checked_add(ri) {
                return Ok(Value::Number(serde_json::Number::from(sum)));
            }
            // Integer overflow — fall through to f64 path so the result
            // is still produceable (matches Neo4j's behaviour of
            // promoting only on overflow-would-happen).
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

        if let Some((li, ri)) = both_as_i64(left, right) {
            if let Some(diff) = li.checked_sub(ri) {
                return Ok(Value::Number(serde_json::Number::from(diff)));
            }
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
        if let Some((li, ri)) = both_as_i64(left, right) {
            if let Some(prod) = li.checked_mul(ri) {
                return Ok(Value::Number(serde_json::Number::from(prod)));
            }
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
        // phase6 §4 — Cypher integer division: int / int stays int
        // (`100 / 4 = 25`, `7 / 2 = 3`). Only promote to float if either
        // operand is itself a float.
        if let Some((li, ri)) = both_as_i64(left, right) {
            if ri == 0 {
                return Err(Error::TypeMismatch {
                    expected: "non-zero".to_string(),
                    actual: "division by zero".to_string(),
                });
            }
            // i64::MIN / -1 overflows — fall through to f64 in that case.
            if let Some(q) = li.checked_div(ri) {
                return Ok(Value::Number(serde_json::Number::from(q)));
            }
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

        if let Some((li, ri)) = both_as_i64(left, right) {
            if ri == 0 {
                return Err(Error::TypeMismatch {
                    expected: "non-zero".to_string(),
                    actual: "modulo by zero".to_string(),
                });
            }
            if let Some(m) = li.checked_rem_euclid(ri) {
                return Ok(Value::Number(serde_json::Number::from(m)));
            }
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
