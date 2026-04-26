//! `Executor::evaluate_projection_expression` — the monolithic expression
//! evaluator used by `Project`, `With`, `Aggregate`, and `Filter`. Supports
//! literals, variable lookup, property access, arithmetic, string/list/map
//! operations, case expressions, pattern-exists checks, collection
//! comprehensions, and dozens of built-in functions.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::Direction;
use crate::{Error, Result};
use chrono::{Datelike, TimeZone, Timelike};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// openCypher-ish type name used in error messages from type-check and
/// list-coercion builtins. Keeps the error surface aligned with the
/// openCypher spec's `INTEGER`, `FLOAT`, `STRING`, etc.
fn type_name_of(v: &Value) -> &'static str {
    match v {
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

impl Executor {
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
                                type_name_of(&other)
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
                Ok(context.params.get(name).cloned().unwrap_or(Value::Null))
            }
            parser::Expression::FunctionCall { name, args } => {
                let lowered = name.to_lowercase();

                // First, check if it's a registered UDF
                if let Some(udf) = self.shared.udf_registry.get(&lowered) {
                    // Evaluate arguments
                    let mut evaluated_args = Vec::new();
                    for arg_expr in args {
                        let arg_value =
                            self.evaluate_projection_expression(row, context, arg_expr)?;
                        evaluated_args.push(arg_value);
                    }

                    // Execute UDF
                    return udf
                        .execute(&evaluated_args)
                        .map_err(|e| Error::CypherSyntax(format!("UDF execution error: {}", e)));
                }

                // If not a UDF, check built-in functions
                match lowered.as_str() {
                    // phase6_opencypher-quickwins §8 — runtime evaluator
                    // for the synthetic `__label_predicate__(var, 'Label')`
                    // the parser emits for `var:Label` in WHERE /
                    // RETURN position. The Filter operator's text-mode
                    // fast path only handles a single `var:Label`
                    // standalone predicate (len==2 after splitting on
                    // ':'); compound forms like
                    // `n:Employee OR n:Manager` fall through to the
                    // generic expression evaluator, which needs this
                    // arm to return BOOLEAN instead of NULL and drop
                    // every row in the process.
                    "__label_predicate__" => {
                        if args.len() == 2 {
                            let node_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let label_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            // Resolve the label argument to a String.
                            // The parser emits the label as a STRING
                            // literal; dynamic `$param` references have
                            // already been rewritten through a Parameter
                            // node that the evaluator dereferences.
                            let label_name = match label_val {
                                Value::String(s) => s,
                                Value::Null => return Ok(Value::Bool(false)),
                                other => {
                                    return Err(Error::TypeMismatch {
                                        expected: "STRING label".to_string(),
                                        actual: format!("{other:?}"),
                                    });
                                }
                            };
                            if label_name.is_empty() {
                                return Ok(Value::Bool(false));
                            }
                            let node_id = match &node_val {
                                Value::Object(obj) => obj.get("_nexus_id").and_then(|v| v.as_u64()),
                                _ => None,
                            };
                            let Some(nid) = node_id else {
                                return Ok(Value::Bool(false));
                            };
                            let Ok(label_id) = self.catalog().get_label_id(&label_name) else {
                                return Ok(Value::Bool(false));
                            };
                            let Ok(node_record) = self.store().read_node(nid) else {
                                return Ok(Value::Bool(false));
                            };
                            // Labels with id >= 64 do not fit the
                            // per-node label bitmap; they are indexed
                            // elsewhere and the current executor has no
                            // path to check them here. Mirrors the
                            // same limitation `filter.rs` documents.
                            let present = if label_id < 64 {
                                (node_record.label_bits & (1u64 << label_id)) != 0
                            } else {
                                false
                            };
                            return Ok(Value::Bool(present));
                        }
                        Ok(Value::Null)
                    }
                    "labels" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract node ID from the value
                            let node_id = if let Value::Object(obj) = &value {
                                // Try to get _nexus_id from the object
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    id.as_u64()
                                } else {
                                    None
                                }
                            } else if let Value::String(id_str) = &value {
                                // Try to parse as string ID
                                id_str.parse::<u64>().ok()
                            } else {
                                None
                            };

                            if let Some(nid) = node_id {
                                // Read the node record to get labels
                                if let Ok(node_record) = self.store().read_node(nid) {
                                    if let Ok(label_names) = self
                                        .catalog()
                                        .get_labels_from_bitmap(node_record.label_bits)
                                    {
                                        let labels: Vec<Value> =
                                            label_names.into_iter().map(Value::String).collect();
                                        return Ok(Value::Array(labels));
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "type" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract relationship ID from the value
                            let rel_id = if let Value::Object(obj) = &value {
                                // Try to get _nexus_id from the object
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    id.as_u64()
                                } else {
                                    None
                                }
                            } else if let Value::String(id_str) = &value {
                                // Try to parse as string ID
                                id_str.parse::<u64>().ok()
                            } else {
                                None
                            };

                            if let Some(rid) = rel_id {
                                // Read the relationship record to get type_id
                                if let Ok(rel_record) = self.store().read_rel(rid) {
                                    if let Ok(Some(type_name)) =
                                        self.catalog().get_type_name(rel_record.type_id)
                                    {
                                        return Ok(Value::String(type_name));
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "keys" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract keys from the value (node or relationship)
                            if let Value::Object(obj) = &value {
                                let mut keys: Vec<String> = obj
                                    .keys()
                                    .filter(|k| {
                                        // Exclude internal fields:
                                        // - Fields starting with _ (like _nexus_id, _nexus_type)
                                        // - "type" field (internal relationship type)
                                        !k.starts_with('_') && *k != "type"
                                    })
                                    .map(|k| k.to_string())
                                    .collect();
                                keys.sort();
                                let key_values: Vec<Value> =
                                    keys.into_iter().map(Value::String).collect();
                                return Ok(Value::Array(key_values));
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "id" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // Extract node or relationship ID from _nexus_id
                            if let Value::Object(obj) = &value {
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    return Ok(Value::Number(id.clone()));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Database functions
                    "database" => {
                        // Return current database name
                        // If DatabaseManager is available, get current database
                        // Otherwise return default "neo4j"
                        if let Some(db_manager_arc) = self.shared.database_manager() {
                            let db_manager = db_manager_arc.read();
                            Ok(Value::String(
                                db_manager.default_database_name().to_string(),
                            ))
                        } else {
                            Ok(Value::String("neo4j".to_string()))
                        }
                    }
                    "db" => {
                        // Alias for database()
                        if let Some(db_manager_arc) = self.shared.database_manager() {
                            let db_manager = db_manager_arc.read();
                            Ok(Value::String(
                                db_manager.default_database_name().to_string(),
                            ))
                        } else {
                            Ok(Value::String("neo4j".to_string()))
                        }
                    }
                    // String functions
                    "tolower" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.to_lowercase()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "toupper" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.to_uppercase()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "substring" => {
                        // substring(string, start, [length])
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let start_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::Number(start_num)) =
                                (string_val, start_val)
                            {
                                let char_len = s.chars().count() as i64;
                                // Handle both integer and float numbers (floats come from unary minus)
                                let start_i64 = start_num
                                    .as_i64()
                                    .or_else(|| start_num.as_f64().map(|f| f as i64))
                                    .unwrap_or(0);

                                // Handle negative indices (count from end)
                                let start = if start_i64 < 0 {
                                    ((char_len + start_i64).max(0)) as usize
                                } else {
                                    start_i64.min(char_len) as usize
                                };

                                if args.len() >= 3 {
                                    let length_val = self
                                        .evaluate_projection_expression(row, context, &args[2])?;
                                    if let Value::Number(len_num) = length_val {
                                        let length = len_num.as_i64().unwrap_or(0).max(0) as usize;
                                        let chars: Vec<char> = s.chars().collect();
                                        let end = (start + length).min(chars.len());
                                        return Ok(Value::String(
                                            chars[start..end].iter().collect(),
                                        ));
                                    }
                                } else {
                                    // No length specified - take from start to end
                                    let chars: Vec<char> = s.chars().collect();
                                    return Ok(Value::String(chars[start..].iter().collect()));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "trim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "ltrim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim_start().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "rtrim" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::String(s) = value {
                                return Ok(Value::String(s.trim_end().to_string()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "replace" => {
                        // replace(string, search, replace)
                        if args.len() >= 3 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let search_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            let replace_val =
                                self.evaluate_projection_expression(row, context, &args[2])?;

                            if let (
                                Value::String(s),
                                Value::String(search),
                                Value::String(replace),
                            ) = (string_val, search_val, replace_val)
                            {
                                return Ok(Value::String(s.replace(&search, &replace)));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "split" => {
                        // split(string, delimiter)
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let delim_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(delim)) =
                                (string_val, delim_val)
                            {
                                let parts: Vec<Value> = s
                                    .split(&delim)
                                    .map(|part| Value::String(part.to_string()))
                                    .collect();
                                return Ok(Value::Array(parts));
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Regex functions
                    "regexmatch" => {
                        // regexMatch(string, pattern) - returns true if pattern matches string
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(pattern)) =
                                (string_val, pattern_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => return Ok(Value::Bool(re.is_match(&s))),
                                    Err(_) => return Ok(Value::Bool(false)),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexreplace" => {
                        // regexReplace(string, pattern, replacement) - replaces first match
                        if args.len() >= 3 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            let replacement_val =
                                self.evaluate_projection_expression(row, context, &args[2])?;

                            if let (
                                Value::String(s),
                                Value::String(pattern),
                                Value::String(replacement),
                            ) = (string_val, pattern_val, replacement_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        // Replace only the first match
                                        let result = re.replace(&s, replacement.as_str());
                                        return Ok(Value::String(result.into_owned()));
                                    }
                                    Err(_) => return Ok(Value::String(s)),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexreplaceall" => {
                        // regexReplaceAll(string, pattern, replacement) - replaces all matches
                        if args.len() >= 3 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            let replacement_val =
                                self.evaluate_projection_expression(row, context, &args[2])?;

                            if let (
                                Value::String(s),
                                Value::String(pattern),
                                Value::String(replacement),
                            ) = (string_val, pattern_val, replacement_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        // Replace all matches
                                        let result = re.replace_all(&s, replacement.as_str());
                                        return Ok(Value::String(result.into_owned()));
                                    }
                                    Err(_) => return Ok(Value::String(s)),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexextract" => {
                        // regexExtract(string, pattern) - extracts first match
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(pattern)) =
                                (string_val, pattern_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        if let Some(m) = re.find(&s) {
                                            return Ok(Value::String(m.as_str().to_string()));
                                        }
                                        return Ok(Value::Null);
                                    }
                                    Err(_) => return Ok(Value::Null),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexextractall" => {
                        // regexExtractAll(string, pattern) - extracts all matches as array
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(pattern)) =
                                (string_val, pattern_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        let matches: Vec<Value> = re
                                            .find_iter(&s)
                                            .map(|m| Value::String(m.as_str().to_string()))
                                            .collect();
                                        return Ok(Value::Array(matches));
                                    }
                                    Err(_) => return Ok(Value::Array(vec![])),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexextractgroups" => {
                        // regexExtractGroups(string, pattern) - extracts capture groups from first match
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(pattern)) =
                                (string_val, pattern_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        if let Some(caps) = re.captures(&s) {
                                            let groups: Vec<Value> = caps
                                                .iter()
                                                .skip(1) // Skip the full match (group 0)
                                                .map(|m| {
                                                    m.map(|m| Value::String(m.as_str().to_string()))
                                                        .unwrap_or(Value::Null)
                                                })
                                                .collect();
                                            return Ok(Value::Array(groups));
                                        }
                                        return Ok(Value::Null);
                                    }
                                    Err(_) => return Ok(Value::Null),
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "regexsplit" => {
                        // regexSplit(string, pattern) - splits string by regex pattern
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let pattern_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::String(pattern)) =
                                (string_val, pattern_val)
                            {
                                match regex::Regex::new(&pattern) {
                                    Ok(re) => {
                                        let parts: Vec<Value> = re
                                            .split(&s)
                                            .map(|part| Value::String(part.to_string()))
                                            .collect();
                                        return Ok(Value::Array(parts));
                                    }
                                    Err(_) => {
                                        // Fallback to returning original string in array
                                        return Ok(Value::Array(vec![Value::String(s)]));
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Math functions
                    "abs" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.abs())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "ceil" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.ceil())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "floor" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.floor())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "round" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.round())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "sqrt" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.sqrt())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "pow" => {
                        // pow(base, exponent)
                        if args.len() >= 2 {
                            let base_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let exp_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            if base_val.is_null() || exp_val.is_null() {
                                return Ok(Value::Null);
                            }
                            let base = self.value_to_number(&base_val)?;
                            let exp = self.value_to_number(&exp_val)?;
                            return serde_json::Number::from_f64(base.powf(exp))
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "sin" => {
                        // sin(angle) - sine function (angle in radians)
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.sin())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "cos" => {
                        // cos(angle) - cosine function (angle in radians)
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.cos())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "tan" => {
                        // tan(angle) - tangent function (angle in radians)
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.tan())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    // Geospatial predicate functions
                    // (phase6_opencypher-geospatial-predicates §4).
                    // Each predicate extracts up to two points via
                    // `Point::from_json_value` and reuses the
                    // existing distance / CRS helpers. CRS or
                    // dimensionality mismatches surface as a
                    // CypherSyntax error with the `ERR_CRS_MISMATCH`
                    // prefix so SDK assertions can pattern-match on
                    // the error code.
                    "point.withinbbox" => {
                        if args.len() < 2 {
                            return Ok(Value::Null);
                        }
                        let p_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let bbox_val =
                            self.evaluate_projection_expression(row, context, &args[1])?;
                        if p_val.is_null() || bbox_val.is_null() {
                            return Ok(Value::Null);
                        }
                        let Value::Object(_) = &p_val else {
                            return Ok(Value::Null);
                        };
                        let p = crate::geospatial::Point::from_json_value(&p_val)
                            .map_err(|e| Error::CypherSyntax(format!("Invalid point: {e}")))?;
                        let bbox_obj = bbox_val.as_object().ok_or_else(|| {
                            Error::CypherSyntax(
                                "ERR_BBOX_MALFORMED: bbox must be a map".to_string(),
                            )
                        })?;
                        let bl_v = bbox_obj.get("bottomLeft").ok_or_else(|| {
                            Error::CypherSyntax(
                                "ERR_BBOX_MALFORMED: missing 'bottomLeft'".to_string(),
                            )
                        })?;
                        let tr_v = bbox_obj.get("topRight").ok_or_else(|| {
                            Error::CypherSyntax(
                                "ERR_BBOX_MALFORMED: missing 'topRight'".to_string(),
                            )
                        })?;
                        let bl = crate::geospatial::Point::from_json_value(bl_v).map_err(|e| {
                            Error::CypherSyntax(format!("ERR_BBOX_MALFORMED: bottomLeft: {e}"))
                        })?;
                        let tr = crate::geospatial::Point::from_json_value(tr_v).map_err(|e| {
                            Error::CypherSyntax(format!("ERR_BBOX_MALFORMED: topRight: {e}"))
                        })?;
                        if !p.same_crs(&bl) || !p.same_crs(&tr) {
                            return Err(Error::CypherSyntax(format!(
                                "ERR_CRS_MISMATCH: point={}, bbox=({}, {})",
                                p.crs_name(),
                                bl.crs_name(),
                                tr.crs_name()
                            )));
                        }
                        Ok(Value::Bool(p.within_bbox(&bl, &tr)))
                    }
                    "point.withindistance" => {
                        if args.len() < 3 {
                            return Ok(Value::Null);
                        }
                        let a_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let b_val = self.evaluate_projection_expression(row, context, &args[1])?;
                        let d_val = self.evaluate_projection_expression(row, context, &args[2])?;
                        if a_val.is_null() || b_val.is_null() || d_val.is_null() {
                            return Ok(Value::Null);
                        }
                        let Value::Object(_) = &a_val else {
                            return Ok(Value::Null);
                        };
                        let Value::Object(_) = &b_val else {
                            return Ok(Value::Null);
                        };
                        let a = crate::geospatial::Point::from_json_value(&a_val)
                            .map_err(|e| Error::CypherSyntax(format!("Invalid point a: {e}")))?;
                        let b = crate::geospatial::Point::from_json_value(&b_val)
                            .map_err(|e| Error::CypherSyntax(format!("Invalid point b: {e}")))?;
                        if !a.same_crs(&b) {
                            return Err(Error::CypherSyntax(format!(
                                "ERR_CRS_MISMATCH: a={}, b={}",
                                a.crs_name(),
                                b.crs_name()
                            )));
                        }
                        let dist = self.value_to_number(&d_val)?;
                        Ok(Value::Bool(a.distance_to(&b) <= dist))
                    }
                    "point.azimuth" => {
                        if args.len() < 2 {
                            return Ok(Value::Null);
                        }
                        let a_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let b_val = self.evaluate_projection_expression(row, context, &args[1])?;
                        if a_val.is_null() || b_val.is_null() {
                            return Ok(Value::Null);
                        }
                        let Value::Object(_) = &a_val else {
                            return Ok(Value::Null);
                        };
                        let Value::Object(_) = &b_val else {
                            return Ok(Value::Null);
                        };
                        let a = crate::geospatial::Point::from_json_value(&a_val)
                            .map_err(|e| Error::CypherSyntax(format!("Invalid point a: {e}")))?;
                        let b = crate::geospatial::Point::from_json_value(&b_val)
                            .map_err(|e| Error::CypherSyntax(format!("Invalid point b: {e}")))?;
                        if !a.same_crs(&b) {
                            return Err(Error::CypherSyntax(format!(
                                "ERR_CRS_MISMATCH: a={}, b={}",
                                a.crs_name(),
                                b.crs_name()
                            )));
                        }
                        match a.azimuth_to(&b) {
                            Some(deg) => serde_json::Number::from_f64(deg)
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                }),
                            None => Ok(Value::Null),
                        }
                    }
                    "point.distance" => {
                        // point.distance as a namespaced alias for
                        // the bare `distance()` function. Keeps
                        // parity with Neo4j's newer surface where
                        // the `point.*` namespace is the idiomatic
                        // one.
                        if args.len() < 2 {
                            return Ok(Value::Null);
                        }
                        let a_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let b_val = self.evaluate_projection_expression(row, context, &args[1])?;
                        if a_val.is_null() || b_val.is_null() {
                            return Ok(Value::Null);
                        }
                        let a = if let Value::Object(_) = &a_val {
                            crate::geospatial::Point::from_json_value(&a_val)
                                .map_err(|e| Error::CypherSyntax(format!("Invalid point a: {e}")))?
                        } else {
                            return Ok(Value::Null);
                        };
                        let b = if let Value::Object(_) = &b_val {
                            crate::geospatial::Point::from_json_value(&b_val)
                                .map_err(|e| Error::CypherSyntax(format!("Invalid point b: {e}")))?
                        } else {
                            return Ok(Value::Null);
                        };
                        if !a.same_crs(&b) {
                            return Err(Error::CypherSyntax(format!(
                                "ERR_CRS_MISMATCH: a={}, b={}",
                                a.crs_name(),
                                b.crs_name()
                            )));
                        }
                        serde_json::Number::from_f64(a.distance_to(&b))
                            .map(Value::Number)
                            .ok_or_else(|| Error::TypeMismatch {
                                expected: "number".to_string(),
                                actual: "non-finite".to_string(),
                            })
                    }
                    // Geospatial functions
                    "distance" => {
                        // distance(point1, point2) - calculate distance between two points
                        if args.len() >= 2 {
                            let p1_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let p2_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            // Try to parse points from JSON values
                            // Points can be:
                            // 1. Point literals (already converted to JSON objects via to_json_value)
                            // 2. JSON objects with x/y/z/crs fields
                            let p1 = if let Value::Object(_) = &p1_val {
                                crate::geospatial::Point::from_json_value(&p1_val).map_err(
                                    |_| Error::CypherSyntax("Invalid point 1".to_string()),
                                )?
                            } else {
                                return Ok(Value::Null);
                            };

                            let p2 = if let Value::Object(_) = &p2_val {
                                crate::geospatial::Point::from_json_value(&p2_val).map_err(
                                    |_| Error::CypherSyntax("Invalid point 2".to_string()),
                                )?
                            } else {
                                return Ok(Value::Null);
                            };

                            let distance = p1.distance_to(&p2);
                            return serde_json::Number::from_f64(distance)
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    // Type conversion functions
                    "tointeger" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    if let Some(i) = n.as_i64() {
                                        return Ok(Value::Number(i.into()));
                                    }
                                    if let Some(f) = n.as_f64() {
                                        return Ok(Value::Number((f as i64).into()));
                                    }
                                }
                                Value::String(s) => {
                                    if let Ok(i) = s.parse::<i64>() {
                                        return Ok(Value::Number(i.into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tofloat" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    if let Some(f) = n.as_f64() {
                                        return serde_json::Number::from_f64(f)
                                            .map(Value::Number)
                                            .ok_or_else(|| Error::TypeMismatch {
                                                expected: "float".to_string(),
                                                actual: "non-finite".to_string(),
                                            });
                                    }
                                }
                                Value::String(s) => {
                                    if let Ok(f) = s.parse::<f64>() {
                                        return serde_json::Number::from_f64(f)
                                            .map(Value::Number)
                                            .ok_or_else(|| Error::TypeMismatch {
                                                expected: "float".to_string(),
                                                actual: "non-finite".to_string(),
                                            });
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tostring" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => Ok(Value::String(s)),
                                Value::Number(n) => Ok(Value::String(n.to_string())),
                                Value::Bool(b) => Ok(Value::String(b.to_string())),
                                Value::Null => Ok(Value::Null),
                                Value::Array(_) | Value::Object(_) => {
                                    Ok(Value::String(value.to_string()))
                                }
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "toboolean" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Bool(b) => Ok(Value::Bool(b)),
                                Value::String(s) => {
                                    let lower = s.to_lowercase();
                                    if lower == "true" {
                                        Ok(Value::Bool(true))
                                    } else if lower == "false" {
                                        Ok(Value::Bool(false))
                                    } else {
                                        Ok(Value::Null)
                                    }
                                }
                                Value::Number(n) => {
                                    // 0 = false, non-zero = true
                                    Ok(Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0))
                                }
                                _ => Ok(Value::Null),
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    // phase6_opencypher-quickwins §1 — type-check predicates.
                    // Each returns NULL on NULL input (three-valued logic) and
                    // BOOLEAN otherwise. Nodes vs relationships are
                    // disambiguated by the presence of a "type" key on the
                    // serialised Object form (relationships carry their
                    // relationship-type there; nodes do not).
                    "isinteger" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Number(n) => Ok(Value::Bool(n.is_i64() || n.is_u64())),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "isfloat" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Number(n) => Ok(Value::Bool(n.is_f64())),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "isstring" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::String(_) => Ok(Value::Bool(true)),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "isboolean" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Bool(_) => Ok(Value::Bool(true)),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "islist" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(_) => Ok(Value::Bool(true)),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "ismap" => {
                        // A MAP is any Object that ISN'T one of Nexus's
                        // serialised graph entities (node/relationship carry
                        // `_nexus_id`). Plain user maps are Object values
                        // without `_nexus_id`.
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Object(obj) => Ok(Value::Bool(!obj.contains_key("_nexus_id"))),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "isnode" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Object(obj) => Ok(Value::Bool(
                                obj.contains_key("_nexus_id") && !obj.contains_key("type"),
                            )),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "isrelationship" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Object(obj) => Ok(Value::Bool(
                                obj.contains_key("_nexus_id") && obj.contains_key("type"),
                            )),
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    "ispath" => {
                        // Paths currently surface through the executor as
                        // Arrays of alternating node/relationship Objects.
                        // Narrow predicate: non-empty Array whose elements
                        // are all `_nexus_id`-tagged Objects. Scalars and
                        // plain lists return false.
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(items) if !items.is_empty() => {
                                let is_path = items.iter().all(|el| {
                                    matches!(el, Value::Object(o) if o.contains_key("_nexus_id"))
                                });
                                Ok(Value::Bool(is_path))
                            }
                            _ => Ok(Value::Bool(false)),
                        }
                    }
                    // phase6_opencypher-quickwins §2 — list type-converter
                    // functions. Per-element coercion: elements that fail to
                    // convert become NULL rather than erroring the query.
                    // A NULL input list returns NULL (not an empty list).
                    // A non-LIST input raises TypeMismatch.
                    "tointegerlist" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(items) => {
                                let out: Vec<Value> = items
                                    .into_iter()
                                    .map(|el| match el {
                                        Value::Null => Value::Null,
                                        Value::Number(n) => n
                                            .as_i64()
                                            .or_else(|| n.as_f64().map(|f| f as i64))
                                            .map(|i| Value::Number(i.into()))
                                            .unwrap_or(Value::Null),
                                        Value::Bool(b) => {
                                            Value::Number((if b { 1i64 } else { 0 }).into())
                                        }
                                        Value::String(s) => s
                                            .parse::<i64>()
                                            .ok()
                                            .or_else(|| s.parse::<f64>().ok().map(|f| f as i64))
                                            .map(|i| Value::Number(i.into()))
                                            .unwrap_or(Value::Null),
                                        _ => Value::Null,
                                    })
                                    .collect();
                                Ok(Value::Array(out))
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "LIST".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    "tofloatlist" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(items) => {
                                let out: Vec<Value> = items
                                    .into_iter()
                                    .map(|el| match el {
                                        Value::Null => Value::Null,
                                        Value::Number(n) => n
                                            .as_f64()
                                            .and_then(serde_json::Number::from_f64)
                                            .map(Value::Number)
                                            .unwrap_or(Value::Null),
                                        Value::Bool(b) => {
                                            serde_json::Number::from_f64(if b { 1.0 } else { 0.0 })
                                                .map(Value::Number)
                                                .unwrap_or(Value::Null)
                                        }
                                        Value::String(s) => s
                                            .parse::<f64>()
                                            .ok()
                                            .and_then(serde_json::Number::from_f64)
                                            .map(Value::Number)
                                            .unwrap_or(Value::Null),
                                        _ => Value::Null,
                                    })
                                    .collect();
                                Ok(Value::Array(out))
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "LIST".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    "tostringlist" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(items) => {
                                let out: Vec<Value> = items
                                    .into_iter()
                                    .map(|el| match el {
                                        Value::Null => Value::Null,
                                        Value::String(s) => Value::String(s),
                                        Value::Number(n) => Value::String(n.to_string()),
                                        Value::Bool(b) => Value::String(b.to_string()),
                                        other => Value::String(other.to_string()),
                                    })
                                    .collect();
                                Ok(Value::Array(out))
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "LIST".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    "tobooleanlist" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::Array(items) => {
                                let out: Vec<Value> = items
                                    .into_iter()
                                    .map(|el| match el {
                                        Value::Null => Value::Null,
                                        Value::Bool(b) => Value::Bool(b),
                                        Value::Number(n) => {
                                            Value::Bool(n.as_f64().unwrap_or(0.0) != 0.0)
                                        }
                                        Value::String(s) => {
                                            let lo = s.to_lowercase();
                                            if lo == "true" {
                                                Value::Bool(true)
                                            } else if lo == "false" {
                                                Value::Bool(false)
                                            } else {
                                                Value::Null
                                            }
                                        }
                                        _ => Value::Null,
                                    })
                                    .collect();
                                Ok(Value::Array(out))
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "LIST".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    // phase6_opencypher-quickwins §7 — `exists(prop)` scalar.
                    // Distinguishes "property absent" (false) from "property
                    // present but NULL" (false as well — Cypher treats NULL
                    // properties as absent for EXISTS), and from "property
                    // present with a real value" (true). NULL input returns
                    // NULL under three-valued logic.
                    "exists" => {
                        if args.is_empty() {
                            return Ok(Value::Null);
                        }
                        match &args[0] {
                            parser::Expression::PropertyAccess { variable, property } => {
                                let target = row.get(variable).cloned().unwrap_or(Value::Null);
                                match target {
                                    Value::Null => Ok(Value::Null),
                                    Value::Object(obj) => match obj.get(property) {
                                        Some(Value::Null) | None => Ok(Value::Bool(false)),
                                        Some(_) => Ok(Value::Bool(true)),
                                    },
                                    _ => Ok(Value::Bool(false)),
                                }
                            }
                            other => {
                                let v = self.evaluate_projection_expression(row, context, other)?;
                                Ok(Value::Bool(!matches!(v, Value::Null)))
                            }
                        }
                    }
                    // phase6_opencypher-quickwins §3 — polymorphic `isEmpty`.
                    // Dispatches on STRING / LIST / MAP; returns NULL on NULL.
                    "isempty" => {
                        let v = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match v {
                            Value::Null => Ok(Value::Null),
                            Value::String(s) => Ok(Value::Bool(s.is_empty())),
                            Value::Array(a) => Ok(Value::Bool(a.is_empty())),
                            Value::Object(obj) => {
                                // Treat serialised graph entities as non-empty
                                // (they always carry `_nexus_id`); plain maps
                                // compare by user-visible key count.
                                if obj.contains_key("_nexus_id") {
                                    Ok(Value::Bool(false))
                                } else {
                                    Ok(Value::Bool(obj.is_empty()))
                                }
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "STRING, LIST, or MAP".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    // phase6_opencypher-quickwins §4 — left / right UTF-8-safe
                    // prefix / suffix extraction.
                    "left" => {
                        if args.len() < 2 {
                            return Ok(Value::Null);
                        }
                        let s_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let n_val = self.evaluate_projection_expression(row, context, &args[1])?;
                        if matches!(s_val, Value::Null) || matches!(n_val, Value::Null) {
                            return Ok(Value::Null);
                        }
                        let s = match s_val {
                            Value::String(s) => s,
                            _ => return Ok(Value::Null),
                        };
                        let n = match n_val {
                            Value::Number(n) => n
                                .as_i64()
                                .or_else(|| n.as_f64().map(|f| f as i64))
                                .unwrap_or(0),
                            _ => return Ok(Value::Null),
                        };
                        let take = n.max(0) as usize;
                        Ok(Value::String(s.chars().take(take).collect()))
                    }
                    "right" => {
                        if args.len() < 2 {
                            return Ok(Value::Null);
                        }
                        let s_val = self.evaluate_projection_expression(row, context, &args[0])?;
                        let n_val = self.evaluate_projection_expression(row, context, &args[1])?;
                        if matches!(s_val, Value::Null) || matches!(n_val, Value::Null) {
                            return Ok(Value::Null);
                        }
                        let s = match s_val {
                            Value::String(s) => s,
                            _ => return Ok(Value::Null),
                        };
                        let n = match n_val {
                            Value::Number(n) => n
                                .as_i64()
                                .or_else(|| n.as_f64().map(|f| f as i64))
                                .unwrap_or(0),
                            _ => return Ok(Value::Null),
                        };
                        let char_len = s.chars().count();
                        let take = (n.max(0) as usize).min(char_len);
                        let skip = char_len - take;
                        Ok(Value::String(s.chars().skip(skip).collect()))
                    }
                    // phase6_opencypher-advanced-types §1 — BYTES family.
                    // Uses the `{"_bytes": "<base64>"}` wire shape so
                    // the JSON-based runtime stays unchanged. NULL-in →
                    // NULL-out across every entry point.
                    "bytes" => {
                        let arg = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match arg {
                            Value::Null => Ok(Value::Null),
                            Value::String(s) => super::bytes::bytes_from_vec(s.into_bytes()),
                            other if super::bytes::is_bytes_value(&other) => Ok(other),
                            other => Err(Error::TypeMismatch {
                                expected: "STRING or BYTES".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    "bytesfrombase64" => {
                        let arg = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        match arg {
                            Value::Null => Ok(Value::Null),
                            Value::String(s) => {
                                use base64::Engine as _;
                                use base64::engine::general_purpose::STANDARD as B64;
                                let raw = B64.decode(&s).map_err(|e| {
                                    Error::CypherExecution(format!(
                                        "ERR_INVALID_BYTES: base64 decode failed: {e}"
                                    ))
                                })?;
                                super::bytes::bytes_from_vec(raw)
                            }
                            other => Err(Error::TypeMismatch {
                                expected: "STRING".to_string(),
                                actual: type_name_of(&other).to_string(),
                            }),
                        }
                    }
                    "bytestobase64" => {
                        let arg = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        if matches!(arg, Value::Null) {
                            return Ok(Value::Null);
                        }
                        let raw = super::bytes::bytes_value_to_vec(&arg)?;
                        use base64::Engine as _;
                        use base64::engine::general_purpose::STANDARD as B64;
                        Ok(Value::String(B64.encode(raw)))
                    }
                    "bytestohex" => {
                        let arg = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        if matches!(arg, Value::Null) {
                            return Ok(Value::Null);
                        }
                        let raw = super::bytes::bytes_value_to_vec(&arg)?;
                        Ok(Value::String(super::bytes::to_hex(&raw)))
                    }
                    "byteslength" => {
                        let arg = match args.first() {
                            Some(a) => self.evaluate_projection_expression(row, context, a)?,
                            None => return Ok(Value::Null),
                        };
                        if matches!(arg, Value::Null) {
                            return Ok(Value::Null);
                        }
                        let raw = super::bytes::bytes_value_to_vec(&arg)?;
                        Ok(Value::Number(serde_json::Number::from(raw.len() as i64)))
                    }
                    "bytesslice" => {
                        if args.len() < 3 {
                            return Ok(Value::Null);
                        }
                        let b = self.evaluate_projection_expression(row, context, &args[0])?;
                        let start_v =
                            self.evaluate_projection_expression(row, context, &args[1])?;
                        let len_v = self.evaluate_projection_expression(row, context, &args[2])?;
                        if matches!(b, Value::Null)
                            || matches!(start_v, Value::Null)
                            || matches!(len_v, Value::Null)
                        {
                            return Ok(Value::Null);
                        }
                        let raw = super::bytes::bytes_value_to_vec(&b)?;
                        let start = match &start_v {
                            Value::Number(n) => n
                                .as_i64()
                                .or_else(|| n.as_f64().map(|f| f as i64))
                                .unwrap_or(0),
                            _ => {
                                return Err(Error::TypeMismatch {
                                    expected: "INTEGER".to_string(),
                                    actual: type_name_of(&start_v).to_string(),
                                });
                            }
                        };
                        let len = match &len_v {
                            Value::Number(n) => n
                                .as_i64()
                                .or_else(|| n.as_f64().map(|f| f as i64))
                                .unwrap_or(0),
                            _ => {
                                return Err(Error::TypeMismatch {
                                    expected: "INTEGER".to_string(),
                                    actual: type_name_of(&len_v).to_string(),
                                });
                            }
                        };
                        let sliced = super::bytes::slice(&raw, start, len);
                        super::bytes::bytes_from_vec(sliced)
                    }
                    "todate" => {
                        // toDate(value) - Convert to date string (YYYY-MM-DD)
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date string
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                    // Try datetime format
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::String(
                                            dt.date_naive().format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {year, month, day} format
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Temporal functions
                    "date" => {
                        if args.is_empty() {
                            // Return current date in ISO format (YYYY-MM-DD)
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%Y-%m-%d").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse date from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse ISO date format
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {year, month, day} format
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        return Ok(Value::String(
                                            date.format("%Y-%m-%d").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "datetime" => {
                        if args.is_empty() {
                            // Return current datetime in ISO format
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.to_rfc3339()));
                        } else if let Some(arg) = args.first() {
                            // Parse datetime from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse RFC3339/ISO8601 datetime
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::String(dt.to_rfc3339()));
                                    }
                                    // Try to parse without timezone
                                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
                                        &s,
                                        "%Y-%m-%dT%H:%M:%S",
                                    ) {
                                        let local = chrono::Local::now().timezone();
                                        let dt_local = local
                                            .from_local_datetime(&dt)
                                            .earliest()
                                            .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                        return Ok(Value::String(dt_local.to_rfc3339()));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {year, month, day, hour, minute, second} format
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        if let Some(time) =
                                            chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                        {
                                            let dt = chrono::NaiveDateTime::new(date, time);
                                            let local = chrono::Local::now().timezone();
                                            let dt_local = local
                                                .from_local_datetime(&dt)
                                                .earliest()
                                                .unwrap_or_else(|| local.from_utc_datetime(&dt));
                                            return Ok(Value::String(dt_local.to_rfc3339()));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "time" => {
                        if args.is_empty() {
                            // Return current time in HH:MM:SS format
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%H:%M:%S").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse time from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse time format HH:MM:SS
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                    // Try HH:MM format
                                    if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    // Support {hour, minute, second} format
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(time) =
                                        chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "timestamp" => {
                        if args.is_empty() {
                            // Return current Unix timestamp in milliseconds
                            let now = chrono::Local::now();
                            let millis = now.timestamp_millis();
                            return Ok(Value::Number(millis.into()));
                        } else if let Some(arg) = args.first() {
                            // Parse timestamp from string or return existing number
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Number(n) => {
                                    // Return as-is if already a number
                                    return Ok(Value::Number(n));
                                }
                                Value::String(s) => {
                                    // Try to parse datetime and convert to timestamp
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        let millis = dt.timestamp_millis();
                                        return Ok(Value::Number(millis.into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                // Support duration components: years, months, days, hours, minutes, seconds
                                let mut duration_map = Map::new();

                                if let Some(years) = map.get("years") {
                                    duration_map.insert("years".to_string(), years.clone());
                                }
                                if let Some(months) = map.get("months") {
                                    duration_map.insert("months".to_string(), months.clone());
                                }
                                if let Some(days) = map.get("days") {
                                    duration_map.insert("days".to_string(), days.clone());
                                }
                                if let Some(hours) = map.get("hours") {
                                    duration_map.insert("hours".to_string(), hours.clone());
                                }
                                if let Some(minutes) = map.get("minutes") {
                                    duration_map.insert("minutes".to_string(), minutes.clone());
                                }
                                if let Some(seconds) = map.get("seconds") {
                                    duration_map.insert("seconds".to_string(), seconds.clone());
                                }

                                return Ok(Value::Object(duration_map));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration.between" => {
                        // duration.between(datetime1, datetime2) - computes the duration between two datetimes
                        if args.len() >= 2 {
                            let dt1 =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let dt2 =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if Self::is_datetime_string(&dt1) && Self::is_datetime_string(&dt2) {
                                return self.datetime_difference(&dt1, &dt2);
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration.inMonths" => {
                        // duration.inMonths(datetime1, datetime2) - duration in months
                        if args.len() >= 2 {
                            let dt1 =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let dt2 =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                                // Try parsing as dates
                                let d1 = chrono::NaiveDate::parse_from_str(s1, "%Y-%m-%d").or_else(
                                    |_| {
                                        chrono::DateTime::parse_from_rfc3339(s1)
                                            .map(|dt| dt.date_naive())
                                    },
                                );
                                let d2 = chrono::NaiveDate::parse_from_str(s2, "%Y-%m-%d").or_else(
                                    |_| {
                                        chrono::DateTime::parse_from_rfc3339(s2)
                                            .map(|dt| dt.date_naive())
                                    },
                                );

                                if let (Ok(date1), Ok(date2)) = (d1, d2) {
                                    let months = (date1.year() - date2.year()) * 12
                                        + (date1.month() as i32 - date2.month() as i32);

                                    let mut result_map = Map::new();
                                    result_map
                                        .insert("months".to_string(), Value::Number(months.into()));
                                    return Ok(Value::Object(result_map));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration.inDays" => {
                        // duration.inDays(datetime1, datetime2) - duration in days
                        if args.len() >= 2 {
                            let dt1 =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let dt2 =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                                // Try parsing as dates
                                let d1 = chrono::NaiveDate::parse_from_str(s1, "%Y-%m-%d").or_else(
                                    |_| {
                                        chrono::DateTime::parse_from_rfc3339(s1)
                                            .map(|dt| dt.date_naive())
                                    },
                                );
                                let d2 = chrono::NaiveDate::parse_from_str(s2, "%Y-%m-%d").or_else(
                                    |_| {
                                        chrono::DateTime::parse_from_rfc3339(s2)
                                            .map(|dt| dt.date_naive())
                                    },
                                );

                                if let (Ok(date1), Ok(date2)) = (d1, d2) {
                                    let days = date1.signed_duration_since(date2).num_days();

                                    let mut result_map = Map::new();
                                    result_map
                                        .insert("days".to_string(), Value::Number(days.into()));
                                    return Ok(Value::Object(result_map));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "duration.inSeconds" => {
                        // duration.inSeconds(datetime1, datetime2) - duration in seconds
                        if args.len() >= 2 {
                            let dt1 =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let dt2 =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s1), Value::String(s2)) = (&dt1, &dt2) {
                                // Try parsing as datetimes
                                let d1 = chrono::DateTime::parse_from_rfc3339(s1)
                                    .map(|dt| dt.with_timezone(&chrono::Utc));
                                let d2 = chrono::DateTime::parse_from_rfc3339(s2)
                                    .map(|dt| dt.with_timezone(&chrono::Utc));

                                if let (Ok(dt1), Ok(dt2)) = (d1, d2) {
                                    let seconds = dt1.signed_duration_since(dt2).num_seconds();

                                    let mut result_map = Map::new();
                                    result_map.insert(
                                        "seconds".to_string(),
                                        Value::Number(seconds.into()),
                                    );
                                    return Ok(Value::Object(result_map));
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Path functions
                    "nodes" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // If value is already an array, treat it as a path of nodes
                            if let Value::Array(arr) = value {
                                // Filter only node objects (objects with _nexus_id)
                                let nodes: Vec<Value> = arr
                                    .into_iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_id")
                                        } else {
                                            false
                                        }
                                    })
                                    .collect();
                                return Ok(Value::Array(nodes));
                            }
                            // If it's a single node, return it as array
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_id") {
                                    return Ok(Value::Array(vec![value]));
                                }
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "relationships" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // If value is already an array, extract relationships
                            if let Value::Array(arr) = value {
                                // Filter only relationship objects (objects with _nexus_type and source/target)
                                let rels: Vec<Value> = arr
                                    .into_iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_type")
                                                && (obj.contains_key("_source")
                                                    || obj.contains_key("_target"))
                                        } else {
                                            false
                                        }
                                    })
                                    .collect();
                                return Ok(Value::Array(rels));
                            }
                            // If it's a single relationship, return it as array
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_type") {
                                    return Ok(Value::Array(vec![value]));
                                }
                            }
                        }
                        Ok(Value::Array(Vec::new()))
                    }
                    "length" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            // For arrays representing paths, length is the number of relationships
                            // which is (number of nodes - 1) or number of relationship objects
                            if let Value::Array(arr) = value {
                                // Count relationship objects in the path
                                let rel_count = arr
                                    .iter()
                                    .filter(|v| {
                                        if let Value::Object(obj) = v {
                                            obj.contains_key("_nexus_type")
                                        } else {
                                            false
                                        }
                                    })
                                    .count();
                                return Ok(Value::Number((rel_count as i64).into()));
                            }
                            // For a single relationship, length is 1
                            if let Value::Object(obj) = &value {
                                if obj.contains_key("_nexus_type") {
                                    return Ok(Value::Number(1.into()));
                                }
                            }
                        }
                        Ok(Value::Number(0.into()))
                    }
                    "shortestpath" => {
                        // shortestPath((start)-[*]->(end))
                        // Returns the shortest path between two nodes
                        // For now, we support: shortestPath((a)-[*]->(b)) where a and b are variables
                        if !args.is_empty() {
                            // Try to extract pattern from first argument
                            // Pattern should be a PatternComprehension or we need to extract nodes from context
                            if let parser::Expression::PatternComprehension { pattern, .. } =
                                &args[0]
                            {
                                // Extract start and end nodes from pattern
                                if let (Some(start_node), Some(end_node)) =
                                    (pattern.elements.first(), pattern.elements.last())
                                {
                                    if let (
                                        parser::PatternElement::Node(start),
                                        parser::PatternElement::Node(end),
                                    ) = (start_node, end_node)
                                    {
                                        // Get node IDs from row context
                                        let start_id = if let Some(var) = &start.variable {
                                            if let Some(Value::Object(obj)) = row.get(var) {
                                                if let Some(Value::Number(id)) =
                                                    obj.get("_nexus_id")
                                                {
                                                    id.as_u64()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        let end_id = if let Some(var) = &end.variable {
                                            if let Some(Value::Object(obj)) = row.get(var) {
                                                if let Some(Value::Number(id)) =
                                                    obj.get("_nexus_id")
                                                {
                                                    id.as_u64()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        if let (Some(start_id), Some(end_id)) = (start_id, end_id) {
                                            // Extract relationship type and direction from
                                            // pattern. Slice 3b §5.2: when the pattern carries
                                            // a `QuantifiedGroup` (named-body QPP that the
                                            // slice-1 lowering rejected), reach inside its
                                            // `inner` Vec to pick up the first relationship's
                                            // type and direction. Anonymous-body QPP is already
                                            // covered by the slice-1 lowering — the pattern at
                                            // this point looks like a legacy `*m..n`.
                                            //
                                            // Limitation: the BFS path-finder only honours type
                                            // and direction; per-iteration label / property /
                                            // `WHERE` filters declared inside the QPP body are
                                            // not enforced by `find_shortest_path`. That stays
                                            // as a slice-4 follow-up — `shortestPath` over
                                            // filtered named-body QPP needs the dedicated
                                            // operator wired into the path-finder, not just
                                            // the legacy BFS.
                                            fn extract_first_rel<'a>(
                                                elements: &'a [parser::PatternElement],
                                            ) -> Option<&'a parser::RelationshipPattern>
                                            {
                                                for el in elements {
                                                    match el {
                                                        parser::PatternElement::Relationship(r) => {
                                                            return Some(r);
                                                        }
                                                        parser::PatternElement::QuantifiedGroup(
                                                            g,
                                                        ) => {
                                                            if let Some(r) =
                                                                extract_first_rel(&g.inner)
                                                            {
                                                                return Some(r);
                                                            }
                                                        }
                                                        parser::PatternElement::Node(_) => {}
                                                    }
                                                }
                                                None
                                            }
                                            let rel = extract_first_rel(&pattern.elements);
                                            let rel_type =
                                                rel.and_then(|r| r.types.first().cloned());
                                            let type_id = rel_type.and_then(|t| {
                                                self.catalog().get_type_id(&t).ok().flatten()
                                            });
                                            let direction = rel
                                                .map(|r| match r.direction {
                                                    parser::RelationshipDirection::Outgoing => {
                                                        Direction::Outgoing
                                                    }
                                                    parser::RelationshipDirection::Incoming => {
                                                        Direction::Incoming
                                                    }
                                                    parser::RelationshipDirection::Both => {
                                                        Direction::Both
                                                    }
                                                })
                                                .unwrap_or(Direction::Both);

                                            // Find shortest path using BFS
                                            if let Ok(Some(path)) = self.find_shortest_path(
                                                start_id, end_id, type_id, direction,
                                            ) {
                                                return Ok(self.path_to_value(&path));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "allshortestpaths" => {
                        // allShortestPaths((start)-[*]->(end))
                        // Returns all shortest paths between two nodes
                        if !args.is_empty() {
                            if let parser::Expression::PatternComprehension { pattern, .. } =
                                &args[0]
                            {
                                if let (Some(start_node), Some(end_node)) =
                                    (pattern.elements.first(), pattern.elements.last())
                                {
                                    if let (
                                        parser::PatternElement::Node(start),
                                        parser::PatternElement::Node(end),
                                    ) = (start_node, end_node)
                                    {
                                        let start_id = if let Some(var) = &start.variable {
                                            if let Some(Value::Object(obj)) = row.get(var) {
                                                if let Some(Value::Number(id)) =
                                                    obj.get("_nexus_id")
                                                {
                                                    id.as_u64()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        let end_id = if let Some(var) = &end.variable {
                                            if let Some(Value::Object(obj)) = row.get(var) {
                                                if let Some(Value::Number(id)) =
                                                    obj.get("_nexus_id")
                                                {
                                                    id.as_u64()
                                                } else {
                                                    None
                                                }
                                            } else {
                                                None
                                            }
                                        } else {
                                            None
                                        };

                                        if let (Some(start_id), Some(end_id)) = (start_id, end_id) {
                                            let rel_type = pattern.elements.iter().find_map(|e| {
                                                if let parser::PatternElement::Relationship(rel) = e
                                                {
                                                    rel.types.first().cloned()
                                                } else {
                                                    None
                                                }
                                            });
                                            let type_id = rel_type.and_then(|t| {
                                                self.catalog().get_type_id(&t).ok().flatten()
                                            });
                                            let direction = pattern.elements.iter()
                                                .find_map(|e| {
                                                    if let parser::PatternElement::Relationship(rel) = e {
                                                        Some(match rel.direction {
                                                            parser::RelationshipDirection::Outgoing => Direction::Outgoing,
                                                            parser::RelationshipDirection::Incoming => Direction::Incoming,
                                                            parser::RelationshipDirection::Both => Direction::Both,
                                                        })
                                                    } else {
                                                        None
                                                    }
                                                })
                                                .unwrap_or(Direction::Both);

                                            // Find all shortest paths
                                            if let Ok(paths) = self.find_all_shortest_paths(
                                                start_id, end_id, type_id, direction,
                                            ) {
                                                let path_values: Vec<Value> = paths
                                                    .iter()
                                                    .map(|p| self.path_to_value(p))
                                                    .collect();
                                                return Ok(Value::Array(path_values));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    // List functions
                    "size" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::Array(arr) => Ok(Value::Number((arr.len() as i64).into())),
                                Value::String(s) => Ok(Value::Number((s.len() as i64).into())),
                                _ => Ok(Value::Null),
                            }
                        } else {
                            Ok(Value::Null)
                        }
                    }
                    "head" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                return Ok(arr.first().cloned().unwrap_or(Value::Null));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "tail" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                if arr.len() > 1 {
                                    return Ok(Value::Array(arr[1..].to_vec()));
                                }
                                return Ok(Value::Array(Vec::new()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "last" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                return Ok(arr.last().cloned().unwrap_or(Value::Null));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "range" => {
                        // range(start, end, [step])
                        if args.len() >= 2 {
                            let start_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let end_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::Number(start_num), Value::Number(end_num)) =
                                (start_val, end_val)
                            {
                                // Convert to i64, handling both integer and float cases
                                let start = start_num
                                    .as_i64()
                                    .or_else(|| start_num.as_f64().map(|f| f as i64))
                                    .unwrap_or(0);
                                let end = end_num
                                    .as_i64()
                                    .or_else(|| end_num.as_f64().map(|f| f as i64))
                                    .unwrap_or(0);
                                let step = if args.len() >= 3 {
                                    let step_val = self
                                        .evaluate_projection_expression(row, context, &args[2])?;
                                    if let Value::Number(s) = step_val {
                                        s.as_i64()
                                            .or_else(|| s.as_f64().map(|f| f as i64))
                                            .unwrap_or(1)
                                    } else {
                                        1
                                    }
                                } else {
                                    1
                                };

                                if step == 0 {
                                    return Ok(Value::Array(Vec::new()));
                                }

                                let mut result = Vec::new();
                                if step > 0 {
                                    let mut i = start;
                                    while i <= end {
                                        result.push(Value::Number(i.into()));
                                        i += step;
                                    }
                                } else {
                                    let mut i = start;
                                    while i >= end {
                                        result.push(Value::Number(i.into()));
                                        i += step;
                                    }
                                }
                                return Ok(Value::Array(result));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "reverse" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(mut arr) = value {
                                arr.reverse();
                                return Ok(Value::Array(arr));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "reduce" => {
                        // reduce(accumulator, variable IN list | expression)
                        // Example: reduce(total = 0, n IN [1,2,3] | total + n)
                        if args.len() >= 3 {
                            // First arg: accumulator initial value
                            let acc_init =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            // Second arg: variable name (string)
                            let var_name = if let Value::String(s) =
                                self.evaluate_projection_expression(row, context, &args[1])?
                            {
                                s
                            } else {
                                return Ok(Value::Null);
                            };
                            // Third arg: list
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[2])?;
                            if let Value::Array(list) = list_val {
                                // Fourth arg: expression (optional, if not provided use variable itself)
                                let expr = args.get(3).cloned();

                                let mut accumulator = acc_init;
                                for item in list {
                                    // Set variable in context
                                    let mut new_row = row.clone();
                                    new_row.insert(var_name.clone(), item);

                                    // Evaluate expression with new context
                                    if let Some(ref expr) = expr {
                                        let result = self.evaluate_projection_expression(
                                            &new_row, context, expr,
                                        )?;
                                        accumulator = result;
                                    } else {
                                        accumulator =
                                            new_row.get(&var_name).cloned().unwrap_or(Value::Null);
                                    }
                                }
                                return Ok(accumulator);
                            }
                        }
                        Ok(Value::Null)
                    }
                    "extract" => {
                        // extract(variable IN list | expression)
                        // Example: extract(n IN [1,2,3] | n * 2)
                        if args.len() >= 2 {
                            // First arg: variable name (string)
                            let var_name = if let Value::String(s) =
                                self.evaluate_projection_expression(row, context, &args[0])?
                            {
                                s
                            } else {
                                return Ok(Value::Null);
                            };
                            // Second arg: list
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            if let Value::Array(list) = list_val {
                                // Third arg: expression (optional, if not provided use variable itself)
                                let expr = args.get(2).cloned();

                                let mut results = Vec::new();
                                for item in list {
                                    // Set variable in context
                                    let mut new_row = row.clone();
                                    new_row.insert(var_name.clone(), item);

                                    // Evaluate expression with new context
                                    if let Some(ref expr) = expr {
                                        if let Ok(result) = self
                                            .evaluate_projection_expression(&new_row, context, expr)
                                        {
                                            results.push(result);
                                        }
                                    } else {
                                        results.push(
                                            new_row.get(&var_name).cloned().unwrap_or(Value::Null),
                                        );
                                    }
                                }
                                return Ok(Value::Array(results));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "all" => {
                        // all(variable IN list WHERE predicate)
                        // Returns true if all elements in list satisfy predicate
                        if args.len() >= 2 {
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let Value::Array(list) = list_val {
                                if list.is_empty() {
                                    return Ok(Value::Bool(true)); // All elements of empty list satisfy predicate
                                }

                                // If third arg exists, it's the predicate expression
                                if let Some(predicate) = args.get(2) {
                                    // Extract variable name from first arg if it's a string
                                    let var_name = if let Ok(Value::String(s)) =
                                        self.evaluate_projection_expression(row, context, &args[0])
                                    {
                                        s
                                    } else {
                                        return Ok(Value::Bool(false));
                                    };

                                    for item in list {
                                        let mut new_row = row.clone();
                                        new_row.insert(var_name.clone(), item);

                                        let result = self.evaluate_projection_expression(
                                            &new_row, context, predicate,
                                        )?;
                                        if !result.as_bool().unwrap_or(false) {
                                            return Ok(Value::Bool(false));
                                        }
                                    }
                                    return Ok(Value::Bool(true));
                                }
                            }
                        }
                        Ok(Value::Bool(false))
                    }
                    "any" => {
                        // any(variable IN list WHERE predicate)
                        // Returns true if any element in list satisfies predicate
                        if args.len() >= 2 {
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let Value::Array(list) = list_val {
                                if list.is_empty() {
                                    return Ok(Value::Bool(false)); // No elements satisfy predicate
                                }

                                if let Some(predicate) = args.get(2) {
                                    let var_name = if let Ok(Value::String(s)) =
                                        self.evaluate_projection_expression(row, context, &args[0])
                                    {
                                        s
                                    } else {
                                        return Ok(Value::Bool(false));
                                    };

                                    for item in list {
                                        let mut new_row = row.clone();
                                        new_row.insert(var_name.clone(), item);

                                        let result = self.evaluate_projection_expression(
                                            &new_row, context, predicate,
                                        )?;
                                        if result.as_bool().unwrap_or(false) {
                                            return Ok(Value::Bool(true));
                                        }
                                    }
                                    return Ok(Value::Bool(false));
                                }
                            }
                        }
                        Ok(Value::Bool(false))
                    }
                    "none" => {
                        // none(variable IN list WHERE predicate)
                        // Returns true if no elements in list satisfy predicate
                        if args.len() >= 2 {
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let Value::Array(list) = list_val {
                                if list.is_empty() {
                                    return Ok(Value::Bool(true)); // No elements satisfy predicate
                                }

                                if let Some(predicate) = args.get(2) {
                                    let var_name = if let Ok(Value::String(s)) =
                                        self.evaluate_projection_expression(row, context, &args[0])
                                    {
                                        s
                                    } else {
                                        return Ok(Value::Bool(false));
                                    };

                                    for item in list {
                                        let mut new_row = row.clone();
                                        new_row.insert(var_name.clone(), item);

                                        let result = self.evaluate_projection_expression(
                                            &new_row, context, predicate,
                                        )?;
                                        if result.as_bool().unwrap_or(false) {
                                            return Ok(Value::Bool(false));
                                        }
                                    }
                                    return Ok(Value::Bool(true));
                                }
                            }
                        }
                        Ok(Value::Bool(true))
                    }
                    "single" => {
                        // single(variable IN list WHERE predicate)
                        // Returns true if exactly one element in list satisfies predicate
                        if args.len() >= 2 {
                            let list_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let Value::Array(list) = list_val {
                                if list.is_empty() {
                                    return Ok(Value::Bool(false)); // No elements satisfy
                                }

                                if let Some(predicate) = args.get(2) {
                                    let var_name = if let Ok(Value::String(s)) =
                                        self.evaluate_projection_expression(row, context, &args[0])
                                    {
                                        s
                                    } else {
                                        return Ok(Value::Bool(false));
                                    };

                                    let mut count = 0;
                                    for item in list {
                                        let mut new_row = row.clone();
                                        new_row.insert(var_name.clone(), item);

                                        let result = self.evaluate_projection_expression(
                                            &new_row, context, predicate,
                                        )?;
                                        if result.as_bool().unwrap_or(false) {
                                            count += 1;
                                            if count > 1 {
                                                return Ok(Value::Bool(false));
                                            }
                                        }
                                    }
                                    return Ok(Value::Bool(count == 1));
                                }
                            }
                        }
                        Ok(Value::Bool(false))
                    }
                    "coalesce" => {
                        // coalesce(expr1, expr2, ...) - returns first non-null value
                        // Evaluates arguments in order and returns the first non-null value
                        for arg in args {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if !value.is_null() {
                                return Ok(value);
                            }
                        }
                        // All arguments were null
                        Ok(Value::Null)
                    }
                    // Temporal component extraction functions
                    "year" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::Number((date.year() as i64).into()));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.year() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "month" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::Number((date.month() as i64).into()));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.month() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "day" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::Number((date.day() as i64).into()));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.day() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "hour" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime or time
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.hour() as i64).into()));
                                    }
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::Number((time.hour() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "minute" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime or time
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.minute() as i64).into()));
                                    }
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::Number((time.minute() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "second" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime or time
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.second() as i64).into()));
                                    }
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::Number((time.second() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "quarter" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        let quarter = (date.month() - 1) / 3 + 1;
                                        return Ok(Value::Number((quarter as i64).into()));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        let quarter = (dt.month() - 1) / 3 + 1;
                                        return Ok(Value::Number((quarter as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "week" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::Number(
                                            (date.iso_week().week() as i64).into(),
                                        ));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number(
                                            (dt.iso_week().week() as i64).into(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "dayofweek" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        // Neo4j returns 1-7 (Monday to Sunday)
                                        return Ok(Value::Number(
                                            (date.weekday().num_days_from_monday() as i64 + 1)
                                                .into(),
                                        ));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number(
                                            (dt.weekday().num_days_from_monday() as i64 + 1).into(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "dayofyear" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse date/datetime
                                    if let Ok(date) =
                                        chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                                    {
                                        return Ok(Value::Number((date.ordinal() as i64).into()));
                                    }
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number((dt.ordinal() as i64).into()));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "millisecond" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number(
                                            ((dt.timestamp_subsec_millis() % 1000) as i64).into(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "microsecond" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number(
                                            ((dt.timestamp_subsec_micros() % 1000000) as i64)
                                                .into(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "nanosecond" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::Number(
                                            ((dt.timestamp_subsec_nanos() % 1000000000) as i64)
                                                .into(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Advanced string functions
                    "left" => {
                        // left(string, length) - returns leftmost n characters
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let length_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::Number(len_num)) =
                                (string_val, length_val)
                            {
                                let length = len_num.as_i64().unwrap_or(0).max(0) as usize;
                                let chars: Vec<char> = s.chars().collect();
                                let end = length.min(chars.len());
                                return Ok(Value::String(chars[..end].iter().collect()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "right" => {
                        // right(string, length) - returns rightmost n characters
                        if args.len() >= 2 {
                            let string_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let length_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;

                            if let (Value::String(s), Value::Number(len_num)) =
                                (string_val, length_val)
                            {
                                let length = len_num.as_i64().unwrap_or(0).max(0) as usize;
                                let chars: Vec<char> = s.chars().collect();
                                let start = chars.len().saturating_sub(length);
                                return Ok(Value::String(chars[start..].iter().collect()));
                            }
                        }
                        Ok(Value::Null)
                    }
                    // List functions
                    // filter() is now handled by the parser - it gets converted to ListComprehension
                    // during parsing, so it will never reach here as a FunctionCall
                    "flatten" => {
                        // flatten(list) - flattens a list of lists by one level
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Array(arr) = value {
                                let mut result = Vec::new();
                                for item in arr {
                                    if let Value::Array(inner) = item {
                                        result.extend(inner);
                                    } else {
                                        result.push(item);
                                    }
                                }
                                return Ok(Value::Array(result));
                            }
                        }
                        Ok(Value::Null)
                    }
                    "zip" => {
                        // zip(list1, list2, ...) - zips multiple lists together
                        if args.len() >= 2 {
                            let mut lists: Vec<Vec<Value>> = Vec::new();
                            let mut min_len = usize::MAX;

                            for arg in args {
                                let value =
                                    self.evaluate_projection_expression(row, context, arg)?;
                                if let Value::Array(arr) = value {
                                    min_len = min_len.min(arr.len());
                                    lists.push(arr);
                                } else {
                                    return Ok(Value::Null);
                                }
                            }

                            let mut result = Vec::new();
                            for i in 0..min_len {
                                let mut tuple = Vec::new();
                                for list in &lists {
                                    tuple.push(list[i].clone());
                                }
                                result.push(Value::Array(tuple));
                            }
                            return Ok(Value::Array(result));
                        }
                        Ok(Value::Null)
                    }
                    // Mathematical functions
                    "asin" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.asin())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "acos" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.acos())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "atan" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.atan())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "atan2" => {
                        // atan2(y, x) - returns arctangent of y/x
                        if args.len() >= 2 {
                            let y_val =
                                self.evaluate_projection_expression(row, context, &args[0])?;
                            let x_val =
                                self.evaluate_projection_expression(row, context, &args[1])?;
                            if y_val.is_null() || x_val.is_null() {
                                return Ok(Value::Null);
                            }
                            let y = self.value_to_number(&y_val)?;
                            let x = self.value_to_number(&x_val)?;
                            return serde_json::Number::from_f64(y.atan2(x))
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "exp" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.exp())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "log" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            // Natural logarithm (ln)
                            return serde_json::Number::from_f64(num.ln())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "log10" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            return serde_json::Number::from_f64(num.log10())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "radians" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            // Convert degrees to radians
                            return serde_json::Number::from_f64(num.to_radians())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "degrees" => {
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if value.is_null() {
                                return Ok(Value::Null);
                            }
                            let num = self.value_to_number(&value)?;
                            // Convert radians to degrees
                            return serde_json::Number::from_f64(num.to_degrees())
                                .map(Value::Number)
                                .ok_or_else(|| Error::TypeMismatch {
                                    expected: "number".to_string(),
                                    actual: "non-finite".to_string(),
                                });
                        }
                        Ok(Value::Null)
                    }
                    "pi" => {
                        // pi() - returns the mathematical constant π
                        Ok(Value::Number(
                            serde_json::Number::from_f64(std::f64::consts::PI).unwrap(),
                        ))
                    }
                    "e" => {
                        // e() - returns the mathematical constant e
                        Ok(Value::Number(
                            serde_json::Number::from_f64(std::f64::consts::E).unwrap(),
                        ))
                    }
                    // Advanced temporal functions
                    "localtime" => {
                        // localtime() - returns current local time without timezone
                        if args.is_empty() {
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%H:%M:%S").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse time from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse time format
                                    if let Ok(time) =
                                        chrono::NaiveTime::parse_from_str(&s, "%H:%M:%S")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                    // Try HH:MM format
                                    if let Ok(time) = chrono::NaiveTime::parse_from_str(&s, "%H:%M")
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(time) =
                                        chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                    {
                                        return Ok(Value::String(
                                            time.format("%H:%M:%S").to_string(),
                                        ));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    "localdatetime" => {
                        // localdatetime() - returns current local datetime without timezone
                        if args.is_empty() {
                            let now = chrono::Local::now();
                            return Ok(Value::String(now.format("%Y-%m-%dT%H:%M:%S").to_string()));
                        } else if let Some(arg) = args.first() {
                            // Parse datetime from string or map
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            match value {
                                Value::String(s) => {
                                    // Try to parse datetime format
                                    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(
                                        &s,
                                        "%Y-%m-%dT%H:%M:%S",
                                    ) {
                                        return Ok(Value::String(
                                            dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                                        ));
                                    }
                                    // Try with timezone and convert to naive
                                    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&s) {
                                        return Ok(Value::String(
                                            dt.naive_local()
                                                .format("%Y-%m-%dT%H:%M:%S")
                                                .to_string(),
                                        ));
                                    }
                                }
                                Value::Object(map) => {
                                    let year = map
                                        .get("year")
                                        .and_then(|v| v.as_i64())
                                        .unwrap_or_else(|| chrono::Local::now().year() as i64)
                                        as i32;
                                    let month =
                                        map.get("month").and_then(|v| v.as_u64()).unwrap_or(1)
                                            as u32;
                                    let day =
                                        map.get("day").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
                                    let hour = map.get("hour").and_then(|v| v.as_u64()).unwrap_or(0)
                                        as u32;
                                    let minute =
                                        map.get("minute").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;
                                    let second =
                                        map.get("second").and_then(|v| v.as_u64()).unwrap_or(0)
                                            as u32;

                                    if let Some(date) =
                                        chrono::NaiveDate::from_ymd_opt(year, month, day)
                                    {
                                        if let Some(time) =
                                            chrono::NaiveTime::from_hms_opt(hour, minute, second)
                                        {
                                            let dt = chrono::NaiveDateTime::new(date, time);
                                            return Ok(Value::String(
                                                dt.format("%Y-%m-%dT%H:%M:%S").to_string(),
                                            ));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(Value::Null)
                    }
                    // Duration component extraction functions
                    "years" => {
                        // years(duration) - extract years component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(years) = map.get("years") {
                                    return Ok(years.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "months" => {
                        // months(duration) - extract months component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(months) = map.get("months") {
                                    return Ok(months.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "weeks" => {
                        // weeks(duration) - extract weeks component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(weeks) = map.get("weeks") {
                                    return Ok(weeks.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "days" => {
                        // days(duration) - extract days component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(days) = map.get("days") {
                                    return Ok(days.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "hours" => {
                        // hours(duration) - extract hours component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(hours) = map.get("hours") {
                                    return Ok(hours.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "minutes" => {
                        // minutes(duration) - extract minutes component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(minutes) = map.get("minutes") {
                                    return Ok(minutes.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    "seconds" => {
                        // seconds(duration) - extract seconds component from duration
                        if let Some(arg) = args.first() {
                            let value = self.evaluate_projection_expression(row, context, arg)?;
                            if let Value::Object(map) = value {
                                if let Some(seconds) = map.get("seconds") {
                                    return Ok(seconds.clone());
                                }
                            }
                        }
                        Ok(Value::Null)
                    }
                    _ => Ok(Value::Null),
                }
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
}
