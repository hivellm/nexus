//! Projection pipeline operators: `execute_project` (RETURN projection),
//! `execute_with` (WITH carry-over projection), `execute_limit`,
//! `execute_sort`, and the streaming `execute_top_k_sort` optimisation
//! plus its `get_following_limit` lookahead helper.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::push_with_row_cap;
use super::super::types::{Operator, ProjectionItem, ResultSet, Row};
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_project(
        &self,
        context: &mut ExecutionContext,
        items: &[ProjectionItem],
    ) -> Result<Vec<Row>> {
        // First check if Filter already ran and filtered out all rows
        // This MUST be checked first before any other processing
        let has_filter_marker = context
            .result_set
            .columns
            .iter()
            .any(|c| c == "__filtered__" || c == "__filter_created__");
        if has_filter_marker {
            // Filter already processed - if __filtered__, no rows should be returned
            // If __filter_created__, Filter already created the row
            if context
                .result_set
                .columns
                .iter()
                .any(|c| c == "__filtered__")
            {
                // Filter filtered out all rows, return empty result
                context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
                context.result_set.rows.clear();
                return Ok(vec![]);
            }
            // If __filter_created__, continue with existing rows (Filter already created them)
        }

        // Use existing result_set.rows if available (from UNWIND, Filter, etc), otherwise materialize from variables
        // CRITICAL FIX: In UNION context, always materialize from variables to ensure correct structure
        // The existing result_set.rows may have wrong column structure from previous operators
        let rows = if !context.result_set.rows.is_empty()
            && !context
                .result_set
                .columns
                .contains(&"__filtered__".to_string())
            && !context
                .result_set
                .columns
                .contains(&"__filter_created__".to_string())
        {
            // Use existing rows only if they don't have filter markers (indicating they are real data rows)
            let existing_columns = context.result_set.columns.clone();
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &existing_columns))
                .collect()
        } else {
            // Check if Filter already ran and removed all rows (marked with "__filtered__" column)
            let has_filter_marker = context
                .result_set
                .columns
                .iter()
                .any(|c| c == "__filtered__" || c == "__filter_created__");

            if has_filter_marker && context.result_set.rows.is_empty() {
                // Filter already processed and removed all rows, don't create new ones
                vec![]
            } else {
                let materialized = self.materialize_rows_from_variables(context);

                // CRITICAL FIX: If we have variables but materialized is empty,
                // check if variables contain empty arrays (MATCH found nothing)
                // vs single values (after MATCH with filter)
                if materialized.is_empty() && !context.variables.is_empty() {
                    // Check if all variables are empty arrays - if so, no rows should be created
                    let all_empty_arrays = context.variables.values().all(|v| {
                        match v {
                            Value::Array(arr) => arr.is_empty(),
                            _ => false, // Non-array values should create a row
                        }
                    });

                    if all_empty_arrays {
                        // All variables are empty arrays (MATCH found nothing) - return empty
                        vec![]
                    } else {
                        // CRITICAL FIX: If materialized is empty but we have non-empty arrays,
                        // there might be arrays with multiple elements that materialize_rows_from_variables
                        // should have handled. Let's check if we have multi-element arrays:
                        let has_multi_element_arrays =
                            context.variables.values().any(|v| match v {
                                Value::Array(arr) => arr.len() > 1,
                                _ => false,
                            });

                        if has_multi_element_arrays {
                            // We have multi-element arrays - materialize_rows_from_variables should have
                            // created rows from them. If it didn't, there's a bug. But let's try again
                            // in case variables changed:
                            let retry_materialized = self.materialize_rows_from_variables(context);
                            if !retry_materialized.is_empty() {
                                tracing::trace!(
                                    "Project: retry materialization succeeded, got {} rows",
                                    retry_materialized.len()
                                );
                                retry_materialized
                            } else {
                                // Still empty - this suggests a bug in materialize_rows_from_variables
                                // or the variables don't match what we expect
                                tracing::warn!(
                                    "Project: materialize_rows_from_variables returned empty despite multi-element arrays"
                                );
                                // Return empty - this will cause the query to return no rows
                                vec![]
                            }
                        } else {
                            // Some variables contain single values, create a row
                            let mut row = HashMap::new();
                            for (var, value) in &context.variables {
                                match value {
                                    Value::Array(arr) if arr.len() == 1 => {
                                        row.insert(var.clone(), arr[0].clone());
                                    }
                                    Value::Array(_) => {
                                        // Empty or multiple-element array - skip
                                        // (multi-element arrays should be handled above)
                                    }
                                    _ => {
                                        row.insert(var.clone(), value.clone());
                                    }
                                }
                            }
                            if !row.is_empty() {
                                vec![row]
                            } else {
                                materialized
                            }
                        }
                    }
                } else if materialized.is_empty()
                    && context.variables.is_empty()
                    && !items.is_empty()
                {
                    // Check if ALL projection items can be evaluated without variables
                    // Only create a row if ALL items are literals/constants (like RETURN 1+1)
                    // If ANY item requires variables (like RETURN a), don't create a row
                    if items
                        .iter()
                        .all(|item| self.can_evaluate_without_variables(&item.expression))
                    {
                        // Create single empty row for expression evaluation (literals like 1+1)
                        vec![std::collections::HashMap::new()]
                    } else {
                        // Some expressions require variables but none exist - return empty (MATCH found nothing)
                        vec![]
                    }
                } else {
                    materialized
                }
            }
        };

        // Double-check filter marker before creating projected rows
        // This is a safety check in case rows were created despite filter marker
        let has_filter_marker_final = context
            .result_set
            .columns
            .iter()
            .any(|c| c == "__filtered__" || c == "__filter_created__");
        if has_filter_marker_final
            && context
                .result_set
                .columns
                .iter()
                .any(|c| c == "__filtered__")
        {
            // Filter filtered out all rows, return empty result
            context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
            context.result_set.rows.clear();
            return Ok(vec![]);
        }

        // Final safety check: if Filter marker exists, don't create any projected rows
        let has_filter_marker_before_projection = context
            .result_set
            .columns
            .iter()
            .any(|c| c == "__filtered__");
        if has_filter_marker_before_projection {
            // Filter filtered out all rows, return empty result
            context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
            context.result_set.rows.clear();
            return Ok(vec![]);
        }

        tracing::trace!(
            "Project: input_rows={}, items={:?}, result_set.rows={}, variables={:?}",
            rows.len(),
            items.iter().map(|i| i.alias.clone()).collect::<Vec<_>>(),
            context.result_set.rows.len(),
            context.variables.keys().collect::<Vec<_>>()
        );

        // DEBUG: Log variable contents for UNION context
        if rows.is_empty() && !context.variables.is_empty() {
            tracing::trace!("Project: DEBUG - No input rows, checking variables:");
            for (var, value) in &context.variables {
                match value {
                    Value::Array(arr) => {
                        tracing::trace!(
                            "Project: DEBUG - Variable '{}' has array with {} elements",
                            var,
                            arr.len()
                        );
                    }
                    _ => {
                        tracing::trace!(
                            "Project: DEBUG - Variable '{}' has non-array value: {:?}",
                            var,
                            value
                        );
                    }
                }
            }
        }

        let mut projected_rows = Vec::new();

        // CRITICAL FIX: Deduplicate rows before projecting, but preserve rows in these cases:
        // 1. When rows contain relationships (same node can appear with different relationships)
        // 2. When rows have different primitive values (e.g., after UNWIND creates multiple rows per node)
        use std::collections::HashSet;

        // Check if any rows contain relationships
        let has_relationships = rows.iter().any(|row_map| {
            row_map.values().any(|val| {
                if let Value::Object(obj) = val {
                    obj.get("type").is_some() // Relationships have "type" property
                } else {
                    false
                }
            })
        });

        // Check if any rows carry a non-node, non-relationship MAP
        // (e.g. the `s` status map emitted by `CALL { … } IN
        // TRANSACTIONS REPORT STATUS AS s`, or general user-built
        // maps). The legacy dedup keys those rows by `_nexus_id` —
        // which the status maps don't carry — so without this guard
        // multiple status rows collapse to one. See
        // phase6_opencypher-subquery-transactions §7 for the spec.
        let has_synthetic_maps = rows.iter().any(|row_map| {
            row_map.values().any(|val| {
                if let Value::Object(obj) = val {
                    !obj.contains_key("_nexus_id") && !obj.contains_key("type")
                } else {
                    false
                }
            })
        });

        // Check if rows have primitive values (non-object, non-array) that differ
        // This happens after UNWIND creates multiple rows with different values
        let has_varying_primitives = if rows.len() > 1 {
            // Collect all primitive values from each row
            let primitive_sets: Vec<Vec<String>> = rows
                .iter()
                .map(|row_map| {
                    row_map
                        .values()
                        .filter_map(|v| match v {
                            Value::Number(n) => Some(format!("num:{}", n)),
                            Value::String(s) => Some(format!("str:{}", s)),
                            Value::Bool(b) => Some(format!("bool:{}", b)),
                            _ => None,
                        })
                        .collect()
                })
                .collect();

            // Check if primitive values differ between rows
            if !primitive_sets.is_empty() {
                let first = &primitive_sets[0];
                primitive_sets.iter().skip(1).any(|set| set != first)
            } else {
                false
            }
        } else {
            false
        };

        let unique_rows = if has_relationships || has_varying_primitives || has_synthetic_maps {
            // CRITICAL: Don't deduplicate when:
            // 1. Rows contain relationships (same node with different relationships)
            // 2. Rows have different primitive values (e.g., from UNWIND)
            tracing::trace!(
                "Project: skipping deduplication (has_relationships={}, has_varying_primitives={}), preserving {} rows",
                has_relationships,
                has_varying_primitives,
                rows.len()
            );
            rows.clone()
        } else {
            // No relationships and no varying primitives - deduplicate by ROW COMBINATION
            // CRITICAL FIX: For multi-variable patterns like (a)->(b)->(c)->(a) (triangle),
            // we need to track the COMBINATION of all node IDs in a row, not individual IDs.
            // Each unique combination (a=1,b=2,c=3), (a=2,b=3,c=1), (a=3,b=1,c=2) is different!
            let mut seen_row_combinations = HashSet::new();
            let mut deduplicated_rows = Vec::new();

            for row_map in &rows {
                // Build a unique key for this row based on ALL node IDs with their variable names
                // Example: for row {a: node1, b: node2, c: node3} → key = "a:1_b:2_c:3"
                let mut var_ids: Vec<(String, u64)> = Vec::new();

                for (var_name, value) in row_map.iter() {
                    if let Value::Object(obj) = value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            if let Some(node_id) = id.as_u64() {
                                var_ids.push((var_name.clone(), node_id));
                            }
                        }
                    }
                }

                // Sort by variable name for consistent key generation
                var_ids.sort_by(|a, b| a.0.cmp(&b.0));

                // Build row combination key
                let row_key: String = var_ids
                    .iter()
                    .map(|(var, id)| format!("{}:{}", var, id))
                    .collect::<Vec<_>>()
                    .join("_");

                // Check if this exact combination was seen before
                let is_duplicate = !seen_row_combinations.insert(row_key.clone());

                if !is_duplicate {
                    deduplicated_rows.push(row_map.clone());
                } else {
                    tracing::trace!("Project: deduplicating row with key '{}'", row_key);
                }
            }

            tracing::trace!(
                "Project: deduplicated {} rows to {} unique rows (by row combination)",
                rows.len(),
                deduplicated_rows.len()
            );
            deduplicated_rows
        };

        // Process deduplicated rows
        for (idx, row_map) in unique_rows.iter().enumerate() {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let value =
                    self.evaluate_projection_expression(row_map, context, &item.expression)?;
                values.push(value);
            }
            projected_rows.push(Row { values });
            tracing::trace!(
                "Project: processed row {} of {}",
                idx + 1,
                unique_rows.len()
            );
        }

        tracing::trace!("Project: output_rows={}", projected_rows.len());

        context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
        context.result_set.rows = projected_rows.clone();
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);

        Ok(projected_rows)
    }

    /// Execute WITH clause operator
    ///
    /// WITH is like Project but it:
    /// 1. Evaluates expressions and creates new variables with aliased names
    /// 2. Replaces the current scope (old variables are removed)
    /// 3. Does NOT finalize result_set (that's what RETURN/Project does)
    pub(in crate::executor) fn execute_with(
        &self,
        context: &mut ExecutionContext,
        items: &[ProjectionItem],
        distinct: bool,
    ) -> Result<()> {
        tracing::trace!("execute_with: {} items, distinct={}", items.len(), distinct);

        // Materialize current rows from variables
        let rows = if !context.result_set.rows.is_empty() {
            let existing_columns = context.result_set.columns.clone();
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &existing_columns))
                .collect::<Vec<_>>()
        } else {
            self.materialize_rows_from_variables(context)
        };

        tracing::trace!("execute_with: processing {} input rows", rows.len());

        if rows.is_empty() {
            // No rows - nothing to project
            context.variables.clear();
            context.result_set.columns = items.iter().map(|item| item.alias.clone()).collect();
            context.result_set.rows.clear();
            return Ok(());
        }

        // Evaluate WITH items for each row and create new variables
        let mut new_rows: Vec<HashMap<String, Value>> = Vec::new();

        for row in &rows {
            let mut new_row = HashMap::new();

            // Use evaluate_projection_expression like execute_project does
            // This properly handles PropertyAccess (e.g., n.name) by looking up
            // the entity from the row HashMap first
            for item in items {
                let value = self.evaluate_projection_expression(row, context, &item.expression)?;
                new_row.insert(item.alias.clone(), value);
            }

            new_rows.push(new_row);
        }

        // Handle DISTINCT
        if distinct {
            let mut seen = std::collections::HashSet::new();
            new_rows.retain(|row| {
                let key = format!("{:?}", row);
                seen.insert(key)
            });
        }

        tracing::trace!("execute_with: produced {} output rows", new_rows.len());

        // Clear old variables and set new ones
        context.variables.clear();

        // Convert rows to context variables (each variable is an array of values from all rows)
        let columns: Vec<String> = items.iter().map(|item| item.alias.clone()).collect();

        for col in &columns {
            let values: Vec<Value> = new_rows
                .iter()
                .map(|row| row.get(col).cloned().unwrap_or(Value::Null))
                .collect();
            context.set_variable(col, Value::Array(values));
        }

        // Update result_set for subsequent operators
        context.result_set.columns = columns;
        context.result_set.rows = new_rows
            .iter()
            .map(|row_map| {
                let values: Vec<Value> = context
                    .result_set
                    .columns
                    .iter()
                    .map(|col| row_map.get(col).cloned().unwrap_or(Value::Null))
                    .collect();
                Row { values }
            })
            .collect();

        Ok(())
    }

    /// Execute Limit operator
    pub(in crate::executor) fn execute_limit(
        &self,
        context: &mut ExecutionContext,
        count: usize,
    ) -> Result<()> {
        if context.result_set.rows.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.len() > count {
            context.result_set.rows.truncate(count);
        }

        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }

    /// Execute Sort operator with LIMIT optimization (Phase 5)
    pub(in crate::executor) fn execute_sort(
        &self,
        context: &mut ExecutionContext,
        columns: &[String],
        ascending: &[bool],
    ) -> Result<()> {
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.is_empty() {
            return Ok(());
        }

        // Check if we have a LIMIT that follows this SORT (Phase 5 optimization)
        if let Some(limit) = self.get_following_limit(context) {
            // Use top-K sorting optimization for better performance with LIMIT
            self.execute_top_k_sort(context, columns, ascending, limit)?;
            return Ok(());
        }

        // Standard full sort for cases without LIMIT
        context.result_set.rows.sort_by(|a, b| {
            for (idx, column) in columns.iter().enumerate() {
                let col_idx = self
                    .get_column_index(column, &context.result_set.columns)
                    .unwrap_or(usize::MAX);
                if col_idx == usize::MAX {
                    continue;
                }
                let asc = ascending.get(idx).copied().unwrap_or(true);
                let left = a.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let right = b.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let ordering = cypher_null_aware_order(&left, &right, asc, |l, r| {
                    self.compare_values_for_sort(l, r)
                });
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            std::cmp::Ordering::Equal
        });

        // Don't rebuild rows after sort - it breaks the column order!
        // The rows are already sorted in place.
        Ok(())
    }

    /// Check if there's a LIMIT operator following the current sort in the plan
    pub(in crate::executor) fn get_following_limit(
        &self,
        context: &ExecutionContext,
    ) -> Option<usize> {
        // This is a simplified check. In a full implementation, we'd need access
        // to the remaining operators in the plan. For Phase 5 MVP, we check
        // if there's a limit stored in the context.

        // For now, return None to use full sort
        // Future: Check remaining operators and extract LIMIT value
        None
    }

    /// Execute top-K sorting optimization for LIMIT queries (Phase 5)
    ///
    /// Uses a binary heap to maintain only the top K results, avoiding
    /// full sort when K is much smaller than total results.
    pub(in crate::executor) fn execute_top_k_sort(
        &self,
        context: &mut ExecutionContext,
        columns: &[String],
        ascending: &[bool],
        k: usize,
    ) -> Result<()> {
        // For Phase 5 MVP, implement a simpler approach
        // Full top-K heap implementation would require custom Ord implementation
        // For now, sort all and take first K (still better than nothing for small K)

        // Sort all rows first
        context.result_set.rows.sort_by(|a, b| {
            for (idx, column) in columns.iter().enumerate() {
                let col_idx = self
                    .get_column_index(column, &context.result_set.columns)
                    .unwrap_or(usize::MAX);
                if col_idx == usize::MAX {
                    continue;
                }
                let asc = ascending.get(idx).copied().unwrap_or(true);
                let left = a.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let right = b.values.get(col_idx).cloned().unwrap_or(Value::Null);
                let ordering = cypher_null_aware_order(&left, &right, asc, |l, r| {
                    self.compare_values_for_sort(l, r)
                });
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            std::cmp::Ordering::Equal
        });

        // Take only first K rows
        context.result_set.rows.truncate(k);
        Ok(())
    }
}

/// phase6 §7 — openCypher null-positioning for ORDER BY. Returns the
/// FINAL sort ordering (no further reverse should be applied by the
/// caller), combining both the null-positioning rule and the ASC/DESC
/// direction:
/// * ASC  → non-null values in natural order, NULLs sort LAST
/// * DESC → non-null values in reversed order, NULLs sort FIRST
///
/// The base comparator (`compare_values_for_sort`) treats null as
/// less-than everything — that contract is correct for predicate
/// evaluation (`<`, `>`) but opposite of what openCypher wants for
/// ORDER BY, so we special-case the null comparisons here instead of
/// changing the base comparator.
fn cypher_null_aware_order<F>(
    left: &Value,
    right: &Value,
    ascending: bool,
    base_cmp: F,
) -> std::cmp::Ordering
where
    F: FnOnce(&Value, &Value) -> std::cmp::Ordering,
{
    match (left.is_null(), right.is_null()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => {
            // ASC: null sorts last (Greater). DESC: null sorts first (Less).
            if ascending {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            }
        }
        (false, true) => {
            if ascending {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        }
        (false, false) => {
            let base = base_cmp(left, right);
            if ascending { base } else { base.reverse() }
        }
    }
}
