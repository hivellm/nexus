//! `execute_filter` and `execute_optional_filter`. Filter applies a WHERE
//! predicate to rows in the working set; optional-filter preserves rows
//! whose optional variables are NULL instead of dropping them.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::push_with_row_cap;
use super::super::types::{ResultSet, Row};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_filter(
        &self,
        context: &mut ExecutionContext,
        predicate: &str,
    ) -> Result<()> {
        // Try index-based filtering first (optimization for Phase 5)
        if let Some(optimized_rows) = self.try_index_based_filter(context, predicate)? {
            // Index-based filtering succeeded, use optimized results
            context.result_set.rows = optimized_rows;
            return Ok(());
        }

        // Fall back to regular filter execution
        // Check for label check pattern: variable:Label
        if predicate.contains(':') && !predicate.contains("::") {
            let parts: Vec<&str> = predicate.split(':').collect();
            if parts.len() == 2 && !parts[0].contains(' ') && !parts[1].contains(' ') {
                // This is a label check: variable:Label
                let variable = parts[0].trim();
                let label_name = parts[1].trim();

                // Get label ID
                if let Ok(label_id) = self.catalog().get_label_id(label_name) {
                    // Filter rows where variable has this label
                    let rows = self.materialize_rows_from_variables(context);
                    let mut filtered_rows = Vec::new();

                    for row in rows {
                        if let Some(Value::Object(obj)) = row.get(variable) {
                            if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                if let Some(node_id) = id.as_u64() {
                                    // Read node and check if it has the label
                                    if let Ok(node_record) = self.store().read_node(node_id) {
                                        // Check if node has the label
                                        // For label_id < 64, use bitmap check (fast)
                                        // For label_id >= 64, labels are not stored in bitmap, so return false
                                        let has_label = if label_id < 64 {
                                            (node_record.label_bits & (1u64 << label_id)) != 0
                                        } else {
                                            // Labels with ID >= 64 are not supported in the bitmap
                                            // This is a limitation of the current implementation
                                            false
                                        };
                                        if has_label {
                                            filtered_rows.push(row);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    self.update_variables_from_rows(context, &filtered_rows);
                    self.update_result_set_from_rows(context, &filtered_rows);
                    return Ok(());
                }
            }
        }

        // Regular predicate expression
        let mut parser = parser::CypherParser::new(predicate.to_string());
        let expr = parser.parse_expression()?;

        // Get rows from variables OR from result_set.rows (e.g., from UNWIND)
        // CRITICAL: Always prefer materializing from variables if they exist,
        // because variables contain the actual node/relationship objects with all properties.
        // Using result_set.rows may lose property information if columns were reordered.
        let had_existing_rows = !context.result_set.rows.is_empty();
        let existing_columns = if had_existing_rows {
            context.result_set.columns.clone()
        } else {
            Vec::new()
        };

        // DEBUG: Print state at start of Filter

        // CRITICAL FIX: If result_set.rows already exists, use them directly to avoid rematerialization
        // Rematerializing from variables when rows already exist can cause duplicates if variables
        // contain unfiltered arrays. Only materialize from variables if no rows exist yet.
        let rows = if had_existing_rows {
            // Use existing rows - they're already correctly materialized and filtered
            // This prevents duplicate materialization when variables still contain unfiltered arrays
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &existing_columns))
                .collect()
        } else if !context.variables.is_empty() {
            // No existing rows - materialize from variables (source of truth)
            // This ensures we have full node/relationship objects with all properties accessible for filtering
            self.materialize_rows_from_variables(context)
        } else {
            // No variables and no existing rows
            Vec::new()
        };
        let mut filtered_rows = Vec::new();

        // Check if we're in a RETURN ... WHERE scenario (no MATCH, no variables, no existing rows)
        // For RETURN ... WHERE, we should have no rows, no variables, and no existing result_set rows
        // Columns might have markers from previous Filter execution, which is OK
        let is_return_where_scenario = rows.is_empty()
            && context.variables.is_empty()
            && !had_existing_rows
            && self.can_evaluate_without_variables(&expr);

        if is_return_where_scenario {
            // Evaluate predicate directly without a row
            let empty_row = std::collections::HashMap::new();
            if self.evaluate_predicate_on_row(&empty_row, context, &expr)? {
                // Only create a row if predicate is true
                filtered_rows.push(empty_row);
            }
            // If predicate is false, filtered_rows stays empty (no rows returned)
        } else {
            // CRITICAL DEBUG: Log number of input rows before filtering
            tracing::debug!(
                "Filter operator: received {} input rows before filtering",
                rows.len()
            );

            // CRITICAL FIX: Deduplicate rows by COMPOSITE KEY (all values in row) before filtering
            // Use HashSet to track unique row combinations to avoid processing duplicate rows
            // IMPORTANT: Include BOTH node IDs AND primitive values (from UNWIND) in the key
            // This allows valid cartesian products and UNWIND-generated rows to be processed correctly
            use std::collections::HashSet;
            let mut seen_row_keys = HashSet::new();

            for row in &rows {
                // Extract ALL values from row to create composite key
                // CRITICAL FIX: Include variable names in the key to differentiate between
                // rows like (p1=Alice, p2=Bob) and (p1=Bob, p2=Alice)
                let mut var_value_pairs: Vec<(String, String)> = Vec::new();
                let mut found_node_id: Option<u64> = None;

                // First pass: collect (variable_name, value_key) pairs for ALL values
                for var_name in row.keys() {
                    if let Some(value) = row.get(var_name) {
                        let value_key = match value {
                            Value::Object(obj) => {
                                // For objects (nodes/relationships), use _nexus_id
                                if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                    if let Some(node_id) = id.as_u64() {
                                        if found_node_id.is_none() {
                                            found_node_id = Some(node_id);
                                        }
                                        format!("obj:{}", node_id)
                                    } else {
                                        "obj:unknown".to_string()
                                    }
                                } else {
                                    "obj:no_id".to_string()
                                }
                            }
                            // CRITICAL: Handle primitive values from UNWIND
                            Value::Number(n) => format!("num:{}", n),
                            Value::String(s) => format!("str:{}", s),
                            Value::Bool(b) => format!("bool:{}", b),
                            Value::Null => "null".to_string(),
                            Value::Array(arr) => format!("arr:{}", arr.len()),
                        };
                        var_value_pairs.push((var_name.clone(), value_key));
                    }
                }

                // Sort by variable name for consistent key generation
                // This ensures the key order is deterministic
                var_value_pairs.sort_by(|a, b| a.0.cmp(&b.0));

                // Create composite key with variable names and values
                // Format: "var1=val1,var2=val2,..." to differentiate all row combinations
                let row_key = var_value_pairs
                    .iter()
                    .map(|(var, val)| format!("{}={}", var, val))
                    .collect::<Vec<_>>()
                    .join(",");

                // Check if we've seen this exact combination before
                let is_duplicate = !seen_row_keys.insert(row_key);

                // Only process row if it's not a duplicate and passes the predicate
                if !is_duplicate {
                    // TRACE: Check row variables for relationships before evaluation
                    let mut has_relationships_in_row = false;
                    let mut var_types: Vec<(String, String)> = Vec::new();
                    for (var_name, var_value) in row.iter() {
                        let var_type = match var_value {
                            Value::Object(obj) => {
                                if obj.contains_key("type") {
                                    has_relationships_in_row = true;
                                    "RELATIONSHIP".to_string()
                                } else {
                                    "NODE".to_string()
                                }
                            }
                            _ => "OTHER".to_string(),
                        };
                        var_types.push((var_name.clone(), var_type));
                    }
                    // CRITICAL FIX: Extract variable name from predicate for correct logging
                    // Try to extract the variable from PropertyAccess expressions (e.g., "p1.name")
                    let predicate_var_name = match &expr {
                        parser::Expression::PropertyAccess { variable, .. } => {
                            Some(variable.clone())
                        }
                        parser::Expression::BinaryOp { left, .. } => {
                            // For binary ops like "p1.name = 'Alice'", extract from left side
                            match left.as_ref() {
                                parser::Expression::PropertyAccess { variable, .. } => {
                                    Some(variable.clone())
                                }
                                _ => None,
                            }
                        }
                        _ => None,
                    };

                    // DEBUG: Log node properties before evaluating predicate
                    // Use the variable from predicate if available, otherwise use found_node_id
                    let log_node_id = if let Some(var_name) = &predicate_var_name {
                        // Try to get node_id from the specific variable in the row
                        row.get(var_name)
                            .and_then(|v| {
                                if let Value::Object(obj) = v {
                                    obj.get("_nexus_id").and_then(|id| id.as_u64())
                                } else {
                                    None
                                }
                            })
                            .or(found_node_id)
                    } else {
                        found_node_id
                    };

                    let predicate_result = self.evaluate_predicate_on_row(row, context, &expr)?;
                    if predicate_result {
                        filtered_rows.push(row.clone());
                        // Row key already tracked in seen_row_keys during duplicate check
                    }
                }
            }

            // CRITICAL DEBUG: Log number of filtered rows after deduplication and predicate evaluation
            tracing::debug!(
                "Filter operator: {} rows passed deduplication and predicate (from {} input rows)",
                filtered_rows.len(),
                rows.len()
            );
        }

        // If Filter processed rows and there were no rows/variables to begin with (RETURN ... WHERE),
        // we need to handle it specially:
        // - If predicate was false: set a marker column so Project knows not to create a row
        // - If predicate was true: update result set normally (row will be in result_set.rows)
        if filtered_rows.is_empty() && is_return_where_scenario {
            // Predicate was false - Filter removed all rows, set marker so Project doesn't create a row
            // Clear variables and result set since we have no rows
            context.variables.clear();
            context.result_set.columns = vec!["__filtered__".to_string()];
            context.result_set.rows.clear();
        } else if !filtered_rows.is_empty() && is_return_where_scenario {
            // Predicate was true - Filter created a row from empty
            // Update variables and result set, but preserve that Filter created the row
            self.update_variables_from_rows(context, &filtered_rows);
            self.update_result_set_from_rows(context, &filtered_rows);
            // If columns are empty after update (no variables), mark that Filter created the row
            // so Project knows not to create another one
            if context.result_set.columns.is_empty() {
                context.result_set.columns = vec!["__filter_created__".to_string()];
            }
        } else if had_existing_rows {
            // Had rows from result_set (e.g., from UNWIND or previous operators) - preserve columns and update rows
            // DEBUG
            if !filtered_rows.is_empty() {}
            // CRITICAL FIX: Clear result_set.rows BEFORE updating to ensure we don't mix old and new rows
            // This prevents duplicates when Filter processes rows that were already materialized
            context.result_set.rows.clear();
            // CRITICAL FIX: Update variables to reflect filtered rows
            // This is essential when there are multiple NodeByLabel operators - the second NodeByLabel
            // will materialize rows from variables, so variables must contain only filtered nodes
            self.update_variables_from_rows(context, &filtered_rows);
            // Preserve existing columns and update rows completely (no mixing with old rows)
            context.result_set.columns = existing_columns.clone();
            context.result_set.rows = filtered_rows
                .iter()
                .map(|row_map| Row {
                    values: existing_columns
                        .iter()
                        .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                        .collect(),
                })
                .collect();
        } else {
            // Had rows initially from variables - update result set normally
            // Update variables FIRST (this clears old variables and sets new filtered ones),
            // then result_set, ensuring variables match filtered rows
            // CRITICAL: update_result_set_from_rows already replaces result_set.rows completely,
            // so no need to clear beforehand
            if !filtered_rows.is_empty() {}
            self.update_variables_from_rows(context, &filtered_rows);
            self.update_result_set_from_rows(context, &filtered_rows);
        }
        Ok(())
    }

    /// Execute OptionalFilter operator - special filter for WHERE after OPTIONAL MATCH
    /// Unlike regular Filter, if predicate fails but optional_vars are involved,
    /// the row is preserved with optional_vars set to NULL instead of being removed
    pub(in crate::executor) fn execute_optional_filter(
        &self,
        context: &mut ExecutionContext,
        predicate: &str,
        optional_vars: &[String],
    ) -> Result<()> {
        tracing::debug!(
            "execute_optional_filter: predicate='{}', optional_vars={:?}",
            predicate,
            optional_vars
        );

        // Parse the predicate
        let mut parser = parser::CypherParser::new(predicate.to_string());
        let expr = parser.parse_expression()?;

        // Get rows from variables or result_set
        let had_existing_rows = !context.result_set.rows.is_empty();
        let existing_columns = if had_existing_rows {
            context.result_set.columns.clone()
        } else {
            Vec::new()
        };

        let rows: Vec<HashMap<String, Value>> = if had_existing_rows {
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &existing_columns))
                .collect()
        } else if !context.variables.is_empty() {
            self.materialize_rows_from_variables(context)
        } else {
            Vec::new()
        };

        // Neo4j semantics: group by mandatory vars, keep passing rows OR one NULL row per group
        // Identify mandatory variables (all vars NOT in optional_vars)
        let all_vars: std::collections::HashSet<&String> =
            rows.first().map(|r| r.keys().collect()).unwrap_or_default();
        let optional_set: std::collections::HashSet<&String> = optional_vars.iter().collect();
        let mandatory_vars: Vec<&String> = all_vars.difference(&optional_set).cloned().collect();

        // Helper to create a group key from mandatory variables
        let make_group_key = |row: &HashMap<String, Value>| -> String {
            mandatory_vars
                .iter()
                .map(|var| {
                    let val = row.get(*var).cloned().unwrap_or(Value::Null);
                    format!("{}={:?}", var, val)
                })
                .collect::<Vec<_>>()
                .join("|")
        };

        // Group rows by mandatory variables and evaluate predicate
        let mut groups: std::collections::HashMap<String, Vec<(HashMap<String, Value>, bool)>> =
            std::collections::HashMap::new();

        for row in &rows {
            let group_key = make_group_key(row);
            let all_optional_null = optional_vars
                .iter()
                .all(|var| matches!(row.get(var), None | Some(Value::Null)));

            let predicate_passes = if all_optional_null {
                false // NULL row doesn't "pass" - it's a fallback
            } else {
                self.evaluate_predicate_on_row(row, context, &expr)?
            };

            groups
                .entry(group_key)
                .or_default()
                .push((row.clone(), predicate_passes));
        }

        // Build result: for each group, keep passing rows OR one NULL row
        let mut result_rows: Vec<HashMap<String, Value>> = Vec::new();
        for (_group_key, group_rows) in groups {
            let passing_rows: Vec<_> = group_rows
                .iter()
                .filter(|(_, passes)| *passes)
                .map(|(row, _)| row.clone())
                .collect();

            if !passing_rows.is_empty() {
                result_rows.extend(passing_rows);
            } else if let Some((template_row, _)) = group_rows.first() {
                let mut null_row = template_row.clone();
                for var in optional_vars {
                    null_row.insert(var.clone(), Value::Null);
                }
                result_rows.push(null_row);
            }
        }

        tracing::debug!(
            "OptionalFilter: {} input rows -> {} output rows",
            rows.len(),
            result_rows.len()
        );

        // Update context with result rows
        if had_existing_rows {
            context.result_set.rows.clear();
            self.update_variables_from_rows(context, &result_rows);
            context.result_set.columns = existing_columns.clone();
            context.result_set.rows = result_rows
                .iter()
                .map(|row_map| Row {
                    values: existing_columns
                        .iter()
                        .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                        .collect(),
                })
                .collect();
        } else {
            self.update_variables_from_rows(context, &result_rows);
            self.update_result_set_from_rows(context, &result_rows);
        }

        Ok(())
    }
}
