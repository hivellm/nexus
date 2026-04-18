//! Join operators: hash join, merge join, nested-loop join, plus the
//! adaptive join heuristics (size thresholds, sorted-on-key detection)
//! and the columnar advanced-relationship-join entry point. Also
//! includes `execute_distinct` and `execute_index_scan` which share
//! the join/filter helpers.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::push_with_row_cap;
use super::super::types::{Direction, IndexType, JoinType, Operator, ResultSet, Row};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_join(
        &self,
        context: &mut ExecutionContext,
        left: &Operator,
        right: &Operator,
        join_type: JoinType,
        condition: Option<&str>,
    ) -> Result<()> {
        // Execute left operator and collect its results
        let mut left_context = ExecutionContext::new(context.params.clone(), context.cache.clone());
        self.execute_operator(&mut left_context, left)?;

        // Execute right operator and collect its results
        let mut right_context =
            ExecutionContext::new(context.params.clone(), context.cache.clone());
        self.execute_operator(&mut right_context, right)?;

        // Try advanced join algorithms first (only for larger datasets)
        let left_size = left_context.result_set.rows.len();
        let right_size = right_context.result_set.rows.len();

        // Only use advanced joins for datasets large enough to benefit from optimization
        // Minimum threshold: configurable via executor config to justify columnar overhead
        if self.config.enable_vectorized_execution
            && left_size >= self.config.vectorized_threshold
            && right_size >= self.config.vectorized_threshold
        {
            if let Ok(result) = self.try_advanced_relationship_join(
                &left_context.result_set,
                &right_context.result_set,
                join_type,
                condition,
            ) {
                tracing::info!(
                    "🚀 ADVANCED JOIN: Used optimized join algorithm ({}x{} rows)",
                    left_size,
                    right_size
                );
                context.result_set = result;
                let row_maps = self.result_set_as_rows(context);
                self.update_variables_from_rows(context, &row_maps);
                return Ok(());
            }
        }

        // Fallback to traditional nested loop join
        tracing::debug!("Advanced join failed, falling back to nested loop join");
        self.execute_nested_loop_join(
            context,
            &left_context,
            &right_context,
            join_type,
            condition,
        )?;
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);

        Ok(())
    }

    /// Check if two rows match based on join condition
    pub(in crate::executor) fn rows_match(
        &self,
        left_row: &Row,
        right_row: &Row,
        condition: Option<&str>,
    ) -> Result<bool> {
        match condition {
            Some(_cond) => {
                // For now, implement simple equality matching
                // In a full implementation, this would parse and evaluate the condition
                if left_row.values.len() != right_row.values.len() {
                    return Ok(false);
                }

                for (left_val, right_val) in left_row.values.iter().zip(right_row.values.iter()) {
                    if left_val != right_val {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            None => {
                // No condition means all rows match (Cartesian product)
                Ok(true)
            }
        }
    }

    /// Execute IndexScan operator
    pub(in crate::executor) fn execute_index_scan(
        &self,
        context: &mut ExecutionContext,
        index_type: IndexType,
        key: &str,
        variable: &str,
    ) -> Result<()> {
        let mut results = Vec::new();

        match index_type {
            IndexType::Label => {
                // Scan label index for nodes with the given label
                if let Ok(label_id) = self.catalog().get_or_create_label(key) {
                    let nodes = self.execute_node_by_label(label_id)?;
                    results.extend(nodes);
                }
            }
            IndexType::Property => {
                // Scan property index for nodes with the given property value
                // For now, implement a simple property lookup
                // In a full implementation, this would use the property index
                let nodes = self.execute_node_by_label(0)?; // Get all nodes
                for node in nodes {
                    if let Some(properties) = node.get("properties") {
                        if properties.is_object() {
                            let mut found = false;
                            for (prop_key, prop_value) in properties.as_object().unwrap() {
                                if prop_key == key || (prop_value.as_str() == Some(key)) {
                                    found = true;
                                    break;
                                }
                            }
                            if found {
                                results.push(node);
                            }
                        }
                    }
                }
            }
            IndexType::Vector => {
                // Scan vector index for similar vectors
                // For now, return empty results as vector search requires specific implementation
                // In a full implementation, this would use the KNN index
                results = Vec::new();
            }
            IndexType::Spatial => {
                // Scan spatial index for points within distance or bounding box
                // For now, return empty results - spatial index queries require specific implementation
                // In a full implementation, this would use the spatial index (R-tree)
                // to find points within a given distance or bounding box
                // The planner should detect distance() or withinDistance() calls in WHERE clauses
                // and use this index type for optimization
                results = Vec::new();
            }
            IndexType::FullText => {
                // Scan full-text index for text matches
                // For now, implement a simple text search in properties
                let nodes = self.execute_node_by_label(0)?; // Get all nodes
                for node in nodes {
                    if let Some(properties) = node.get("properties") {
                        if properties.is_object() {
                            let mut found = false;
                            for (_, prop_value) in properties.as_object().unwrap() {
                                if prop_value.is_string() {
                                    let text = prop_value.as_str().unwrap().to_lowercase();
                                    if text.contains(&key.to_lowercase()) {
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if found {
                                results.push(node);
                            }
                        }
                    }
                }
            }
        }

        // Set the results in the context
        context.set_variable(variable, Value::Array(results));
        let rows = self.materialize_rows_from_variables(context);
        self.update_result_set_from_rows(context, &rows);

        Ok(())
    }

    /// Try advanced join algorithms (Hash Join, Merge Join)
    pub(in crate::executor) fn try_advanced_relationship_join(
        &self,
        left_result: &ResultSet,
        right_result: &ResultSet,
        join_type: JoinType,
        condition: Option<&str>,
    ) -> Result<ResultSet> {
        let left_size = left_result.rows.len();
        let right_size = right_result.rows.len();

        // For small datasets, nested loop is often faster due to overhead
        if left_size < 10 || right_size < 10 {
            return Err(Error::Internal(
                "Dataset too small for advanced joins".to_string(),
            ));
        }

        // Parse join condition to extract join keys
        let (left_key_idx, right_key_idx) = if let Some(cond) = condition {
            self.parse_join_condition(cond)?
        } else {
            // Default: join on first column if no condition specified
            (0, 0)
        };

        // Choose algorithm based on data characteristics
        if self.should_use_hash_join(left_size, right_size) {
            self.execute_hash_join(
                left_result,
                right_result,
                join_type,
                left_key_idx,
                right_key_idx,
            )
        } else if self.should_use_merge_join(left_result, right_result, left_key_idx, right_key_idx)
        {
            self.execute_merge_join(
                left_result,
                right_result,
                join_type,
                left_key_idx,
                right_key_idx,
            )
        } else {
            Err(Error::Internal(
                "No suitable advanced join algorithm found".to_string(),
            ))
        }
    }

    /// Determine if Hash Join should be used
    pub(in crate::executor) fn should_use_hash_join(
        &self,
        left_size: usize,
        right_size: usize,
    ) -> bool {
        // Hash join is good when one side fits in memory and the other is larger
        // Use a heuristic: if smaller side is < 1000 rows, hash join is usually better
        left_size.min(right_size) < 1000
    }

    /// Determine if Merge Join should be used
    pub(in crate::executor) fn should_use_merge_join(
        &self,
        left_result: &ResultSet,
        right_result: &ResultSet,
        left_key_idx: usize,
        right_key_idx: usize,
    ) -> bool {
        // Merge join requires sorted data
        // Check if both sides are already sorted on the join key
        self.is_sorted_on_key(left_result, left_key_idx)
            && self.is_sorted_on_key(right_result, right_key_idx)
    }

    /// Check if a result set is sorted on a given column index
    pub(in crate::executor) fn is_sorted_on_key(&self, result: &ResultSet, key_idx: usize) -> bool {
        if result.rows.is_empty() || key_idx >= result.rows[0].values.len() {
            return false;
        }

        for i in 1..result.rows.len() {
            let prev_val = &result.rows[i - 1].values[key_idx];
            let curr_val = &result.rows[i].values[key_idx];

            match (prev_val, curr_val) {
                (Value::Number(a), Value::Number(b)) => {
                    if a.as_f64().unwrap_or(0.0) > b.as_f64().unwrap_or(0.0) {
                        return false;
                    }
                }
                (Value::String(a), Value::String(b)) => {
                    if a > b {
                        return false;
                    }
                }
                _ => return false, // Unsupported comparison
            }
        }
        true
    }

    /// Parse join condition to extract column indices
    pub(in crate::executor) fn parse_join_condition(
        &self,
        condition: &str,
    ) -> Result<(usize, usize)> {
        // Simple parsing for conditions like "n.id = m.id" or "left.id = right.id"
        // For now, assume first column of each side
        Ok((0, 0))
    }

    /// Execute Hash Join algorithm
    pub(in crate::executor) fn execute_hash_join(
        &self,
        left_result: &ResultSet,
        right_result: &ResultSet,
        join_type: JoinType,
        left_key_idx: usize,
        right_key_idx: usize,
    ) -> Result<ResultSet> {
        use std::collections::HashMap;

        // Build hash table from smaller dataset
        let (build_side, probe_side, build_key_idx, probe_key_idx, swap_sides) =
            if left_result.rows.len() <= right_result.rows.len() {
                (
                    left_result,
                    right_result,
                    left_key_idx,
                    right_key_idx,
                    false,
                )
            } else {
                (right_result, left_result, right_key_idx, left_key_idx, true)
            };

        let mut hash_table: HashMap<String, Vec<&Row>> = HashMap::new();

        // Build phase
        for row in &build_side.rows {
            if build_key_idx < row.values.len() {
                let key = self.row_value_to_key(&row.values[build_key_idx]);
                hash_table.entry(key).or_insert_with(Vec::new).push(row);
            }
        }

        let mut result_rows = Vec::new();

        // Probe phase
        match join_type {
            JoinType::Inner => {
                for probe_row in &probe_side.rows {
                    if probe_key_idx < probe_row.values.len() {
                        let key = self.row_value_to_key(&probe_row.values[probe_key_idx]);
                        if let Some(build_rows) = hash_table.get(&key) {
                            for build_row in build_rows {
                                let (left_row, right_row) = if swap_sides {
                                    (probe_row, *build_row)
                                } else {
                                    (*build_row, probe_row)
                                };
                                let mut combined_row = left_row.values.clone();
                                combined_row.extend(right_row.values.clone());
                                result_rows.push(Row {
                                    values: combined_row,
                                });
                            }
                        }
                    }
                }
            }
            _ => {
                // For outer joins, we'd need more complex logic with tracking matched rows
                // For now, fall back to nested loop
                return Err(Error::Internal(
                    "Outer joins not yet implemented for hash join".to_string(),
                ));
            }
        }

        // Combine column names
        let mut result_columns = if swap_sides {
            right_result.columns.clone()
        } else {
            left_result.columns.clone()
        };
        result_columns.extend(if swap_sides {
            left_result.columns.clone()
        } else {
            right_result.columns.clone()
        });

        Ok(ResultSet {
            columns: result_columns,
            rows: result_rows,
        })
    }

    /// Execute Merge Join algorithm
    pub(in crate::executor) fn execute_merge_join(
        &self,
        left_result: &ResultSet,
        right_result: &ResultSet,
        join_type: JoinType,
        left_key_idx: usize,
        right_key_idx: usize,
    ) -> Result<ResultSet> {
        let mut result_rows = Vec::new();
        let mut left_idx = 0;
        let mut right_idx = 0;

        // Only implement inner join for merge join initially
        if join_type != JoinType::Inner {
            return Err(Error::Internal(
                "Only inner joins supported for merge join".to_string(),
            ));
        }

        while left_idx < left_result.rows.len() && right_idx < right_result.rows.len() {
            let left_val = &left_result.rows[left_idx].values[left_key_idx];
            let right_val = &right_result.rows[right_idx].values[right_key_idx];

            match self.compare_values_for_ordering(left_val, right_val) {
                std::cmp::Ordering::Less => {
                    left_idx += 1;
                }
                std::cmp::Ordering::Greater => {
                    right_idx += 1;
                }
                std::cmp::Ordering::Equal => {
                    // Found match, collect all matching rows from both sides
                    let start_left = left_idx;
                    let start_right = right_idx;

                    // Advance through equal values on left side
                    while left_idx < left_result.rows.len()
                        && self.compare_values_for_ordering(
                            &left_result.rows[left_idx].values[left_key_idx],
                            left_val,
                        ) == std::cmp::Ordering::Equal
                    {
                        left_idx += 1;
                    }

                    // Advance through equal values on right side
                    while right_idx < right_result.rows.len()
                        && self.compare_values_for_ordering(
                            &right_result.rows[right_idx].values[right_key_idx],
                            right_val,
                        ) == std::cmp::Ordering::Equal
                    {
                        right_idx += 1;
                    }

                    // Cross product of matching ranges
                    for l in start_left..left_idx {
                        for r in start_right..right_idx {
                            let mut combined_row = left_result.rows[l].values.clone();
                            combined_row.extend(right_result.rows[r].values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                        }
                    }
                }
            }
        }

        // Combine column names
        let mut result_columns = left_result.columns.clone();
        result_columns.extend(right_result.columns.clone());

        Ok(ResultSet {
            columns: result_columns,
            rows: result_rows,
        })
    }

    /// Convert row value to hash key
    pub(in crate::executor) fn row_value_to_key(&self, value: &Value) -> String {
        match value {
            Value::Number(n) => format!("{}", n),
            Value::String(s) => s.clone(),
            Value::Bool(b) => format!("{}", b),
            _ => "".to_string(),
        }
    }

    /// Compare two values for merge join
    pub(in crate::executor) fn compare_values_for_ordering(
        &self,
        a: &Value,
        b: &Value,
    ) -> std::cmp::Ordering {
        match (a, b) {
            (Value::Number(x), Value::Number(y)) => x
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&y.as_f64().unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal),
            (Value::String(x), Value::String(y)) => x.cmp(y),
            _ => std::cmp::Ordering::Equal,
        }
    }

    /// Fallback nested loop join implementation
    pub(in crate::executor) fn execute_nested_loop_join(
        &self,
        context: &mut ExecutionContext,
        left_context: &ExecutionContext,
        right_context: &ExecutionContext,
        join_type: JoinType,
        condition: Option<&str>,
    ) -> Result<()> {
        let mut result_rows = Vec::new();

        // Perform the join based on type
        match join_type {
            JoinType::Inner => {
                // Inner join: only rows that match in both sides
                for left_row in &left_context.result_set.rows {
                    for right_row in &right_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                        }
                    }
                }
            }
            JoinType::LeftOuter => {
                // Left outer join: all left rows, matched right rows where possible
                for left_row in &left_context.result_set.rows {
                    let mut matched = false;
                    for right_row in &right_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            matched = true;
                        }
                    }
                    if !matched {
                        // Add left row with null values for right side
                        let mut combined_row = left_row.values.clone();
                        combined_row.extend(vec![
                            serde_json::Value::Null;
                            right_context.result_set.columns.len()
                        ]);
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
            JoinType::RightOuter => {
                // Right outer join: all right rows, matched left rows where possible
                for right_row in &right_context.result_set.rows {
                    let mut matched = false;
                    for left_row in &left_context.result_set.rows {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            matched = true;
                        }
                    }
                    if !matched {
                        // Add right row with null values for left side
                        let mut combined_row =
                            vec![serde_json::Value::Null; left_context.result_set.columns.len()];
                        combined_row.extend(right_row.values.clone());
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
            JoinType::FullOuter => {
                // Full outer join: all rows from both sides
                let mut left_matched = vec![false; left_context.result_set.rows.len()];
                let mut right_matched = vec![false; right_context.result_set.rows.len()];

                for (i, left_row) in left_context.result_set.rows.iter().enumerate() {
                    for (j, right_row) in right_context.result_set.rows.iter().enumerate() {
                        if self.rows_match(left_row, right_row, condition)? {
                            let mut combined_row = left_row.values.clone();
                            combined_row.extend(right_row.values.clone());
                            result_rows.push(Row {
                                values: combined_row,
                            });
                            left_matched[i] = true;
                            right_matched[j] = true;
                        }
                    }
                }

                // Add unmatched left rows
                for (i, left_row) in left_context.result_set.rows.iter().enumerate() {
                    if !left_matched[i] {
                        let mut combined_row = left_row.values.clone();
                        combined_row.extend(vec![
                            serde_json::Value::Null;
                            right_context.result_set.columns.len()
                        ]);
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }

                // Add unmatched right rows
                for (j, right_row) in right_context.result_set.rows.iter().enumerate() {
                    if !right_matched[j] {
                        let mut combined_row =
                            vec![serde_json::Value::Null; left_context.result_set.columns.len()];
                        combined_row.extend(right_row.values.clone());
                        result_rows.push(Row {
                            values: combined_row,
                        });
                    }
                }
            }
        }

        // Update context with joined results
        context.result_set.rows = result_rows;

        // Combine column names
        let mut combined_columns = left_context.result_set.columns.clone();
        combined_columns.extend(right_context.result_set.columns.clone());
        context.result_set.columns = combined_columns;

        Ok(())
    }
    /// Execute Distinct operator
    pub(in crate::executor) fn execute_distinct(
        &self,
        context: &mut ExecutionContext,
        columns: &[String],
    ) -> Result<()> {
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        if context.result_set.rows.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            "DISTINCT: input_rows={}, columns={:?}, distinct_columns={:?}",
            context.result_set.rows.len(),
            context.result_set.columns,
            columns
        );

        // Use a more robust comparison method that handles NULL correctly
        // Create a key from the values that can be used for comparison
        let mut seen = std::collections::HashSet::new();
        let mut distinct_rows = Vec::new();

        for (idx, row) in context.result_set.rows.iter().enumerate() {
            let mut key_values = Vec::new();
            if columns.is_empty() {
                // DISTINCT on all columns
                key_values = row.values.clone();
            } else {
                // DISTINCT on specific columns
                for column in columns {
                    if let Some(index) = self.get_column_index(column, &context.result_set.columns)
                    {
                        if index < row.values.len() {
                            key_values.push(row.values[index].clone());
                        } else {
                            key_values.push(Value::Null);
                        }
                    } else {
                        key_values.push(Value::Null);
                    }
                }
            }

            // Create a canonical key for comparison
            // Use JSON serialization with sorted keys for objects to ensure consistent comparison
            // This handles NULL, numbers, strings, arrays, objects correctly
            // For consistent comparison, we need to ensure the same value always produces the same key
            let key = serde_json::to_string(&key_values).unwrap_or_default();

            tracing::debug!(
                "DISTINCT: row {} key={}, key_values={:?}",
                idx,
                key,
                key_values
            );

            // Only add row if we haven't seen this key before
            if seen.insert(key.clone()) {
                distinct_rows.push(row.clone());
            } else {
                tracing::debug!("DISTINCT: duplicate row {} removed (key={})", idx, key);
            }
        }

        tracing::debug!(
            "DISTINCT: output_rows={} (filtered {} duplicates)",
            distinct_rows.len(),
            context.result_set.rows.len() - distinct_rows.len()
        );

        context.result_set.rows = distinct_rows.clone();
        let row_maps = self.result_set_as_rows(context);
        self.update_variables_from_rows(context, &row_maps);
        Ok(())
    }
}
