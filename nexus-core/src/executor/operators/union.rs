//! UNION / UNION ALL operator. Executes left and right pipelines against
//! fresh `ExecutionContext`s, then concatenates (or deduplicates) their
//! result sets into the caller's context.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::push_with_row_cap;
use super::super::types::{Operator, Row};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_union(
        &self,
        context: &mut ExecutionContext,
        left: &[Operator],
        right: &[Operator],
        distinct: bool,
    ) -> Result<()> {
        // Execute left operator pipeline and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone(), context.cache.clone());
        for (idx, operator) in left.iter().enumerate() {
            tracing::debug!(
                "UNION: executing left operator {}/{}: {:?}",
                idx + 1,
                left.len(),
                operator
            );
            self.execute_operator(&mut left_context, operator)?;
            tracing::debug!(
                "UNION: after left operator {}, result_set.rows={}, columns={:?}, variables={:?}",
                idx + 1,
                left_context.result_set.rows.len(),
                left_context.result_set.columns,
                left_context.variables.keys().collect::<Vec<_>>()
            );
        }

        tracing::debug!(
            "UNION: left side completed - result_set.rows={}, columns={:?}",
            left_context.result_set.rows.len(),
            left_context.result_set.columns
        );

        // Execute right operator pipeline and collect its results
        let mut right_context =
            ExecutionContext::new(context.params.clone(), context.cache.clone());
        for (idx, operator) in right.iter().enumerate() {
            tracing::debug!(
                "UNION: executing right operator {}/{}: {:?}",
                idx + 1,
                right.len(),
                operator
            );
            self.execute_operator(&mut right_context, operator)?;
            tracing::debug!(
                "UNION: after right operator {}, result_set.rows={}, columns={:?}, variables={:?}",
                idx + 1,
                right_context.result_set.rows.len(),
                right_context.result_set.columns,
                right_context.variables.keys().collect::<Vec<_>>()
            );
        }

        tracing::debug!(
            "UNION: right side completed - result_set.rows={}, columns={:?}",
            right_context.result_set.rows.len(),
            right_context.result_set.columns
        );

        // Combine results from both sides
        // Ensure results are in result_set.rows (some operators may store in variables)
        // Convert variable-based results to rows if needed
        // CRITICAL FIX: Project operator should populate result_set.rows, but if it's empty,
        // we need to materialize from variables to ensure all rows are collected for UNION
        // However, we should NOT materialize if variables only contain empty arrays (no matches found)
        if left_context.result_set.rows.is_empty() && !left_context.variables.is_empty() {
            // Check if any variable has non-empty array - if all are empty, don't materialize
            let has_non_empty_array = left_context.variables.values().any(|v| {
                match v {
                    Value::Array(arr) => !arr.is_empty(),
                    _ => true, // Non-array values should be materialized
                }
            });

            if has_non_empty_array {
                // If no rows but we have variables with data, materialize from variables
                let row_maps = self.materialize_rows_from_variables(&left_context);
                if !row_maps.is_empty() {
                    // Ensure columns are set from variables if not already set
                    if left_context.result_set.columns.is_empty() {
                        let mut columns: Vec<String> = row_maps[0].keys().cloned().collect();
                        columns.sort();
                        left_context.result_set.columns = columns;
                    }
                    self.update_result_set_from_rows(&mut left_context, &row_maps);
                }
            }
            // If all arrays are empty (no matches found), leave result_set.rows empty
        }

        if right_context.result_set.rows.is_empty() && !right_context.variables.is_empty() {
            // Check if any variable has non-empty array - if all are empty, don't materialize
            let has_non_empty_array = right_context.variables.values().any(|v| {
                match v {
                    Value::Array(arr) => !arr.is_empty(),
                    _ => true, // Non-array values should be materialized
                }
            });

            if has_non_empty_array {
                // If no rows but we have variables with data, materialize from variables
                let row_maps = self.materialize_rows_from_variables(&right_context);
                if !row_maps.is_empty() {
                    // Ensure columns are set from variables if not already set
                    if right_context.result_set.columns.is_empty() {
                        let mut columns: Vec<String> = row_maps[0].keys().cloned().collect();
                        columns.sort();
                        right_context.result_set.columns = columns;
                    }
                    self.update_result_set_from_rows(&mut right_context, &row_maps);
                }
            }
            // If all arrays are empty (no matches found), leave result_set.rows empty
        }

        // CRITICAL FIX: Ensure columns are set from result_set.rows if Project already executed
        // Project should have set columns, but verify they match the row structure
        if !left_context.result_set.rows.is_empty() && !left_context.result_set.columns.is_empty() {
            // Verify column count matches row value count
            if let Some(first_row) = left_context.result_set.rows.first() {
                if first_row.values.len() != left_context.result_set.columns.len() {
                    // Mismatch - this shouldn't happen, but log it
                    tracing::warn!(
                        "UNION: Left side column/row mismatch: {} cols, {} values",
                        left_context.result_set.columns.len(),
                        first_row.values.len()
                    );
                }
            }
        }

        if !right_context.result_set.rows.is_empty() && !right_context.result_set.columns.is_empty()
        {
            if let Some(first_row) = right_context.result_set.rows.first() {
                if first_row.values.len() != right_context.result_set.columns.len() {
                    tracing::warn!(
                        "UNION: Right side column/row mismatch: {} cols, {} values",
                        right_context.result_set.columns.len(),
                        first_row.values.len()
                    );
                }
            }
        }

        // Ensure both sides have the same columns (UNION requires matching column structure)
        // UNION requires that both sides have the same number of columns with compatible types
        // Priority: left columns > right columns > columns from RETURN items
        let columns = if !left_context.result_set.columns.is_empty() {
            left_context.result_set.columns.clone()
        } else if !right_context.result_set.columns.is_empty() {
            right_context.result_set.columns.clone()
        } else {
            // If both sides are empty, try to get columns from variables or result set rows
            // First try to get from left side variables
            let left_row_maps = self.materialize_rows_from_variables(&left_context);
            let right_row_maps = self.materialize_rows_from_variables(&right_context);

            // Get columns from row maps if available
            let mut all_columns = std::collections::HashSet::new();
            if !left_row_maps.is_empty() {
                all_columns.extend(left_row_maps[0].keys().cloned());
            }
            if !right_row_maps.is_empty() {
                all_columns.extend(right_row_maps[0].keys().cloned());
            }

            // If still empty, try variables
            if all_columns.is_empty() {
                for (var, _) in &left_context.variables {
                    all_columns.insert(var.clone());
                }
                for (var, _) in &right_context.variables {
                    all_columns.insert(var.clone());
                }
            }

            let mut cols: Vec<String> = all_columns.into_iter().collect();
            cols.sort();
            cols
        };

        // Normalize rows from both sides to use the same column order
        // CRITICAL FIX: If columns are empty but rows exist, use row order directly
        let mut left_rows = Vec::new();
        tracing::debug!(
            "UNION: left side - result_set.rows={}, columns={:?}, left_context.columns={:?}",
            left_context.result_set.rows.len(),
            columns,
            left_context.result_set.columns
        );

        if left_context.result_set.columns.is_empty() && !left_context.result_set.rows.is_empty() {
            // No columns defined - use row values as-is (shouldn't happen if Project ran correctly)
            tracing::debug!("UNION: left side has no columns, using row values as-is");
            for row in &left_context.result_set.rows {
                left_rows.push(row.clone());
            }
        } else {
            for (row_idx, row) in left_context.result_set.rows.iter().enumerate() {
                let mut normalized_values = Vec::new();
                for col in &columns {
                    if let Some(idx) = left_context
                        .result_set
                        .columns
                        .iter()
                        .position(|c| c == col)
                    {
                        if idx < row.values.len() {
                            normalized_values.push(row.values[idx].clone());
                        } else {
                            normalized_values.push(Value::Null);
                        }
                    } else {
                        normalized_values.push(Value::Null);
                    }
                }
                tracing::debug!(
                    "UNION: left row {} normalized: {:?}",
                    row_idx,
                    normalized_values
                );
                left_rows.push(Row {
                    values: normalized_values,
                });
            }
        }

        tracing::debug!("UNION: left_rows after normalization: {}", left_rows.len());

        let mut right_rows = Vec::new();
        tracing::debug!(
            "UNION: right side - result_set.rows={}, columns={:?}, right_context.columns={:?}",
            right_context.result_set.rows.len(),
            columns,
            right_context.result_set.columns
        );

        if right_context.result_set.columns.is_empty() && !right_context.result_set.rows.is_empty()
        {
            // No columns defined - use row values as-is (shouldn't happen if Project ran correctly)
            tracing::debug!("UNION: right side has no columns, using row values as-is");
            for row in &right_context.result_set.rows {
                right_rows.push(row.clone());
            }
        } else {
            for (row_idx, row) in right_context.result_set.rows.iter().enumerate() {
                let mut normalized_values = Vec::new();
                for col in &columns {
                    if let Some(idx) = right_context
                        .result_set
                        .columns
                        .iter()
                        .position(|c| c == col)
                    {
                        if idx < row.values.len() {
                            normalized_values.push(row.values[idx].clone());
                        } else {
                            normalized_values.push(Value::Null);
                        }
                    } else {
                        normalized_values.push(Value::Null);
                    }
                }
                tracing::debug!(
                    "UNION: right row {} normalized: {:?}",
                    row_idx,
                    normalized_values
                );
                right_rows.push(Row {
                    values: normalized_values,
                });
            }
        }

        tracing::debug!(
            "UNION: right_rows after normalization: {}",
            right_rows.len()
        );

        tracing::debug!(
            "UNION: left_rows={}, right_rows={}, columns={:?}",
            left_rows.len(),
            right_rows.len(),
            columns
        );

        let mut combined_rows = Vec::new();
        combined_rows.extend(left_rows);
        combined_rows.extend(right_rows);

        tracing::debug!(
            "UNION: combined_rows before dedup={}, distinct={}",
            combined_rows.len(),
            distinct
        );

        // If UNION (not UNION ALL), deduplicate results
        if distinct {
            let mut seen = std::collections::HashSet::new();
            let mut deduped_rows = Vec::new();

            for row in combined_rows {
                // Serialize row values to a canonical JSON string for comparison.
                // Phase2: propagate serialisation failures rather than deduping all
                // failing rows into the empty-string bucket. Callers running
                // `UNION` (not `UNION ALL`) over a column with non-finite floats
                // now see a clear error.
                let row_key = match serde_json::to_string(&row.values) {
                    Ok(s) => s,
                    Err(e) => {
                        super::super::serde_metrics::record_propagated_failure(
                            super::super::serde_metrics::SerdeFallbackSite::UnionDedupKey,
                        );
                        return Err(Error::CypherExecution(format!(
                            "UNION dedup key serialization failed ({}). Use UNION ALL to skip \
                             deduplication when rows contain non-JSON-representable values.",
                            e
                        )));
                    }
                };
                if seen.insert(row_key.clone()) {
                    deduped_rows.push(row);
                } else {
                    tracing::debug!("UNION: duplicate row removed: {}", row_key);
                }
            }
            combined_rows = deduped_rows;
            tracing::debug!("UNION: deduped_rows={}", combined_rows.len());
        }

        // Update the main context with combined results
        context.set_columns_and_rows(columns, combined_rows);
        tracing::debug!(
            "UNION: final result_set.rows={}",
            context.result_set.rows.len()
        );
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }
}
