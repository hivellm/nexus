//! WHERE-clause evaluator plus value comparison helpers shared across
//! operators. `evaluate_predicate` runs a parsed predicate string against a
//! row; `evaluate_expression` evaluates a `parser::Expression` within a row.
//! The rest are type-coercion and ordering helpers used by both the
//! predicate evaluator and the sort operator.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::temporal_parse;
use super::temporal_retag;
use super::temporal_value;
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Neo4j's own duration ordering key: `months * AVG_SECONDS_PER_MONTH +
/// days * 86_400 + seconds`, matching `DurationValue.unsafeCompareTo`'s
/// "average length" model rather than a plain lexicographic
/// `(months, days, seconds, nanos)` tuple compare — `duration('P1M')`
/// orders *before* `duration('P31D')` (2,629,746s < 2,678,400s) even
/// though a bare tuple compare would put the larger `months` field first.
/// `i128` (not `i64`) avoids overflow: `i64::MAX` months times the
/// ~2.6M-second average month length overflows `i64` by several orders of
/// magnitude.
fn duration_average_length_key(parts: (i64, i64, i64, i32)) -> i128 {
    // `AVG_SECONDS_PER_MONTH` is an `f64` (shared with the ISO-duration
    // string parser's own fractional-month carry) but is always a whole
    // number of seconds (365.2425 days/year * 86400 / 12 = exactly
    // 2_629_746.0) — the `as i64` cast below relies on that exactness,
    // so assert it explicitly rather than leaving the coupling implicit.
    debug_assert!(
        temporal_parse::AVG_SECONDS_PER_MONTH.fract() == 0.0,
        "AVG_SECONDS_PER_MONTH must be a whole number of seconds for the `as i64` cast below \
         to be exact"
    );
    let (months, days, seconds, _nanos) = parts;
    i128::from(months) * i128::from(temporal_parse::AVG_SECONDS_PER_MONTH as i64)
        + i128::from(days) * 86_400_i128
        + i128::from(seconds)
}

/// Total order over two normalized `(months, days, seconds, nanos)`
/// duration tuples: primary key is [`duration_average_length_key`]
/// (Neo4j's average-length model), `nanos` breaks a tie on that key, and
/// the raw tuple is a final tiebreak so two durations that land on the
/// same average-length-and-nanos key (a real possibility — the average
/// length collapses distinct `(months, days, seconds)` combinations onto
/// the same total) still resolve deterministically rather than comparing
/// `Equal` when they aren't identical.
fn compare_duration_parts(a: (i64, i64, i64, i32), b: (i64, i64, i64, i32)) -> std::cmp::Ordering {
    duration_average_length_key(a)
        .cmp(&duration_average_length_key(b))
        .then_with(|| a.3.cmp(&b.3))
        .then_with(|| a.cmp(&b))
}

