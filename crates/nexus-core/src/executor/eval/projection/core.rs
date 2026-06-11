//! Core projection-expression evaluator and COLLECT subquery support.
//!
//! Contains `evaluate_projection_expression` — the main entry point used by
//! `Project`, `With`, `Aggregate`, and `Filter` operators — and
//! `evaluate_collect_subquery` together with its private helper methods.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    /// Main expression evaluator for the projection layer.
    ///
    /// Dispatches over every `parser::Expression` variant, evaluating literals,
    /// variable lookups, property accesses, arithmetic, string/list/map ops,
    /// CASE, pattern-exists, collection comprehensions, and built-in functions.
    pub(in crate::executor) fn evaluate_projection_expression(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Variable(name) => {
                let result = row.get(name).cloned().unwrap_or(Value::Null);
                tracing::debug!(
                    "evaluate_projection_expression: Variable '{}' -> {:?}",
                    name,
                    result
                );
                Ok(result)
            }
            parser::Expression::PropertyAccess { variable, property } => {
                // Check if this is a point method call (e.g., point.distance())
                if property == "distance" {
                    // Get the point from the variable
                    if let Some(Value::Object(_)) = row.get(variable) {
                        // This is a point object, but we need another point to calculate distance
                        // For now, return a function that can be called with another point
                        // In Cypher, this would be: point1.distance(point2)
                        // We'll handle this as a special case - the syntax would be different
                        // For now, return null and document that distance() function should be used
                        return Ok(Value::Null);
                    }
                }

                // First try to get the entity from the row
                let mut entity_opt = if let Some(e) = row.get(variable) {
                    Some(e.clone())
                } else {
                    // If not in row, try to get from context variables (for single values, not arrays)
                    context.get_variable(variable).and_then(|v| {
                        // If it's an array, take the first element (for compatibility)
                        match v {
                            Value::Array(arr) => arr.first().cloned(),
                            _ => Some(v.clone()),
                        }
                    })
                };

                // CRITICAL FIX: If property is not found and entity is a node, reload it from storage
                // This handles the case where prop_ptr was reset to 0 and properties need to be recovered via reverse_index
                if let Some(ref entity) = entity_opt {
                    let prop_value = Self::extract_property(entity, property);
                    if prop_value.is_null() {
                        // Property not found - try to reload node if it has _nexus_id
                        if let Some(node_id) = Self::extract_entity_id(entity) {
                            // Check if it's a node (not a relationship) by checking if it doesn't have "type" property
                            if let Value::Object(obj) = entity {
                                if !obj.contains_key("type") {
                                    // It's a node - reload it to recover properties via reverse_index
                                    if let Ok(reloaded_node) = self.read_node_as_value(node_id) {
                                        // Use reloaded node for property access
                                        entity_opt = Some(reloaded_node);
                                    }
                                }
                            }
                        }
                    } else {
                    }
                }

                // phase9_external-node-ids §4.7 — `n._id` projects the
                // external id (in its prefixed string form) from the
                // catalog reverse map, NOT a regular property. Returns
                // Null when the node has no external id.
                if property == "_id" {
                    if let Some(ref entity) = entity_opt {
                        if let Some(node_id) = Self::extract_entity_id(entity) {
                            if let Ok(txn) = self.catalog().read_txn() {
                                if let Ok(Some(ext)) = self
                                    .catalog()
                                    .external_id_index()
                                    .get_external(&txn, node_id)
                                {
                                    return Ok(Value::String(ext.to_string()));
                                }
                            }
                        }
                    }
                    return Ok(Value::Null);
                }

                // Handle point accessor aliases (Neo4j compatibility)
                // point.latitude should return y, point.longitude should return x
                let actual_property = match property.as_str() {
                    "latitude" => {
                        // Check if this is a point object (has x, y, crs)
                        if let Some(Value::Object(ref obj)) = entity_opt {
                            if obj.contains_key("x")
                                && obj.contains_key("y")
                                && obj.contains_key("crs")
                            {
                                "y"
                            } else {
                                property
                            }
                        } else {
                            property
                        }
                    }
                    "longitude" => {
                        // Check if this is a point object (has x, y, crs)
                        if let Some(Value::Object(ref obj)) = entity_opt {
                            if obj.contains_key("x")
                                && obj.contains_key("y")
                                && obj.contains_key("crs")
                            {
                                "x"
                            } else {
                                property
                            }
                        } else {
                            property
                        }
                    }
                    // WGS-84 3D points expose the vertical coordinate as
                    // `height` in Neo4j; internally it is stored as `z`.
                    "height" => {
                        if let Some(Value::Object(ref obj)) = entity_opt {
                            if obj.contains_key("x")
                                && obj.contains_key("y")
                                && obj.contains_key("crs")
                            {
                                "z"
                            } else {
                                property
                            }
                        } else {
                            property
                        }
                    }
                    _ => property,
                };

                Ok(entity_opt
                    .as_ref()
                    .map(|e| Self::extract_property(e, actual_property))
                    .unwrap_or(Value::Null))
            }
            parser::Expression::ArrayIndex { base, index } => {
                // Evaluate the base expression (may be an array, a node, a
                // relationship, or a plain map).
                let base_value = self.evaluate_projection_expression(row, context, base)?;

                // phase6_opencypher-quickwins §5 — dynamic property access.
                // When the base is a graph-entity Object OR a plain user map,
                // `base[key]` is a property lookup (key must resolve to
                // STRING or NULL). When the base is a list, it's the
                // ordinary integer-indexed lookup. Lists take precedence
                // because they're the pre-existing behaviour and the
                // common case.
                if let Value::Array(_) = &base_value {
                    // fall through to the numeric-index path below
                } else {
                    let index_value = self.evaluate_projection_expression(row, context, index)?;
                    if matches!(base_value, Value::Null) || matches!(index_value, Value::Null) {
                        return Ok(Value::Null);
                    }
                    let key = match index_value {
                        Value::String(s) => s,
                        other => {
                            return Err(Error::CypherExecution(format!(
                                "ERR_INVALID_KEY: property key must be STRING (got {})",
                                super::type_name_of(&other)
                            )));
                        }
                    };
                    match base_value {
                        Value::Object(obj) => {
                            return Ok(obj.get(&key).cloned().unwrap_or(Value::Null));
                        }
                        _ => return Ok(Value::Null),
                    }
                }

                // Evaluate the index expression (should return an integer)
                let index_value = self.evaluate_projection_expression(row, context, index)?;

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
                let base_value = self.evaluate_projection_expression(row, context, base)?;

                match base_value {
                    Value::Array(arr) => {
                        let array_len = arr.len() as i64;

                        // Evaluate start index (default to 0)
                        let start_idx = if let Some(start_expr) = start {
                            let start_val =
                                self.evaluate_projection_expression(row, context, start_expr)?;
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
                            let end_val =
                                self.evaluate_projection_expression(row, context, end_expr)?;
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
                parser::Literal::Float(f) => Ok(serde_json::Number::from_f64(*f)
                    .map(Value::Number)
                    .unwrap_or(Value::Null)),
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
            parser::Expression::FunctionCall { name, args } => {
                let lowered = name.to_lowercase();

                // First, check if it's a registered UDF
                if let Some(udf) = self.shared.udf_registry.get(&lowered) {
                    let mut evaluated_args = Vec::new();
                    for arg_expr in args {
                        let arg_value =
                            self.evaluate_projection_expression(row, context, arg_expr)?;
                        evaluated_args.push(arg_value);
                    }
                    return udf
                        .execute(&evaluated_args)
                        .map_err(|e| Error::CypherSyntax(format!("UDF execution error: {}", e)));
                }

                // Dispatch to function-group helpers; each returns Option<Result<Value>>.
                // Try graph-entity functions first, then string, math, geo, temporal, list.
                if let Some(r) = self.eval_builtin_graph(row, context, lowered.as_str(), args) {
                    return r;
                }
                if let Some(r) = self.eval_builtin_string(row, context, lowered.as_str(), args) {
                    return r;
                }
                if let Some(r) = self.eval_builtin_math(row, context, lowered.as_str(), args) {
                    return r;
                }
                if let Some(r) = self.eval_builtin_geo(row, context, lowered.as_str(), args) {
                    return r;
                }
                if let Some(r) = self.eval_builtin_temporal(row, context, lowered.as_str(), args) {
                    return r;
                }
                if let Some(r) = self.eval_builtin_list(row, context, lowered.as_str(), args) {
                    return r;
                }

                Ok(Value::Null)
            }
            parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_projection_expression(row, context, left)?;
                let right_val = self.evaluate_projection_expression(row, context, right)?;
                match op {
                    parser::BinaryOperator::Add => self.add_values(&left_val, &right_val),
                    parser::BinaryOperator::Subtract => self.subtract_values(&left_val, &right_val),
                    parser::BinaryOperator::Multiply => self.multiply_values(&left_val, &right_val),
                    parser::BinaryOperator::Divide => self.divide_values(&left_val, &right_val),
                    parser::BinaryOperator::Modulo => self.modulo_values(&left_val, &right_val),
                    parser::BinaryOperator::Equal => {
                        // In Neo4j, null = null returns null (not true), and null = anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Bool(
                                self.values_equal_for_comparison(&left_val, &right_val),
                            ))
                        }
                    }
                    parser::BinaryOperator::NotEqual => {
                        // In Neo4j, null <> null returns null (not false), and null <> anything else returns null
                        if left_val.is_null() || right_val.is_null() {
                            Ok(Value::Null)
                        } else {
                            Ok(Value::Bool(left_val != right_val))
                        }
                    }
                    parser::BinaryOperator::LessThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Less,
                    )),
                    parser::BinaryOperator::LessThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::GreaterThan => Ok(Value::Bool(
                        self.compare_values_for_sort(&left_val, &right_val)
                            == std::cmp::Ordering::Greater,
                    )),
                    parser::BinaryOperator::GreaterThanOrEqual => Ok(Value::Bool(matches!(
                        self.compare_values_for_sort(&left_val, &right_val),
                        std::cmp::Ordering::Greater | std::cmp::Ordering::Equal
                    ))),
                    parser::BinaryOperator::And => {
                        let result =
                            self.value_to_bool(&left_val)? && self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(result))
                    }
                    parser::BinaryOperator::Or => {
                        let result =
                            self.value_to_bool(&left_val)? || self.value_to_bool(&right_val)?;
                        Ok(Value::Bool(result))
                    }
                    parser::BinaryOperator::StartsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.starts_with(&right_str)))
                    }
                    parser::BinaryOperator::EndsWith => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.ends_with(&right_str)))
                    }
                    parser::BinaryOperator::Contains => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        Ok(Value::Bool(left_str.contains(&right_str)))
                    }
                    parser::BinaryOperator::RegexMatch => {
                        let left_str = self.value_to_string(&left_val);
                        let right_str = self.value_to_string(&right_val);
                        // Use regex crate for pattern matching
                        match regex::Regex::new(&right_str) {
                            Ok(re) => Ok(Value::Bool(re.is_match(&left_str))),
                            Err(_) => Ok(Value::Bool(false)), // Invalid regex pattern returns false
                        }
                    }
                    parser::BinaryOperator::Power => {
                        // Power operator: left ^ right
                        self.power_values(&left_val, &right_val)
                    }
                    parser::BinaryOperator::In => {
                        // IN operator: left IN right (where right is a list)
                        // Check if left_val is in the right_val list
                        match &right_val {
                            Value::Array(list) => {
                                // Check if left_val is in the list
                                Ok(Value::Bool(list.iter().any(|item| item == &left_val)))
                            }
                            _ => {
                                // Right side is not a list, return false
                                Ok(Value::Bool(false))
                            }
                        }
                    }
                    _ => Ok(Value::Null),
                }
            }
            parser::Expression::UnaryOp { op, operand } => {
                let value = self.evaluate_projection_expression(row, context, operand)?;
                match op {
                    parser::UnaryOperator::Not => Ok(Value::Bool(!self.value_to_bool(&value)?)),
                    parser::UnaryOperator::Minus => {
                        let number = self.value_to_number(&value)?;
                        serde_json::Number::from_f64(-number)
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            })
                    }
                    parser::UnaryOperator::Plus => Ok(value),
                }
            }
            parser::Expression::IsNull { expr, negated } => {
                let value = self.evaluate_projection_expression(row, context, expr)?;
                let is_null = value.is_null();
                Ok(Value::Bool(if *negated { !is_null } else { is_null }))
            }
            parser::Expression::Exists {
                pattern,
                where_clause,
            } => {
                // Check if the pattern exists in the current context
                let pattern_exists = self.check_pattern_exists(row, context, pattern)?;

                // If pattern doesn't exist, return false
                if !pattern_exists {
                    return Ok(Value::Bool(false));
                }

                // If WHERE clause is present, evaluate it
                if let Some(where_expr) = where_clause {
                    // Create a context with pattern variables for WHERE evaluation
                    let mut exists_row = row.clone();

                    // Extract variables from pattern and add to row context
                    for element in &pattern.elements {
                        match element {
                            parser::PatternElement::Node(node) => {
                                if let Some(var) = &node.variable {
                                    // Try to get variable from current row or context
                                    if let Some(value) = row.get(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    } else if let Some(value) = context.get_variable(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    }
                                }
                            }
                            parser::PatternElement::Relationship(rel) => {
                                if let Some(var) = &rel.variable {
                                    if let Some(value) = row.get(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    } else if let Some(value) = context.get_variable(var) {
                                        exists_row.insert(var.clone(), value.clone());
                                    }
                                }
                            }
                            parser::PatternElement::QuantifiedGroup(_) => {
                                return Err(Error::CypherExecution(
                                    "ERR_QPP_NOT_IMPLEMENTED: quantified path \
                                     patterns inside EXISTS subqueries need the \
                                     QPP operator (tracked as follow-up task)"
                                        .to_string(),
                                ));
                            }
                        }
                    }

                    // Evaluate WHERE condition
                    let condition_value =
                        self.evaluate_projection_expression(&exists_row, context, where_expr)?;
                    let condition_true = self.value_to_bool(&condition_value)?;

                    Ok(Value::Bool(condition_true))
                } else {
                    Ok(Value::Bool(pattern_exists))
                }
            }
            parser::Expression::CollectSubquery { inner } => {
                self.evaluate_collect_subquery(row, context, inner)
            }
            parser::Expression::MapProjection { source, items } => {
                // Evaluate the source expression (should be a node/map)
                let source_value = self.evaluate_projection_expression(row, context, source)?;

                // Build the projected map
                let mut projected_map = serde_json::Map::new();

                for item in items {
                    match item {
                        parser::MapProjectionItem::Property { property, alias } => {
                            // Extract property from source
                            let prop_value = if let Value::Object(obj) = &source_value {
                                // If source is a node object, get property from properties
                                if let Some(Value::Object(props)) = obj.get("properties") {
                                    props.get(property.as_str()).cloned().unwrap_or(Value::Null)
                                } else {
                                    obj.get(property.as_str()).cloned().unwrap_or(Value::Null)
                                }
                            } else {
                                Value::Null
                            };

                            // Use alias if provided, otherwise use property name
                            let key = alias
                                .as_ref()
                                .map(|s| s.as_str())
                                .unwrap_or(property.as_str())
                                .to_string();
                            projected_map.insert(key, prop_value);
                        }
                        parser::MapProjectionItem::VirtualKey { key, expression } => {
                            // Evaluate the expression and use as value
                            let expr_value =
                                self.evaluate_projection_expression(row, context, expression)?;
                            projected_map.insert(key.clone(), expr_value);
                        }
                    }
                }

                Ok(Value::Object(projected_map))
            }
            parser::Expression::ListComprehension {
                variable,
                list_expression,
                where_clause,
                transform_expression,
            } => {
                // Evaluate the list expression
                let list_value =
                    self.evaluate_projection_expression(row, context, list_expression)?;

                // Convert to array if needed
                let list_items = match list_value {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(),
                    other => vec![other],
                };

                // Filter and transform items
                let mut result_items = Vec::new();

                for item in list_items {
                    // Create a new row context with the variable bound to this item
                    let mut comprehension_row = row.clone();
                    let item_clone = item.clone();
                    comprehension_row.insert(variable.clone(), item_clone);

                    // Apply WHERE clause if present
                    if let Some(where_expr) = where_clause {
                        let condition_value = self.evaluate_projection_expression(
                            &comprehension_row,
                            context,
                            where_expr,
                        )?;

                        // Only include item if condition is true
                        if !self.value_to_bool(&condition_value)? {
                            continue;
                        }
                    }

                    // Apply transformation if present, otherwise use item as-is
                    if let Some(transform_expr) = transform_expression {
                        let transformed_value = self.evaluate_projection_expression(
                            &comprehension_row,
                            context,
                            transform_expr,
                        )?;
                        result_items.push(transformed_value);
                    } else {
                        result_items.push(item);
                    }
                }

                Ok(Value::Array(result_items))
            }
            parser::Expression::PatternComprehension {
                pattern,
                where_clause,
                transform_expression,
            } => {
                // Pattern comprehensions collect matching patterns and transform them
                // This is a simplified implementation that works within the current context

                // For a full implementation, we would need to:
                // 1. Execute the pattern as a subquery within the current context
                // 2. Collect all matching results
                // 3. Apply WHERE clause filtering
                // 4. Apply transformation expression
                // 5. Return as array

                // For now, we'll implement a basic version that:
                // - Extracts variables from the pattern
                // - Checks if they exist in the current row context
                // - Applies WHERE and transform if present

                // Extract variables from pattern
                let mut pattern_vars = Vec::new();
                for element in &pattern.elements {
                    match element {
                        parser::PatternElement::Node(node) => {
                            if let Some(var) = &node.variable {
                                pattern_vars.push(var.clone());
                            }
                        }
                        parser::PatternElement::Relationship(rel) => {
                            if let Some(var) = &rel.variable {
                                pattern_vars.push(var.clone());
                            }
                        }
                        parser::PatternElement::QuantifiedGroup(_) => {
                            return Err(Error::CypherExecution(
                                "ERR_QPP_NOT_IMPLEMENTED: quantified path \
                                 patterns inside COLLECT subqueries need the \
                                 QPP operator (tracked as follow-up task)"
                                    .to_string(),
                            ));
                        }
                    }
                }

                // Check if all pattern variables exist in current row
                let mut all_vars_exist = true;
                let mut pattern_row = HashMap::new();
                for var in &pattern_vars {
                    if let Some(value) = row.get(var) {
                        pattern_row.insert(var.clone(), value.clone());
                    } else {
                        all_vars_exist = false;
                        break;
                    }
                }

                // If pattern variables don't exist in current row, return empty array
                if !all_vars_exist || pattern_row.is_empty() {
                    return Ok(Value::Array(Vec::new()));
                }

                // Apply WHERE clause if present
                if let Some(where_expr) = where_clause {
                    let condition_value =
                        self.evaluate_projection_expression(&pattern_row, context, where_expr)?;

                    // If WHERE condition is false, return empty array
                    if !self.value_to_bool(&condition_value)? {
                        return Ok(Value::Array(Vec::new()));
                    }
                }

                // Apply transformation if present, otherwise return the pattern variables
                if let Some(transform_expr) = transform_expression {
                    // Evaluate transformation expression (can be MapProjection, property access, etc.)
                    let transformed_value =
                        self.evaluate_projection_expression(&pattern_row, context, transform_expr)?;

                    // Always return as array (even if single value)
                    Ok(Value::Array(vec![transformed_value]))
                } else {
                    // No transformation - return array of pattern variable values
                    let values: Vec<Value> = pattern_vars
                        .iter()
                        .filter_map(|var| pattern_row.get(var).cloned())
                        .collect();
                    Ok(Value::Array(values))
                }
            }
            parser::Expression::List(elements) => {
                // Evaluate each element and return as JSON array
                let mut items = Vec::new();
                for element in elements {
                    let value = self.evaluate_projection_expression(row, context, element)?;
                    items.push(value);
                }
                Ok(Value::Array(items))
            }
            parser::Expression::Map(map) => {
                // Evaluate each value and return as JSON object
                let mut obj = serde_json::Map::new();
                for (key, expr) in map {
                    let value = self.evaluate_projection_expression(row, context, expr)?;
                    obj.insert(key.clone(), value);
                }
                Ok(Value::Object(obj))
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Evaluate input expression if present (generic CASE)
                let input_value = if let Some(input_expr) = input {
                    Some(self.evaluate_projection_expression(row, context, input_expr)?)
                } else {
                    None
                };

                // Evaluate WHEN clauses
                for when_clause in when_clauses {
                    let condition_value =
                        self.evaluate_projection_expression(row, context, &when_clause.condition)?;

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
                        return self.evaluate_projection_expression(
                            row,
                            context,
                            &when_clause.result,
                        );
                    }
                }

                // No WHEN clause matched, return ELSE or NULL
                if let Some(else_expr) = else_clause {
                    self.evaluate_projection_expression(row, context, else_expr)
                } else {
                    Ok(Value::Null)
                }
            }
        }
    }

    /// Evaluate a `COLLECT { … }` subquery expression
    /// (phase6_opencypher-subquery-transactions §9).
    ///
    /// Plans the inner AST against the current executor, runs every
    /// operator in turn against an inner [`ExecutionContext`] seeded
    /// with the outer row's bindings, and folds the inner row stream
    /// into a single LIST value:
    ///
    /// - single-column inner → `Value::Array` of that column's values,
    /// - multi-column inner  → `Value::Array` of `Value::Object` maps
    ///   keyed by the column names,
    /// - empty inner row stream → `Value::Array(vec![])` (NOT NULL,
    ///   per the §9 spec).
    pub(in crate::executor) fn evaluate_collect_subquery(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        inner: &parser::CypherQuery,
    ) -> Result<Value> {
        // Plan the inner subquery once. The AST is owned by the
        // expression so it doesn't change between successive
        // evaluations on different outer rows; a single plan is
        // correct.
        let inner_operators = self.plan_ast(inner)?;

        // Build a fresh inner ExecutionContext that inherits the
        // outer's params + cache + plan hints, plus the variable
        // bindings projected from the current outer row.
        let mut inner_ctx = ExecutionContext::new(
            self.collect_subquery_params_for(context),
            self.collect_subquery_cache_for(context),
        );
        inner_ctx.set_plan_hints(self.collect_subquery_hints_for(context));
        for (k, v) in row.iter() {
            inner_ctx.set_variable(k, v.clone());
        }
        // Also import context-level variables that aren't already
        // shadowed by the row's projection — Cypher's COLLECT { … }
        // sees the entire outer scope (the nested-scope tree fix is
        // tracked under the slice-4 task entry).
        for (k, v) in self.collect_subquery_outer_vars(context) {
            if !row.contains_key(&k) {
                inner_ctx.set_variable(&k, v);
            }
        }

        for op in &inner_operators {
            self.execute_operator(&mut inner_ctx, op)?;
        }

        let cols = inner_ctx.result_set.columns.clone();
        let rows = std::mem::take(&mut inner_ctx.result_set.rows);

        // Empty result → empty list (NOT NULL).
        if rows.is_empty() {
            return Ok(Value::Array(Vec::new()));
        }

        let mut out: Vec<Value> = Vec::with_capacity(rows.len());
        if cols.len() == 1 {
            // Single column → flat list of scalars.
            for r in rows {
                out.push(r.values.into_iter().next().unwrap_or(Value::Null));
            }
        } else {
            // Multi-column → list of maps keyed by column name.
            for r in rows {
                let mut m = Map::with_capacity(cols.len());
                for (idx, col) in cols.iter().enumerate() {
                    let v = r.values.get(idx).cloned().unwrap_or(Value::Null);
                    m.insert(col.clone(), v);
                }
                out.push(Value::Object(m));
            }
        }
        Ok(Value::Array(out))
    }

    /// Borrow the outer params for a COLLECT { … } inner subquery.
    fn collect_subquery_params_for(&self, context: &ExecutionContext) -> HashMap<String, Value> {
        context.params_clone()
    }

    /// Borrow the outer cache handle for a COLLECT { … } inner subquery.
    fn collect_subquery_cache_for(
        &self,
        context: &ExecutionContext,
    ) -> Option<std::sync::Arc<parking_lot::RwLock<crate::cache::MultiLayerCache>>> {
        context.cache_clone()
    }

    /// Borrow the outer plan hints for a COLLECT { … } inner subquery.
    fn collect_subquery_hints_for(
        &self,
        context: &ExecutionContext,
    ) -> Vec<super::super::super::planner::PlanHint> {
        context.plan_hints_clone()
    }

    /// Snapshot of the outer scope's variable bindings.
    fn collect_subquery_outer_vars(&self, context: &ExecutionContext) -> Vec<(String, Value)> {
        context.variables_clone_pairs()
    }
}
