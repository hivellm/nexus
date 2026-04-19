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

            // Try the columnar fast path first — fires only when the
            // batch is large enough that the per-batch materialisation
            // cost amortises AND the predicate shape is
            // `variable.property OP numeric-literal`. Every other
            // filter shape (string predicates, multi-column predicates,
            // IS NULL, subqueries, function calls, heterogeneous row
            // widths) stays on the row-at-a-time path below, unchanged
            // — see §3.3 of phase3_executor-columnar-wiring.
            use std::collections::HashSet;
            let mut columnar_fast_path_taken = false;
            if context.should_use_columnar(rows.len(), self.config.columnar_threshold) {
                if let Some(mask) = try_columnar_filter_mask(&rows, &expr) {
                    tracing::debug!(
                        "Filter operator: columnar fast path on {} rows (threshold={})",
                        rows.len(),
                        self.config.columnar_threshold
                    );
                    let mut seen_row_keys = HashSet::new();
                    for (row, &keep) in rows.iter().zip(mask.iter()) {
                        if !keep {
                            continue;
                        }
                        let row_key = compute_row_dedup_key(row);
                        if seen_row_keys.insert(row_key) {
                            filtered_rows.push(row.clone());
                        }
                    }
                    columnar_fast_path_taken = true;
                }
            }

            if !columnar_fast_path_taken {
                // CRITICAL FIX: Deduplicate rows by COMPOSITE KEY (all values in row) before filtering
                // Use HashSet to track unique row combinations to avoid processing duplicate rows
                // IMPORTANT: Include BOTH node IDs AND primitive values (from UNWIND) in the key
                // This allows valid cartesian products and UNWIND-generated rows to be processed correctly
                let mut seen_row_keys = HashSet::new();

                for row in &rows {
                    let row_key = compute_row_dedup_key(row);
                    // `found_node_id` is retained purely for the debug-log
                    // pathway below; `HashMap` iteration has always been
                    // non-deterministic so "first" has always meant "any"
                    // — `find_map` preserves that shape.
                    let found_node_id: Option<u64> = row.values().find_map(|v| {
                        if let Value::Object(obj) = v {
                            if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                                return id.as_u64();
                            }
                        }
                        None
                    });

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

                        let predicate_result =
                            self.evaluate_predicate_on_row(row, context, &expr)?;
                        if predicate_result {
                            filtered_rows.push(row.clone());
                            // Row key already tracked in seen_row_keys during duplicate check
                        }
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

/// Compute the dedup key for a row — the same shape the filter
/// operator's row-at-a-time path has always used. Pulled out of the
/// inline loop so the columnar fast path in
/// [`Executor::execute_filter`] produces byte-for-byte identical
/// output to the row path when both apply.
fn compute_row_dedup_key(row: &HashMap<String, Value>) -> String {
    let mut var_value_pairs: Vec<(String, String)> = Vec::with_capacity(row.len());
    for var_name in row.keys() {
        if let Some(value) = row.get(var_name) {
            let value_key = match value {
                Value::Object(obj) => {
                    // For objects (nodes/relationships), use _nexus_id
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(node_id) = id.as_u64() {
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
    var_value_pairs.sort_by(|a, b| a.0.cmp(&b.0));
    var_value_pairs
        .iter()
        .map(|(var, val)| format!("{}={}", var, val))
        .collect::<Vec<_>>()
        .join(",")
}

/// Try to compute a filter mask columnar-style.
///
/// Returns `Some(mask)` only when the predicate is a
/// `variable.property OP numeric-literal` binary op with a
/// comparison operator (`=`, `!=`, `<`, `<=`, `>`, `>=`) — the shape
/// the SIMD compare kernels cover today. `None` means "fall back to
/// the row-at-a-time executor path unchanged", so every other
/// predicate shape (string ops, AND/OR trees, IS NULL, function
/// calls, multi-column, etc.) keeps its existing semantics.
///
/// The returned `Vec<bool>` has length `rows.len()` — one entry per
/// input row, in the input order, with `true` marking rows the
/// predicate accepts.
fn try_columnar_filter_mask(
    rows: &[HashMap<String, Value>],
    expr: &parser::Expression,
) -> Option<Vec<bool>> {
    use crate::execution::columnar::{Column, ComparisonOp, DataType};
    use parser::{BinaryOperator, Expression, Literal};

    let (left, op, right) = match expr {
        Expression::BinaryOp { left, op, right } => (left.as_ref(), *op, right.as_ref()),
        _ => return None,
    };

    let cmp_op = match op {
        BinaryOperator::Equal => ComparisonOp::Equal,
        BinaryOperator::NotEqual => ComparisonOp::NotEqual,
        BinaryOperator::LessThan => ComparisonOp::Less,
        BinaryOperator::LessThanOrEqual => ComparisonOp::LessEqual,
        BinaryOperator::GreaterThan => ComparisonOp::Greater,
        BinaryOperator::GreaterThanOrEqual => ComparisonOp::GreaterEqual,
        _ => return None,
    };

    // Only `property OP literal` today. `literal OP property`
    // (argument-swapped form) stays on the row path — it's rare and
    // needs operator inversion logic that's cheaper to land alongside
    // string / multi-column support than up front here.
    let (variable, property) = match left {
        Expression::PropertyAccess { variable, property } => (variable.as_str(), property.as_str()),
        _ => return None,
    };

    match right {
        Expression::Literal(Literal::Integer(n)) => {
            let column = Column::materialise_from_rows(rows, variable, property, DataType::Int64)?;
            column.compare_scalar_i64(*n, cmp_op)
        }
        Expression::Literal(Literal::Float(f)) => {
            let column =
                Column::materialise_from_rows(rows, variable, property, DataType::Float64)?;
            column.compare_scalar_f64(*f, cmp_op)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! §3.4 byte-for-byte parity — the columnar fast path and the
    //! row-at-a-time path must produce identical result sets for every
    //! predicate the fast path claims to handle. Run both paths over
    //! the same 10k-row fixture by flipping `columnar_threshold`
    //! between `usize::MAX` (forces row path) and `4096` (default —
    //! fast path fires) and assert value equality.

    use super::*;
    use crate::executor::context::ExecutionContext;
    use crate::testing::create_test_executor;

    fn build_person(id: u64, age: i64, score: f64) -> Value {
        let mut props = serde_json::Map::new();
        props.insert("age".to_string(), Value::Number(age.into()));
        props.insert(
            "score".to_string(),
            Value::Number(
                serde_json::Number::from_f64(score).expect("fixture score is always finite"),
            ),
        );

        let mut node = serde_json::Map::new();
        node.insert("_nexus_id".to_string(), Value::Number(id.into()));
        node.insert("properties".to_string(), Value::Object(props));
        Value::Object(node)
    }

    fn filter_with_threshold(
        nodes: &[Value],
        predicate: &str,
        columnar_threshold: usize,
    ) -> Vec<Vec<Value>> {
        let (mut executor, _ctx) = create_test_executor();
        executor.config.columnar_threshold = columnar_threshold;
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("n", Value::Array(nodes.to_vec()));
        executor
            .execute_filter(&mut context, predicate)
            .expect("filter should succeed");
        context
            .result_set
            .rows
            .iter()
            .map(|row| row.values.clone())
            .collect()
    }

    fn assert_parity(nodes: &[Value], predicate: &str) {
        let row_path = filter_with_threshold(nodes, predicate, usize::MAX);
        let columnar = filter_with_threshold(nodes, predicate, 4096);
        assert_eq!(
            row_path.len(),
            columnar.len(),
            "row/columnar row-count mismatch for `{}`: row={} columnar={}",
            predicate,
            row_path.len(),
            columnar.len()
        );
        assert_eq!(
            row_path, columnar,
            "row/columnar value mismatch for predicate `{}`",
            predicate
        );
    }

    #[test]
    fn filter_columnar_matches_row_path_on_10k_int_predicates() {
        let nodes: Vec<Value> = (0..10_000)
            .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
            .collect();
        assert!(nodes.len() > 4096, "fixture must exceed columnar threshold");

        for predicate in [
            "n.age > 5000",
            "n.age >= 5000",
            "n.age < 5000",
            "n.age <= 5000",
            "n.age = 5000",
            "n.age <> 5000",
        ] {
            assert_parity(&nodes, predicate);
        }
    }

    #[test]
    fn prefer_columnar_hint_forces_fast_path_below_threshold() {
        // 500 rows is far below the default threshold of 4096, so the
        // fast path would normally stay dormant. With
        // `PreferColumnar(true)` in the context, it must fire and
        // produce the same output as the row path.
        let nodes: Vec<Value> = (0..500)
            .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
            .collect();

        let (mut executor, _ctx) = create_test_executor();
        executor.config.columnar_threshold = 4096;
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_plan_hints(vec![crate::executor::planner::PlanHint::PreferColumnar(
            true,
        )]);
        context.set_variable("n", Value::Array(nodes.clone()));
        executor
            .execute_filter(&mut context, "n.age > 100")
            .expect("filter should succeed");
        let hinted: Vec<_> = context
            .result_set
            .rows
            .iter()
            .map(|r| r.values.clone())
            .collect();

        // Baseline: same query without the hint at the same size
        // takes the row path (500 < 4096), so should produce an
        // identical result set.
        let baseline = filter_with_threshold(&nodes, "n.age > 100", 4096);
        assert_eq!(hinted, baseline, "hint should not change output values");
        assert!(
            !hinted.is_empty(),
            "fixture must produce at least some matches"
        );
    }

    #[test]
    fn disable_columnar_hint_forces_row_path_above_threshold() {
        // 5 000 rows would normally trip the 4096 threshold and take
        // the fast path. `PreferColumnar(false)` must pin the row
        // path while still producing identical output.
        let nodes: Vec<Value> = (0..5_000)
            .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
            .collect();

        let (mut executor, _ctx) = create_test_executor();
        executor.config.columnar_threshold = 4096;
        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_plan_hints(vec![crate::executor::planner::PlanHint::PreferColumnar(
            false,
        )]);
        context.set_variable("n", Value::Array(nodes.clone()));
        executor
            .execute_filter(&mut context, "n.age > 1000")
            .expect("filter should succeed");
        let hinted: Vec<_> = context
            .result_set
            .rows
            .iter()
            .map(|r| r.values.clone())
            .collect();

        let baseline = filter_with_threshold(&nodes, "n.age > 1000", usize::MAX);
        assert_eq!(hinted, baseline, "hint should not change output values");
    }

    #[test]
    fn filter_columnar_matches_row_path_on_10k_float_predicates() {
        let nodes: Vec<Value> = (0..10_000)
            .map(|i| build_person(i as u64, i as i64, i as f64 * 0.5))
            .collect();
        assert!(nodes.len() > 4096, "fixture must exceed columnar threshold");

        for predicate in [
            "n.score > 2500.0",
            "n.score >= 2500.0",
            "n.score < 2500.0",
            "n.score <= 2500.0",
            "n.score = 2500.0",
            "n.score <> 2500.0",
        ] {
            assert_parity(&nodes, predicate);
        }
    }
}