impl Executor {
    pub(in crate::executor) fn evaluate_predicate(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<bool> {
        match expr {
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(node, left, context)?;
                let right_val = self.evaluate_expression(node, right, context)?;

                match op {
                    parser::BinaryOperator::Equal => {
                        // In Neo4j, null = null returns null (which evaluates to false in WHERE), and null = anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(false) // null comparisons in WHERE clauses evaluate to false
                        } else {
                            // Use numeric comparison for numbers to handle 1.0 == 1
                            let is_equal = self.values_equal_for_comparison(&left_val, &right_val);
                            Ok(is_equal)
                        }
                    }
                    parser::BinaryOperator::NotEqual => {
                        // In Neo4j, null <> null returns null (which evaluates to false in WHERE), and null <> anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(false) // null comparisons in WHERE clauses evaluate to false
                        } else {
                            // Must be the exact negation of `Equal` above. With a
                            // raw `!=` it was not: `1 <> 1.0` and `1 = 1.0` were
                            // BOTH true, because serde's `Number(1)` and
                            // `Number(1.0)` differ structurally while Cypher
                            // treats them as the same value.
                            Ok(!self.values_equal_for_comparison(&left_val, &right_val))
                        }
                    }
                    parser::BinaryOperator::LessThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a < b)
                    }
                    parser::BinaryOperator::LessThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a <= b)
                    }
                    parser::BinaryOperator::GreaterThan => {
                        self.compare_values(&left_val, &right_val, |a, b| a > b)
                    }
                    parser::BinaryOperator::GreaterThanOrEqual => {
                        self.compare_values(&left_val, &right_val, |a, b| a >= b)
                    }
                    parser::BinaryOperator::And => {
                        // 3VL: NULL result drops the row in a WHERE filter.
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::and_3vl(l, r).unwrap_or(false))
                    }
                    parser::BinaryOperator::Or => {
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::or_3vl(l, r).unwrap_or(false))
                    }
                    parser::BinaryOperator::Xor => {
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::xor_3vl(l, r).unwrap_or(false))
                    }
                    parser::BinaryOperator::StartsWith => {
                        // 3VL: a NULL operand yields NULL → drop the row.
                        if left_val.is_null() || right_val.is_null() {
                            return Ok(false);
                        }
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.starts_with(&right_str))
                    }
                    parser::BinaryOperator::EndsWith => {
                        if left_val.is_null() || right_val.is_null() {
                            return Ok(false);
                        }
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.ends_with(&right_str))
                    }
                    parser::BinaryOperator::Contains => {
                        if left_val.is_null() || right_val.is_null() {
                            return Ok(false);
                        }
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(left_str.contains(&right_str))
                    }
                    parser::BinaryOperator::RegexMatch => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        // Use regex crate for pattern matching
                        match regex::Regex::new(&right_str) {
                            Ok(re) => Ok(re.is_match(&left_str)),
                            Err(_) => Ok(false), // Invalid regex pattern returns false
                        }
                    }
                    parser::BinaryOperator::In => {
                        // IN operator: left IN right (where right is a list)
                        // Check if left_val is in the right_val list
                        match &right_val {
                            Value::Array(list) => {
                                // Check if left_val is in the list
                                Ok(list.iter().any(|item| item == &left_val))
                            }
                            _ => {
                                // Right side is not a list, return false
                                Ok(false)
                            }
                        }
                    }
                    parser::BinaryOperator::Power => {
                        // Power operator: left ^ right
                        // For predicates, we need to return a boolean
                        // But power is a numeric operation, so we compare result to 0
                        let base = self.value_to_number(&left_val)?;
                        let exp = self.value_to_number(&right_val)?;
                        let result = base.powf(exp);
                        Ok(result != 0.0 && result.is_finite())
                    }
                    _ => Ok(false), // Other operators not implemented
                }
            }
            parser::Expression::UnaryOp { op, operand } => {
                let operand_val = self.evaluate_expression(node, operand, context)?;
                match op {
                    parser::UnaryOperator::Not => {
                        // 3VL: NOT NULL → NULL → drop the row in a WHERE filter.
                        let operand = self.logical_operand(&operand_val)?;
                        Ok(Self::not_3vl(operand).unwrap_or(false))
                    }
                    _ => Ok(false),
                }
            }
            parser::Expression::IsNull { expr, negated } => {
                let value = self.evaluate_expression(node, expr, context)?;
                let is_null = value.is_null();
                Ok(if *negated { !is_null } else { is_null })
            }
            _ => {
                let result = self.evaluate_expression(node, expr, context)?;
                self.value_to_bool(&result)
            }
        }
    }

    /// Evaluate an expression against a node
    pub(in crate::executor) fn evaluate_expression(
        &self,
        node: &Value,
        expr: &parser::Expression,
        context: &ExecutionContext,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Variable(name) => {
                if let Some(value) = context.get_variable(name) {
                    Ok(value.clone())
                } else {
                    Ok(Value::Null)
                }
            }
            parser::Expression::PropertyAccess { variable, property } => {
                if variable == "n" || variable == "node" {
                    // Access property of the current node
                    if let Value::Object(props) = node {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                } else {
                    // Access property of a variable
                    if let Some(Value::Object(props)) = context.get_variable(variable) {
                        Ok(props.get(property).cloned().unwrap_or(Value::Null))
                    } else {
                        Ok(Value::Null)
                    }
                }
            }
            parser::Expression::ArrayIndex { base, index } => {
                // Evaluate the base expression (should return an array)
                let base_value = self.evaluate_expression(node, base, context)?;

                // Evaluate the index expression (should return an integer)
                let index_value = self.evaluate_expression(node, index, context)?;

                // Extract index as i64
                // Handle both integer and float numbers (floats come from unary minus)
                let idx = match index_value {
                    Value::Number(n) => n
                        .as_i64()
                        .or_else(|| n.as_f64().map(|f| f as i64))
                        .unwrap_or(0),
                    _ => return Ok(Value::Null), // Invalid index type
                };

                // Access array element
                match base_value {
                    Value::Array(arr) => {
                        // Handle negative indices (Python-style)
                        let array_len = arr.len() as i64;
                        let actual_idx = if idx < 0 {
                            (array_len + idx) as usize
                        } else {
                            idx as usize
                        };

                        // Return element or null if out of bounds
                        Ok(arr.get(actual_idx).cloned().unwrap_or(Value::Null))
                    }
                    _ => Ok(Value::Null), // Base is not an array
                }
            }
            parser::Expression::ArraySlice { base, start, end } => {
                // Evaluate the base expression (should return an array)
                let base_value = self.evaluate_expression(node, base, context)?;

                match base_value {
                    Value::Array(arr) => {
                        let array_len = arr.len() as i64;

                        // Evaluate start index (default to 0)
                        let start_idx = if let Some(start_expr) = start {
                            let start_val = self.evaluate_expression(node, start_expr, context)?;
                            match start_val {
                                Value::Number(n) => {
                                    // Handle both integer and float numbers (floats come from unary minus)
                                    let idx = n
                                        .as_i64()
                                        .or_else(|| n.as_f64().map(|f| f as i64))
                                        .unwrap_or(0);
                                    // Handle negative indices
                                    if idx < 0 {
                                        ((array_len + idx).max(0)) as usize
                                    } else {
                                        idx.min(array_len) as usize
                                    }
                                }
                                _ => 0,
                            }
                        } else {
                            0
                        };

                        // Evaluate end index (default to array length)
                        let end_idx = if let Some(end_expr) = end {
                            let end_val = self.evaluate_expression(node, end_expr, context)?;
                            match end_val {
                                Value::Number(n) => {
                                    // Handle both integer and float numbers (floats come from unary minus)
                                    let idx = n
                                        .as_i64()
                                        .or_else(|| n.as_f64().map(|f| f as i64))
                                        .unwrap_or(array_len);
                                    // Handle negative indices
                                    // In Cypher, negative end index excludes that many elements from the end
                                    // e.g., [1..-1] means from index 1 to (length - 1), excluding the last element
                                    if idx < 0 {
                                        let calculated = array_len + idx;
                                        // Ensure we don't go below 0, but negative end should exclude elements
                                        if calculated <= 0 {
                                            0
                                        } else {
                                            calculated as usize
                                        }
                                    } else {
                                        idx.min(array_len) as usize
                                    }
                                }
                                _ => arr.len(),
                            }
                        } else {
                            arr.len()
                        };

                        // Return slice (empty if start >= end)
                        if start_idx <= end_idx && start_idx < arr.len() {
                            let slice = arr[start_idx..end_idx.min(arr.len())].to_vec();
                            Ok(Value::Array(slice))
                        } else {
                            Ok(Value::Array(Vec::new()))
                        }
                    }
                    _ => Ok(Value::Null), // Base is not an array
                }
            }
            parser::Expression::Literal(literal) => match literal {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => Ok(Value::Number(
                    serde_json::Number::from_f64(*f).unwrap_or(serde_json::Number::from(0)),
                )),
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
                parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            parser::Expression::Parameter(name) => {
                context.params.get(name).cloned().ok_or_else(|| {
                    Error::CypherExecution(format!(
                        "ERR_MISSING_PARAMETER: parameter ${} not provided",
                        name
                    ))
                })
            }
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_expression(node, left, context)?;
                let right_val = self.evaluate_expression(node, right, context)?;

                match op {
                    parser::BinaryOperator::And => {
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::tri_bool_to_value(Self::and_3vl(l, r)))
                    }
                    parser::BinaryOperator::Or => {
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::tri_bool_to_value(Self::or_3vl(l, r)))
                    }
                    parser::BinaryOperator::Xor => {
                        let l = self.logical_operand(&left_val)?;
                        let r = self.logical_operand(&right_val)?;
                        Ok(Self::tri_bool_to_value(Self::xor_3vl(l, r)))
                    }
                    parser::BinaryOperator::Equal => {
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Bool(left_val == right_val))
                        }
                    }
                    parser::BinaryOperator::NotEqual => {
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            // Exact negation of `Equal`; see the same fix in
                            // `evaluate_predicate`.
                            Ok(Value::Bool(
                                !self.values_equal_for_comparison(&left_val, &right_val),
                            ))
                        }
                    }
                    // Ordering is undefined across types — see
                    // `Self::comparable_kinds`. A NULL operand already yields
                    // NULL through the arms above.
                    parser::BinaryOperator::LessThan
                    | parser::BinaryOperator::LessThanOrEqual
                    | parser::BinaryOperator::GreaterThan
                    | parser::BinaryOperator::GreaterThanOrEqual => {
                        if left_val.is_null()
                            || right_val.is_null()
                            || !Self::comparable_kinds(&left_val, &right_val)
                        {
                            return Ok(Value::Null);
                        }
                        let ordering = self.compare_values_for_sort(&left_val, &right_val);
                        Ok(Value::Bool(match op {
                            parser::BinaryOperator::LessThan => {
                                ordering == std::cmp::Ordering::Less
                            }
                            parser::BinaryOperator::LessThanOrEqual => {
                                ordering != std::cmp::Ordering::Greater
                            }
                            parser::BinaryOperator::GreaterThan => {
                                ordering == std::cmp::Ordering::Greater
                            }
                            _ => ordering != std::cmp::Ordering::Less,
                        }))
                    }
                    parser::BinaryOperator::Add => self.add_values(&left_val, &right_val),
                    parser::BinaryOperator::Subtract => self.subtract_values(&left_val, &right_val),
                    parser::BinaryOperator::Multiply => self.multiply_values(&left_val, &right_val),
                    parser::BinaryOperator::Divide => self.divide_values(&left_val, &right_val),
                    parser::BinaryOperator::Modulo => self.modulo_values(&left_val, &right_val),
                    parser::BinaryOperator::Power => self.power_values(&left_val, &right_val),
                    _ => Ok(Value::Null), // Other operators not implemented in evaluate_expression
                }
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Evaluate input expression if present (generic CASE)
                let input_value = if let Some(input_expr) = input {
                    Some(self.evaluate_expression(node, input_expr, context)?)
                } else {
                    None
                };

                // Evaluate WHEN clauses
                for when_clause in when_clauses {
                    let condition_value =
                        self.evaluate_expression(node, &when_clause.condition, context)?;

                    // For generic CASE: compare input with condition
                    // For simple CASE: evaluate condition as boolean
                    let matches = if let Some(ref input_val) = input_value {
                        // Generic CASE: input == condition
                        input_val == &condition_value
                    } else {
                        // Simple CASE: condition is boolean expression
                        self.value_to_bool(&condition_value)?
                    };

                    if matches {
                        return self.evaluate_expression(node, &when_clause.result, context);
                    }
                }

                // No WHEN clause matched, return ELSE or NULL
                if let Some(else_expr) = else_clause {
                    self.evaluate_expression(node, else_expr, context)
                } else {
                    Ok(Value::Null)
                }
            }
            _ => Ok(Value::Null), // Other expressions not implemented in MVP
        }
    }

    /// Compare two values for equality, handling numeric type differences (1.0 == 1)
    ///
    /// A tagged intermediate temporal value (see `super::temporal_value`)
    /// is canonicalized to its ISO string before the comparison below runs
    /// — otherwise `WHERE n.date = date('2024-11-01')` would compare a
    /// plain-string node property against a `{_nexus_temporal_type:
    /// "date", ...}` object and never match, since equality/WHERE
    /// filtering happens mid-pipeline, well before the single
    /// projection-boundary canonicalization pass in `Executor::execute`.
    /// Whether `left` and `right` are of the same Cypher TYPE for comparison
    /// purposes, with `INTEGER` and `FLOAT` counted as one numeric kind.
    ///
    /// The ordering operators (`<`, `<=`, `>`, `>=`) are only defined WITHIN a
    /// kind: openCypher yields `null` for `1 < 'text'`, not a verdict. Without
    /// this gate they fell through to `compare_values_for_sort`, whose last arm
    /// stringifies both operands — so `1 < 'text'` compared `"1"` against
    /// `"text"` and confidently answered `true`, and a `WHERE` over
    /// heterogeneous properties kept or dropped rows by spelling.
    ///
    /// Source: openCypher TCK `expressions/comparison/Comparison2.feature` [3]
    /// "Comparing across types yields null, except numbers", which draws every
    /// pair from `[node, rel, path, '', 1, 3.14, true, null, [], {}]` and expects
    /// only the numeric pairs to survive a `WHERE result`.
    ///
    /// Equality does NOT use this gate: `'1.0' = 1.0` is `false`, not `null`
    /// (same file's sibling, `Comparison1.feature` [9]). Neither does `ORDER BY`,
    /// which needs a total order over every type — see
    /// `operators::project::order_by_type_rank`.
    pub(in crate::executor) fn comparable_kinds(left: &Value, right: &Value) -> bool {
        value_type_kind(left) == value_type_kind(right)
    }

    pub(in crate::executor) fn values_equal_for_comparison(
        &self,
        left: &Value,
        right: &Value,
    ) -> bool {
        let left_canon;
        let right_canon;
        let left = match temporal_value::canonicalize_temporal(left) {
            Some(s) => {
                left_canon = Value::String(s);
                &left_canon
            }
            None => left,
        };
        let right = match temporal_value::canonicalize_temporal(right) {
            Some(s) => {
                right_canon = Value::String(s);
                &right_canon
            }
            None => right,
        };
        match (left, right) {
            (Value::Number(a), Value::Number(b)) => {
                // Compare numbers (handle int/float conversion)
                if let (Some(a_i64), Some(b_i64)) = (a.as_i64(), b.as_i64()) {
                    a_i64 == b_i64
                } else if let (Some(a_f64), Some(b_f64)) = (a.as_f64(), b.as_f64()) {
                    (a_f64 - b_f64).abs() < f64::EPSILON * 10.0
                } else {
                    false
                }
            }
            (Value::String(a), Value::String(b)) => {
                // String comparison - exact match
                a == b
            }
            // NO string<->number coercion: a STRING never equals a NUMBER in
            // Cypher, however numeric its spelling. `'1.0' = 1.0` is `false`
            // (openCypher TCK `expressions/comparison/Comparison1.feature` [9]);
            // it used to parse the string and answer `true`, which also made an
            // inline property match `{id: '1'}` find a node whose `id` is the
            // number 1.
            _ => value_type_kind(left) == value_type_kind(right) && left == right,
        }
    }

    /// Compare two values using a comparison function
    pub(in crate::executor) fn compare_values<F>(
        &self,
        left: &Value,
        right: &Value,
        compare_fn: F,
    ) -> Result<bool>
    where
        F: FnOnce(f64, f64) -> bool,
    {
        let left_num = self.value_to_number(left)?;
        let right_num = self.value_to_number(right)?;
        Ok(compare_fn(left_num, right_num))
    }

    /// Convert a value to a number
    pub(in crate::executor) fn value_to_number(&self, value: &Value) -> Result<f64> {
        match value {
            Value::Number(n) => n.as_f64().ok_or_else(|| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "invalid number".to_string(),
            }),
            Value::String(s) => s.parse::<f64>().map_err(|_| Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "string".to_string(),
            }),
            Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
            Value::Null => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "null".to_string(),
            }),
            _ => Err(Error::TypeMismatch {
                expected: "number".to_string(),
                actual: "unknown type".to_string(),
            }),
        }
    }

    /// Convert a value to a boolean
    pub(in crate::executor) fn value_to_bool(&self, value: &Value) -> Result<bool> {
        match value {
            Value::Bool(b) => Ok(*b),
            Value::Number(n) => Ok(n.as_f64().unwrap_or(0.0) != 0.0),
            Value::String(s) => Ok(!s.is_empty()),
            Value::Null => Ok(false),
            Value::Array(arr) => Ok(!arr.is_empty()),
            Value::Object(obj) => Ok(!obj.is_empty()),
        }
    }

    /// Coerce a value to a three-valued logical operand: `Some(true)`,
    /// `Some(false)`, or `None` (Cypher `NULL`). Unlike the lenient
    /// [`Self::value_to_bool`] (used for WHERE-filter truthiness and CASE
    /// conditions), Cypher's logical operators are STRICT about operand type:
    /// `123 AND true` is an error, not a coercion. Only `BOOLEAN` and `NULL`
    /// are valid operands to `AND`/`OR`/`NOT`.
    pub(in crate::executor) fn logical_operand(&self, value: &Value) -> Result<Option<bool>> {
        match value {
            Value::Bool(b) => Ok(Some(*b)),
            Value::Null => Ok(None),
            other => Err(Error::CypherSyntax(format!(
                "InvalidArgumentType: logical operator expected BOOLEAN or NULL, got {}",
                Self::value_type_name(other)
            ))),
        }
    }

    /// Short human-readable Cypher type name for error messages.
    pub(in crate::executor) fn value_type_name(value: &Value) -> &'static str {
        match value {
            Value::Null => "NULL",
            Value::Bool(_) => "BOOLEAN",
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    "INTEGER"
                } else {
                    "FLOAT"
                }
            }
            Value::String(_) => "STRING",
            Value::Array(_) => "LIST",
            Value::Object(_) => "MAP",
        }
    }

    /// Kleene AND: `false` dominates, then `NULL`, else `true`.
    pub(in crate::executor) fn and_3vl(a: Option<bool>, b: Option<bool>) -> Option<bool> {
        match (a, b) {
            (Some(false), _) | (_, Some(false)) => Some(false),
            (Some(true), Some(true)) => Some(true),
            _ => None,
        }
    }

    /// Kleene OR: `true` dominates, then `NULL`, else `false`.
    pub(in crate::executor) fn or_3vl(a: Option<bool>, b: Option<bool>) -> Option<bool> {
        match (a, b) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        }
    }

    /// Kleene NOT: `NULL` propagates.
    pub(in crate::executor) fn not_3vl(a: Option<bool>) -> Option<bool> {
        a.map(|v| !v)
    }

    /// Kleene XOR: `NULL` on either side propagates; otherwise `a != b`.
    pub(in crate::executor) fn xor_3vl(a: Option<bool>, b: Option<bool>) -> Option<bool> {
        match (a, b) {
            (Some(x), Some(y)) => Some(x != y),
            _ => None,
        }
    }

    /// Wrap a three-valued logical result into a Cypher `Value`
    /// (`Some(b)` → `Bool`, `None` → `Null`).
    pub(in crate::executor) fn tri_bool_to_value(o: Option<bool>) -> Value {
        match o {
            Some(b) => Value::Bool(b),
            None => Value::Null,
        }
    }

    /// Find relationships for a node

    /// Get a column value from a node for sorting
    pub(in crate::executor) fn get_column_value(&self, node: &Value, column: &str) -> Value {
        if let Value::Object(props) = node {
            if let Some(value) = props.get(column) {
                value.clone()
            } else {
                // Try to access as property access (e.g., "n.name")
                if let Some(dot_pos) = column.find('.') {
                    let var_name = &column[..dot_pos];
                    let prop_name = &column[dot_pos + 1..];

                    if let Some(Value::Object(var_props)) = props.get(var_name) {
                        if let Some(prop_value) = var_props.get(prop_name) {
                            return prop_value.clone();
                        }
                    }
                }
                Value::Null
            }
        } else {
            Value::Null
        }
    }

    /// Compare values for sorting
    ///
    /// Tagged intermediate temporal values (see `super::temporal_value`)
    /// canonicalize to their ISO string before comparing — the `(Object,
    /// Object)` shape would otherwise fall through to the generic
    /// `value_to_string` catch-all below, which renders any object as its
    /// entry count (`"{4}"`), not a chronological order. Canonical ISO
    /// strings sort correctly for same-kind dates/times (lexicographic
    /// order matches chronological order for a fixed-width `YYYY-MM-DD`/
    /// `HH:MM:SS[.fraction]` form).
    ///
    /// Duration ordering is wrong under plain lexicographic string
    /// comparison: the canonical string is variable-width per unit, so
    /// `"PT10H"` would sort before `"PT9H"` (`'1' < '9'`) even though 10
    /// hours is the longer duration. The `(String, String)` arm below
    /// special-cases this: each operand independently re-derives as a
    /// tagged `duration` or not (via [`temporal_retag::retag_duration`] —
    /// handles both an already-tagged value and a stored canonical string
    /// alike), and the four `(Option, Option)` outcomes are ranked by
    /// class first — `(Some, Some)` compares via
    /// [`compare_duration_parts`] (Neo4j's average-length model, NOT a
    /// bare component tuple — `duration('P1M')` orders *before*
    /// `duration('P31D')`, matching `DurationValue.unsafeCompareTo`
    /// converting `months`/`days`/`seconds` to a common "average length"
    /// unit before comparing, not simply comparing `months` first);
    /// `(Some, None)`/`(None, Some)` rank every duration before every
    /// non-duration string, consistently in both orders; `(None, None)`
    /// falls back to plain string comparison. This class-rank step is
    /// required, not cosmetic: without it, comparing a duration against an
    /// ordinary string (e.g. sorting `['PT10H', 'PT5', 'PT9H']`, where
    /// `'PT5'` isn't a valid duration) is inconsistent — `a < b` and
    /// `b < a` could both hold for different `(a, b)` pairs depending on
    /// which side re-derives as a duration — which breaks the total order
    /// `Ord`-based sorting requires and risks an inconsistent-comparator
    /// panic in `sort_by`. This function backs both `ORDER BY` and the
    /// `<`/`<=`/`>`/`>=` operators (see `projection/core.rs`'s `BinaryOp`
    /// match), so a duration comparison in a `WHERE` clause resolves via
    /// the same total order.
    pub(in crate::executor) fn compare_values_for_sort(
        &self,
        a: &Value,
        b: &Value,
    ) -> std::cmp::Ordering {
        let a_canon;
        let b_canon;
        let a = match temporal_value::canonicalize_temporal(a) {
            Some(s) => {
                a_canon = Value::String(s);
                &a_canon
            }
            None => a,
        };
        let b = match temporal_value::canonicalize_temporal(b) {
            Some(s) => {
                b_canon = Value::String(s);
                &b_canon
            }
            None => b,
        };
        match (a, b) {
            (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
            (Value::Null, _) => std::cmp::Ordering::Less,
            (_, Value::Null) => std::cmp::Ordering::Greater,
            (Value::Number(a_num), Value::Number(b_num)) => {
                let a_f64 = a_num.as_f64().unwrap_or(0.0);
                let b_f64 = b_num.as_f64().unwrap_or(0.0);
                a_f64
                    .partial_cmp(&b_f64)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
            (Value::String(a_str), Value::String(b_str)) => {
                let a_duration = temporal_retag::retag_duration(a)
                    .and_then(|v| temporal_value::duration_components(&v));
                let b_duration = temporal_retag::retag_duration(b)
                    .and_then(|v| temporal_value::duration_components(&v));
                match (a_duration, b_duration) {
                    (Some(a_parts), Some(b_parts)) => compare_duration_parts(a_parts, b_parts),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => a_str.cmp(b_str),
                }
            }
            (Value::Bool(a_bool), Value::Bool(b_bool)) => a_bool.cmp(b_bool),
            (Value::Array(a_arr), Value::Array(b_arr)) => match a_arr.len().cmp(&b_arr.len()) {
                std::cmp::Ordering::Equal => {
                    for (a_item, b_item) in a_arr.iter().zip(b_arr.iter()) {
                        let comparison = self.compare_values_for_sort(a_item, b_item);
                        if comparison != std::cmp::Ordering::Equal {
                            return comparison;
                        }
                    }
                    std::cmp::Ordering::Equal
                }
                other => other,
            },
            _ => {
                // Convert to strings for comparison
                let a_str = self.value_to_string(a);
                let b_str = self.value_to_string(b);
                a_str.cmp(&b_str)
            }
        }
    }

    /// Convert a value to string for comparison
    pub(in crate::executor) fn value_to_string(&self, value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Null => "null".to_string(),
            Value::Array(arr) => format!("[{}]", arr.len()),
            Value::Object(obj) => format!("{{{}}}", obj.len()),
        }
    }
}

/// The Cypher TYPE of a value, at the granularity comparison and ordering care
/// about. `INTEGER` and `FLOAT` collapse into [`ValueKind::Number`] because Cypher
/// treats them as one numeric family: `1 = 1.0` is `true` and `1 < 3.14` is
/// defined.
///
/// One taxonomy, two uses: [`Executor::comparable_kinds`] asks whether two values
/// are of the SAME kind (the ordering operators are undefined across kinds), and
/// `operators::project::order_by_type_rank` assigns each kind its position in
/// `ORDER BY`'s total order. Keeping a single classifier means the two cannot
/// drift into disagreeing about what a value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::executor) enum ValueKind {
    Null,
    Map,
    Node,
    Relationship,
    List,
    Path,
    String,
    Boolean,
    Number,
}

/// Classify a value for comparison and ordering. Entity kinds are recognised by
/// their markers (`is_node_value` / `is_relationship_value`) rather than by the
/// presence of any particular property — a node with a `type` property is not a
/// relationship.
pub(in crate::executor) fn value_type_kind(value: &Value) -> ValueKind {
    // A temporal value has TWO representations — the executor's tagged object
    // while it flows through evaluation, and its canonical ISO-8601 string once
    // stored — and `compare_values_for_sort` canonicalizes both operands before
    // comparing them. Classify the same way, or `duration('PT10H') < d.stored`
    // sees Map-vs-String, reads as incomparable, and returns NULL where the
    // comparator would have compared components. (That temporals classify as
    // STRING at all is the known cost of string-typed temporal storage: nothing
    // downstream can tell `'PT10H'` from `duration('PT10H')`.)
    if temporal_value::canonicalize_temporal(value).is_some() {
        return ValueKind::String;
    }
    match value {
        Value::Null => ValueKind::Null,
        Value::Bool(_) => ValueKind::Boolean,
        Value::Number(_) => ValueKind::Number,
        Value::String(_) => ValueKind::String,
        Value::Array(_) => ValueKind::List,
        Value::Object(map) => {
            if crate::executor::is_node_value(value) {
                ValueKind::Node
            } else if crate::executor::is_relationship_value(value) {
                ValueKind::Relationship
            } else if map.contains_key("nodes") && map.contains_key("relationships") {
                ValueKind::Path
            } else {
                ValueKind::Map
            }
        }
    }
}
