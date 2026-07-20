//! Evaluation helpers that sit between the operator layer and the
//! row-level evaluator. Includes Cartesian-product application,
//! row↔variable materialisation, EXISTS-style pattern checks, entity
//! ID extraction, relationship value serialisation, and the context
//! expression evaluator used before operators have materialised rows.

use super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::engine::Executor;
use super::super::parser;
use super::super::push_with_row_cap;
use super::super::types::{Direction, Row};
use crate::storage::RecordStore;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn evaluate_expression_in_context(
        &self,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<Value> {
        // Fast path — literals & unadorned variables avoid the full
        // projection evaluator's row-level setup. Everything else
        // (LIST / MAP literals, FunctionCall, BinaryOp, nested
        // procedure arguments such as `apoc.coll.union([1,2],[3,4])`
        // or `apoc.map.merge({a:1}, {b:2})`) routes through
        // `evaluate_projection_expression` with an empty row, which
        // is the same evaluator RETURN / WITH / WHERE clauses use.
        match expr {
            parser::Expression::Literal(parser::Literal::String(s)) => Ok(Value::String(s.clone())),
            parser::Expression::Literal(parser::Literal::Integer(i)) => {
                Ok(Value::Number((*i).into()))
            }
            parser::Expression::Literal(parser::Literal::Float(f)) => Ok(Value::Number(
                serde_json::Number::from_f64(*f).unwrap_or_else(|| 0.into()),
            )),
            parser::Expression::Literal(parser::Literal::Boolean(b)) => Ok(Value::Bool(*b)),
            parser::Expression::Literal(parser::Literal::Null) => Ok(Value::Null),
            parser::Expression::Literal(parser::Literal::Point(p)) => Ok(p.to_json_value()),
            parser::Expression::Variable(var) => context
                .get_variable(var)
                .cloned()
                .ok_or_else(|| Error::CypherSyntax(format!("Variable '{}' not found", var))),
            _ => {
                let empty_row: std::collections::HashMap<String, Value> =
                    std::collections::HashMap::new();
                self.evaluate_projection_expression(&empty_row, context, expr)
            }
        }
    }

    /// Apply Cartesian product of new values with existing variables in context
    /// This expands all existing array variables by repeating each element M times (where M is new_values.len())
    /// and creates the new variable by repeating the whole sequence N times (where N is existing row count).
    pub(in crate::executor) fn apply_cartesian_product(
        &self,
        context: &mut ExecutionContext,
        new_var: &str,
        new_values: Vec<Value>,
    ) -> Result<()> {
        // 1. Determine current row count (N)
        // Find the length of the first array variable
        let current_count = context
            .variables
            .values()
            .filter_map(|v| {
                if let Value::Array(arr) = v {
                    Some(arr.len())
                } else {
                    None
                }
            })
            .max() // Use max just in case, though they should be equal
            .unwrap_or(0);

        if current_count == 0 {
            // No existing rows (or only scalars), just set the new variable
            context.set_variable(new_var, Value::Array(new_values));
            return Ok(());
        }

        let new_count = new_values.len();
        if new_count == 0 {
            // New set is empty -> Cartesian product is empty
            // Clear all variables to empty arrays
            for val in context.variables.values_mut() {
                *val = Value::Array(Vec::new());
            }
            context.set_variable(new_var, Value::Array(Vec::new()));
            return Ok(());
        }

        // Audit (phase0_fix-cypher-oom-process-abort §3.3): this function has
        // exactly two sites that size an allocation from a product of counts
        // rather than from data already in hand — the per-column rebuild
        // below (`Vec::with_capacity(arr.len() * new_count)`) and the
        // new-variable expansion further down
        // (`Vec::with_capacity(new_count * current_count)`). Both derive
        // their length from the same `current_count * new_count` product
        // computed here, so a single pre-allocation check bounds both. The
        // clone loops that follow only push into these pre-sized vecs and
        // never allocate beyond them. No other allocation in this function
        // is sized from a product of counts.
        //
        // Check the size BEFORE allocating: `Vec::with_capacity` on an
        // unchecked product aborts the process rather than failing the
        // query — an UNWIND of 5 000 rows over two 5 000-node patterns
        // reaches 1.25e11 cells and asks the allocator for ~4 TB. The
        // budget is expressed in bytes, not rows, because the true cost is
        // `rows * size_of::<Value>() * columns` and a row limit means a
        // different amount of memory for a 2-column context than for a
        // 20-column one.
        let product = current_count.checked_mul(new_count).ok_or_else(|| {
            Error::OutOfMemory(format!(
                "Cartesian product {} x {} overflows usize; add LIMIT or narrow the query",
                current_count, new_count
            ))
        })?;

        // Every existing variable is rebuilt to `product` length, plus the
        // new variable itself adds one more column.
        let columns = context.variables.len() + 1;
        let est_bytes = product
            .checked_mul(columns)
            .and_then(|cells| cells.checked_mul(std::mem::size_of::<Value>()));

        let budget = self.config.cartesian_product_max_bytes;
        match est_bytes {
            Some(bytes) if bytes <= budget => {}
            Some(bytes) => {
                return Err(Error::OutOfMemory(format!(
                    "Cartesian product would materialise {} rows ({} x {}) across {} \
                     columns (~{} bytes), exceeding the configured budget of {} bytes; \
                     add LIMIT or narrow the query",
                    product, current_count, new_count, columns, bytes, budget
                )));
            }
            None => {
                return Err(Error::OutOfMemory(format!(
                    "Cartesian product would materialise {} rows ({} x {}) across {} \
                     columns, and the estimated byte size overflows usize, far exceeding \
                     the configured budget of {} bytes; add LIMIT or narrow the query",
                    product, current_count, new_count, columns, budget
                )));
            }
        }

        // 2. Expand existing variables: repeat each element M times (M = new_count)
        // We need to collect keys first to avoid borrowing issues
        let keys: Vec<String> = context.variables.keys().cloned().collect();

        for key in keys {
            if let Some(val) = context.variables.get_mut(&key) {
                if let Value::Array(arr) = val {
                    let mut new_arr = Vec::with_capacity(arr.len() * new_count);
                    for item in arr.iter() {
                        for _ in 0..new_count {
                            new_arr.push(item.clone());
                        }
                    }
                    *val = Value::Array(new_arr);
                }
            }
        }

        // 3. Expand new variable: repeat the whole sequence N times (N = current_count)
        let mut expanded_new_values = Vec::with_capacity(new_count * current_count);
        for _ in 0..current_count {
            expanded_new_values.extend(new_values.clone());
        }
        context.set_variable(new_var, Value::Array(expanded_new_values));

        Ok(())
    }

    pub(in crate::executor) fn materialize_rows_from_variables(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        // TRACE: Log variables before creating cartesian product
        let mut has_relationships = false;
        let mut var_types: Vec<(String, String)> = Vec::new();
        for (var, value) in &context.variables {
            let var_type = match value {
                Value::Object(obj) => {
                    if obj.contains_key("type") {
                        has_relationships = true;
                        "RELATIONSHIP".to_string()
                    } else {
                        "NODE".to_string()
                    }
                }
                Value::Array(arr) => {
                    let has_rel = arr.iter().any(|v| {
                        if let Value::Object(obj) = v {
                            obj.contains_key("type")
                        } else {
                            false
                        }
                    });
                    if has_rel {
                        has_relationships = true;
                    }
                    format!(
                        "ARRAY({})",
                        if has_rel {
                            "HAS_RELATIONSHIPS"
                        } else {
                            "NODES_ONLY"
                        }
                    )
                }
                _ => "OTHER".to_string(),
            };
            var_types.push((var.clone(), var_type));
        }
        tracing::trace!(
            "materialize_rows_from_variables: variables={:?}, has_relationships={}, creating cartesian product",
            var_types,
            has_relationships
        );

        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();

        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    // Only include non-empty arrays
                    if !values.is_empty() {
                        arrays.insert(var.clone(), values.clone());
                    }
                }
                other => {
                    // Include non-null single values
                    if !matches!(other, Value::Null) {
                        arrays.insert(var.clone(), vec![other.clone()]);
                    }
                }
            }
        }

        if arrays.is_empty() {
            return Vec::new();
        }

        // CRITICAL FIX: Implement true cartesian product instead of zip
        // When we have multiple node arrays (e.g., p1=[Alice, Bob], c2=[Acme, TechCorp]),
        // we need ALL combinations (4 rows), not just pairs (2 rows)

        // Check if all arrays have the same length and all are nodes (not single values)
        let all_same_len = arrays
            .values()
            .map(|v| v.len())
            .collect::<std::collections::HashSet<_>>()
            .len()
            == 1;
        let has_multiple_arrays = arrays.len() > 1;
        let all_multi_element = arrays.values().all(|v| v.len() > 1);

        let needs_cartesian_product = has_multiple_arrays && all_multi_element && all_same_len;

        if needs_cartesian_product {
            // TRUE CARTESIAN PRODUCT: Generate ALL combinations
            let var_names: Vec<String> = arrays.keys().cloned().collect();
            let array_values: Vec<Vec<Value>> =
                var_names.iter().map(|k| arrays[k].clone()).collect();

            // Calculate total number of combinations
            let total_combinations: usize = array_values.iter().map(|arr| arr.len()).product();

            let mut rows = Vec::new();

            // Generate all combinations using nested iteration
            let mut indices = vec![0usize; array_values.len()];

            loop {
                // Create a row from current indices
                let mut row = HashMap::new();
                for (i, var_name) in var_names.iter().enumerate() {
                    let value = array_values[i][indices[i]].clone();
                    row.insert(var_name.clone(), value);
                }
                rows.push(row);

                // Increment indices (like odometer)
                let mut carry = true;
                for i in (0..indices.len()).rev() {
                    if carry {
                        indices[i] += 1;
                        if indices[i] < array_values[i].len() {
                            carry = false;
                        } else {
                            indices[i] = 0;
                        }
                    }
                }

                // If carry is still true, we've exhausted all combinations
                if carry {
                    break;
                }
            }

            return rows;
        }

        // FALLBACK: Old zip-based logic for single arrays or mixed sizes
        let max_len = arrays
            .values()
            .map(|values| values.len())
            .max()
            .unwrap_or(0);

        if max_len == 0 {
            return Vec::new();
        }

        let mut rows = Vec::new();

        for idx in 0..max_len {
            let mut row = HashMap::new();
            let mut all_null = true;
            let mut entity_ids = Vec::new();

            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values.first().cloned().unwrap_or(Value::Null)
                } else {
                    // For arrays with different lengths, only use value if index exists
                    if idx < values.len() {
                        values.get(idx).cloned().unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    }
                };

                // Track if row has at least one non-null value
                if !matches!(value, Value::Null) {
                    all_null = false;

                    // Extract entity ID (node or relationship) for deduplication
                    if let Value::Object(obj) = &value {
                        if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                            if let Some(nid) = id.as_u64() {
                                entity_ids.push(nid);
                            }
                        }
                    }
                }

                row.insert(var.clone(), value);
            }

            // Add row if it has content and is not a duplicate
            if !all_null {
                /*
                let is_duplicate = if !entity_ids.is_empty() {
                    // Sort IDs to ensure consistent key regardless of column order
                    entity_ids.sort();
                    let key = entity_ids
                        .iter()
                        .map(|id| id.to_string())
                        .collect::<Vec<String>>()
                        .join("_");
                    !seen_row_keys.insert(key)
                } else {
                    // Fallback for rows without entities (e.g. literals) - no deduplication or full content deduplication?
                    // For now, allow all since we can't identify entities
                    false
                };

                if !is_duplicate {
                    rows.push(row);
                }
                */
                // DEBUG: Disable deduplication to see if rows are being generated
                rows.push(row);
            }
        }

        rows
    }

    /// Materialises rows by ZIPPING already-aligned column variables — the
    /// index-aligned counterpart to the cross-producing path in
    /// [`Self::materialize_rows_from_variables`].
    ///
    /// Called right after [`Self::apply_cartesian_product`], which leaves
    /// every array variable aligned to the SAME product length (index `i`
    /// is one output row). Running the general materialiser there instead
    /// would hit its `needs_cartesian_product` branch and RE-cross the
    /// already-crossed columns into `N^k` rows (`384^3 ≈ 56.6M` for a
    /// two-pattern `MATCH` over an 8-node label with 6 driving rows — a
    /// ~13 GB allocation that freezes the host). Zipping returns the `N`
    /// rows those aligned columns already represent.
    /// (phase0_fix-materialize-recrosses-aligned-columns)
    ///
    /// Length-1 arrays and scalars broadcast across all rows, matching the
    /// fallback (zip) semantics of [`Self::materialize_rows_from_variables`];
    /// all-`Null` rows are dropped identically.
    pub(in crate::executor) fn materialize_aligned_rows(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for (var, value) in &context.variables {
            match value {
                Value::Array(values) => {
                    if !values.is_empty() {
                        arrays.insert(var.clone(), values.clone());
                    }
                }
                other => {
                    if !matches!(other, Value::Null) {
                        arrays.insert(var.clone(), vec![other.clone()]);
                    }
                }
            }
        }

        if arrays.is_empty() {
            return Vec::new();
        }

        let max_len = arrays.values().map(|v| v.len()).max().unwrap_or(0);
        if max_len == 0 {
            return Vec::new();
        }

        let mut rows = Vec::with_capacity(max_len);
        for idx in 0..max_len {
            let mut row = HashMap::new();
            let mut all_null = true;
            for (var, values) in &arrays {
                let value = if values.len() == max_len {
                    values.get(idx).cloned().unwrap_or(Value::Null)
                } else if values.len() == 1 {
                    values[0].clone()
                } else if idx < values.len() {
                    values[idx].clone()
                } else {
                    Value::Null
                };
                if !matches!(value, Value::Null) {
                    all_null = false;
                }
                row.insert(var.clone(), value);
            }
            if !all_null {
                rows.push(row);
            }
        }
        rows
    }

    pub(in crate::executor) fn update_result_set_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        // TRACE: Check if input rows contain relationships
        let mut rows_with_relationships = 0;
        for row in rows {
            let has_rel = row.values().any(|value| {
                if let Value::Object(obj) = value {
                    obj.contains_key("type") // Relationships have "type" property
                } else {
                    false
                }
            });
            if has_rel {
                rows_with_relationships += 1;
            }
        }

        // CRITICAL FIX: Only use columns from rows, not from context.variables
        // Context variables may contain old/unused variables that cause null rows
        // Only include variables that are actually present in the rows
        let mut columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for row in rows {
            columns.extend(row.keys().cloned());
        }

        // Don't include variables from context - they may be stale
        // Only use what's actually in the rows

        let mut columns: Vec<String> = columns.into_iter().collect();
        columns.sort();

        // CRITICAL FIX: Deduplicate rows intelligently - consider full row content for relationship rows
        // When we have relationships (multiple rows with same source node), we need to check the full row
        // content, not just the source node ID, to avoid removing valid relationship rows
        use std::collections::HashSet;
        let mut seen_row_keys = HashSet::new();
        let mut unique_rows = Vec::new();

        for row_map in rows {
            // Collect all entity IDs (nodes and relationships) in this row
            // CRITICAL FIX: Extract all _nexus_id values, which can be from nodes or relationships
            // For relationship rows, we need to use ALL IDs (source node + target node + relationship)
            // to correctly differentiate between different relationships
            let mut all_entity_ids: Vec<u64> = Vec::new();

            // Extract all _nexus_id values from the row (both nodes and relationships have this)
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }

            // Fold non-entity columns (values with no `_nexus_id`, e.g. an
            // `UNWIND` driving map like `{s: 10}`) into the dedup key. Without
            // it, two rows that matched the SAME nodes from DIFFERENT driving
            // rows collapse to one, dropping every driving row after the first
            // — the truncation that surfaced once the aligned multi-pattern
            // path stopped re-crossing into `N^k`
            // (phase0_fix-materialize-recrosses-aligned-columns). Keying by
            // content only makes keys MORE specific (keeps more rows), which is
            // the correct direction: Cypher `MATCH` does not deduplicate rows.
            let non_entity_suffix = {
                let mut parts: Vec<String> = row_map
                    .iter()
                    .filter(|(_, v)| {
                        !matches!(v, Value::Object(o) if o.contains_key("_nexus_id"))
                            && !matches!(v, Value::Null)
                    })
                    .map(|(k, v)| format!("{}={}", k, serde_json::to_string(v).unwrap_or_default()))
                    .collect();
                parts.sort();
                parts.join("|")
            };

            // CRITICAL FIX: Determine deduplication key based on number of entity IDs
            // Relationship rows typically have multiple entity IDs (source node + target node + relationship)
            // Non-relationship rows have only one entity ID (just the node)
            let is_duplicate = if all_entity_ids.len() > 1 {
                // Relationship row or row with multiple entities
                // CRITICAL FIX: Find relationship ID and use it as primary key for deduplication
                // This ensures that rows with the same relationship ID are considered duplicates
                // even if they appear in different contexts (e.g., bidirectional relationships from source vs target)
                let relationship_id = row_map.values().find_map(|value| {
                    if let Value::Object(obj) = value {
                        // Relationship objects have a "type" property
                        if obj.contains_key("type") {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                return nid.as_u64();
                            }
                        }
                    }
                    None
                });

                if let Some(rel_id) = relationship_id {
                    // CRITICAL FIX: For relationship rows, use relationship ID + variable values
                    // This ensures that rows with same relationship ID but different variable assignments
                    // are not considered duplicates (e.g., bidirectional relationships: a=778,b=779 vs a=779,b=778)
                    // Build key using relationship ID + sorted list of variable names and their node IDs
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Skip relationship ID
                                    if entity_id != rel_id && !obj.contains_key("type") {
                                        // This is a node variable
                                        var_entries.push((key.clone(), entity_id));
                                    }
                                }
                            }
                        }
                    }

                    // Sort variable entries by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build deduplication key: rel_{id}_{var1}_{id1}_{var2}_{id2}...
                    let mut key_parts = vec![format!("rel_{}", rel_id)];
                    for (var_name, var_id) in &var_entries {
                        key_parts.push(format!("{}_{}", var_name, var_id));
                    }
                    if !non_entity_suffix.is_empty() {
                        key_parts.push(non_entity_suffix.clone());
                    }
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                } else {
                    // Fallback: Can't find rel_id but have multiple entities - include variables in key
                    // This handles bidirectional relationships where we need to differentiate by variable assignment
                    let mut var_entries: Vec<(String, u64)> = Vec::new();

                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    // Include all entities with their variable names
                                    var_entries.push((key.clone(), entity_id));
                                }
                            }
                        }
                    }

                    // Sort by variable name for consistent key generation
                    var_entries.sort_by(|a, b| a.0.cmp(&b.0));

                    // Build key: var1_id1_var2_id2_var3_id3...
                    let mut key_parts: Vec<String> = var_entries
                        .iter()
                        .map(|(var_name, var_id)| format!("{}_{}", var_name, var_id))
                        .collect();
                    if !non_entity_suffix.is_empty() {
                        key_parts.push(non_entity_suffix.clone());
                    }
                    let row_key = key_parts.join("_");

                    let is_dup = !seen_row_keys.insert(row_key.clone());
                    is_dup
                }
            } else if let Some(first_id) = all_entity_ids.first() {
                // Non-relationship row - but check if this is from OPTIONAL MATCH (has NULL values)
                // CRITICAL FIX: For OPTIONAL MATCH NULL rows, include NULL variable names in key
                // to prevent incorrect deduplication of different source nodes
                let has_null_values = row_map.values().any(|v| matches!(v, Value::Null));

                if has_null_values {
                    // OPTIONAL MATCH NULL row - include all variable names and their values/NULL status
                    let mut var_entries: Vec<String> = Vec::new();
                    for (key, value) in row_map {
                        if let Value::Object(obj) = value {
                            if let Some(Value::Number(nid)) = obj.get("_nexus_id") {
                                if let Some(entity_id) = nid.as_u64() {
                                    var_entries.push(format!("{}_{}", key, entity_id));
                                }
                            }
                        } else if matches!(value, Value::Null) {
                            var_entries.push(format!("{}_null", key));
                        }
                    }
                    var_entries.sort();
                    let row_key = var_entries.join("_");
                    !seen_row_keys.insert(row_key)
                } else {
                    // Regular non-relationship row - use only entity ID
                    let entity_key = format!("node_{}", first_id);
                    !seen_row_keys.insert(entity_key)
                }
            } else {
                // No entity IDs found - use full row content as fallback dedup
                // key. If JSON serialisation fails (usually: non-finite floats
                // in a property map) we fall back to a `{:?}` key rather than
                // the empty string; otherwise every failing row collapses into
                // a single dedup bucket. A warn! + metric marks the event.
                //
                // This helper returns `()` with 18 call sites — propagating
                // errors here would be a wide cascade. The failure is
                // confined to the dedup decision, so degrading to Rust Debug
                // for the key is a safe compromise (different values still
                // produce different strings).
                let row_key = match serde_json::to_string(row_map) {
                    Ok(s) => s,
                    Err(e) => {
                        super::super::serde_metrics::record_fallback(
                            super::super::serde_metrics::SerdeFallbackSite::HelperRowDedupKey,
                        );
                        tracing::warn!(
                            target: "nexus_core::executor",
                            error = %e,
                            "update_result_set_from_rows: serde_json::to_string failed for \
                             dedup key; falling back to Debug representation. \
                             See nexus_executor_serde_fallback_total{{site=\"helper_row_dedup_key\"}}."
                        );
                        format!("{:?}", row_map)
                    }
                };
                !seen_row_keys.insert(row_key)
            };

            // Only add row if it's not a duplicate
            if !is_duplicate {
                unique_rows.push(row_map.clone());
            }
        }

        tracing::debug!(
            "update_result_set_from_rows: deduplicated {} rows to {} unique rows",
            rows.len(),
            unique_rows.len()
        );

        // DEBUG: Log details of each row for debugging
        for (idx, row_map) in rows.iter().enumerate() {
            let mut all_entity_ids: Vec<u64> = Vec::new();
            for value in row_map.values() {
                if let Value::Object(obj) = value {
                    if let Some(Value::Number(id)) = obj.get("_nexus_id") {
                        if let Some(entity_id) = id.as_u64() {
                            all_entity_ids.push(entity_id);
                        }
                    }
                }
            }
            all_entity_ids.sort();
        }

        // CRITICAL FIX: Always clear result_set.rows before updating to ensure complete replacement
        // This prevents mixing old rows with new ones
        context.result_set.rows.clear();
        context.result_set.columns = columns.clone();
        context.result_set.rows = unique_rows
            .iter()
            .map(|row_map| Row {
                values: columns
                    .iter()
                    .map(|column| row_map.get(column).cloned().unwrap_or(Value::Null))
                    .collect(),
            })
            .collect();
    }

    /// Check if an expression can be evaluated without variables (only literals and operations)
    pub(in crate::executor) fn can_evaluate_without_variables(
        &self,
        expr: &parser::Expression,
    ) -> bool {
        match expr {
            parser::Expression::Literal(_) => true,
            parser::Expression::Parameter(_) => true, // Parameters can be evaluated
            parser::Expression::Variable(_) => false, // Variables need context
            parser::Expression::PropertyAccess { .. } => false, // Property access needs variables
            parser::Expression::ArrayIndex { base, index } => {
                // Can evaluate if both base and index can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && self.can_evaluate_without_variables(index)
            }
            parser::Expression::ArraySlice { base, start, end } => {
                // Can evaluate if base and both indices can be evaluated without variables
                self.can_evaluate_without_variables(base)
                    && start
                        .as_ref()
                        .map(|s| self.can_evaluate_without_variables(s))
                        .unwrap_or(true)
                    && end
                        .as_ref()
                        .map(|e| self.can_evaluate_without_variables(e))
                        .unwrap_or(true)
            }
            parser::Expression::BinaryOp { left, right, .. } => {
                // Can evaluate if both operands can be evaluated
                self.can_evaluate_without_variables(left)
                    && self.can_evaluate_without_variables(right)
            }
            parser::Expression::UnaryOp { operand, .. } => {
                // Can evaluate if operand can be evaluated
                self.can_evaluate_without_variables(operand)
            }
            parser::Expression::FunctionCall { args, .. } => {
                // Can evaluate if all arguments can be evaluated
                args.iter()
                    .all(|arg| self.can_evaluate_without_variables(arg))
            }
            parser::Expression::Case {
                input,
                when_clauses,
                else_clause,
            } => {
                // Can evaluate if input (if present) and all when/else expressions can be evaluated
                let input_ok = input
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                let when_ok = when_clauses.iter().all(|when| {
                    self.can_evaluate_without_variables(&when.condition)
                        && self.can_evaluate_without_variables(&when.result)
                });
                let else_ok = else_clause
                    .as_ref()
                    .map(|e| self.can_evaluate_without_variables(e))
                    .unwrap_or(true);
                input_ok && when_ok && else_ok
            }
            parser::Expression::IsNull { expr, .. } => self.can_evaluate_without_variables(expr),
            parser::Expression::List(exprs) => {
                exprs.iter().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Map(map) => {
                map.values().all(|e| self.can_evaluate_without_variables(e))
            }
            parser::Expression::Exists { .. } => false, // EXISTS needs graph context
            parser::Expression::CollectSubquery { .. } => {
                // COLLECT { … } evaluates the inner subquery against
                // the storage layer; when the *outer* row is empty the
                // synthetic-row gate is the only thing standing
                // between us and "RETURN COLLECT { … } AS x" silently
                // emitting zero rows. The inner may reference outer
                // variables, but in that case a preceding clause has
                // already populated rows and this gate is irrelevant.
                true
            }
            parser::Expression::PatternComprehension { .. } => false, // Pattern needs graph context
            parser::Expression::MapProjection { .. } => false, // Map projection needs variables
            parser::Expression::ListComprehension {
                list_expression, ..
            } => {
                // List comprehension can be evaluated if the list expression can be evaluated.
                // The where_clause and transform_expression may reference the comprehension variable,
                // which is fine - it will be bound during comprehension execution.
                self.can_evaluate_without_variables(list_expression)
            }
        }
    }

    /// Check if a pattern exists in the current context
    pub(in crate::executor) fn check_pattern_exists(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<bool> {
        // For EXISTS, we need to check if the pattern matches in the current context
        // This checks if nodes and relationships actually exist

        // If pattern is empty, return false
        if pattern.elements.is_empty() {
            return Ok(false);
        }

        // Get the first node from the pattern
        if let Some(parser::PatternElement::Node(first_node)) = pattern.elements.first() {
            // If the node has a variable, check if it exists in the current row/context
            if let Some(var_name) = &first_node.variable {
                // Check if variable exists in current row
                if let Some(Value::Object(obj)) = row.get(var_name) {
                    // If it's a valid node object, check relationships if pattern has them
                    if let Some(Value::Number(node_id_val)) = obj.get("_nexus_id") {
                        let node_id = node_id_val
                            .as_u64()
                            .ok_or_else(|| Error::InvalidId("Invalid node ID".to_string()))?;

                        // If pattern has only one element (just a node), it exists
                        if pattern.elements.len() == 1 {
                            return Ok(true);
                        }

                        // Pattern has relationships - actually check if they exist
                        // Look for relationship element in pattern
                        for (i, element) in pattern.elements.iter().enumerate() {
                            if let parser::PatternElement::Relationship(rel) = element {
                                // Get relationship types to match
                                let type_ids: Vec<u32> = if rel.types.is_empty() {
                                    // No types specified = match all types
                                    vec![]
                                } else {
                                    rel.types
                                        .iter()
                                        .filter_map(|t| {
                                            self.catalog().get_type_id(t).ok().flatten()
                                        })
                                        .collect()
                                };

                                // Determine direction
                                let direction = match rel.direction {
                                    parser::RelationshipDirection::Outgoing => Direction::Outgoing,
                                    parser::RelationshipDirection::Incoming => Direction::Incoming,
                                    parser::RelationshipDirection::Both => Direction::Both,
                                };

                                // Fetch relationships for this node
                                // find_relationships already filters by type_ids and direction
                                let relationships = self.find_relationships(
                                    node_id, &type_ids, direction,
                                    None, // No cache for EXISTS checks
                                )?;

                                // If no matching relationships found, pattern doesn't exist
                                if relationships.is_empty() {
                                    return Ok(false);
                                }

                                // At least one relationship exists
                                return Ok(true);
                            }
                        }

                        // No relationship element found in pattern
                        return Ok(true);
                    }
                }

                // Check if variable exists in context variables
                if let Some(Value::Array(nodes)) = context.variables.get(var_name) {
                    if !nodes.is_empty() {
                        return Ok(true);
                    }
                }
            } else {
                // No variable - pattern exists if we can find matching nodes
                // For simplicity, if no variable is specified, assume pattern might exist
                // This is a basic implementation
                return Ok(true);
            }
        }

        // Pattern doesn't match
        Ok(false)
    }

    pub(in crate::executor) fn extract_property(entity: &Value, property: &str) -> Value {
        if let Value::Object(obj) = entity {
            // First check directly in the object (for nodes with flat properties)
            // This is the primary case - nodes have properties directly in the object
            if let Some(value) = obj.get(property) {
                // CRITICAL FIX: Allow _nexus_id to be returned when explicitly requested
                // Only skip truly internal properties that shouldn't be exposed
                if property == "_nexus_id" {
                    // _nexus_id is allowed and commonly used in queries
                    return value.clone();
                }
                // Skip other internal properties
                if property != "_nexus_type"
                    && property != "_source"
                    && property != "_target"
                    && property != "_element_id"
                {
                    return value.clone();
                }
            }
            // Then check if there's a nested "properties" object (for compatibility with other formats)
            if let Some(Value::Object(props)) = obj.get("properties") {
                if let Some(value) = props.get(property) {
                    return value.clone();
                }
            }
        }
        Value::Null
    }

    /// Check if value is a duration object (has years, months, days, hours, minutes, or seconds keys)

    pub(in crate::executor) fn update_variables_from_rows(
        &self,
        context: &mut ExecutionContext,
        rows: &[HashMap<String, Value>],
    ) {
        let mut arrays: HashMap<String, Vec<Value>> = HashMap::new();
        for row in rows {
            for (var, value) in row {
                arrays.entry(var.clone()).or_default().push(value.clone());
            }
        }

        context.variables.clear();

        for (var, values) in arrays {
            context.variables.insert(var, Value::Array(values));
        }
    }

    pub(in crate::executor) fn evaluate_predicate_on_row(
        &self,
        row: &HashMap<String, Value>,
        context: &ExecutionContext,
        expr: &parser::Expression,
    ) -> Result<bool> {
        let value = self.evaluate_projection_expression(row, context, expr)?;
        self.value_to_bool(&value)
    }

    pub(in crate::executor) fn extract_entity_id(value: &Value) -> Option<u64> {
        match value {
            Value::Object(obj) => {
                if let Some(id) = obj.get("_nexus_id").and_then(|id| id.as_u64()) {
                    Some(id)
                } else if let Some(id) = obj
                    .get("_element_id")
                    .and_then(|id| id.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                {
                    Some(id)
                } else if let Some(id_value) = obj.get("id") {
                    match id_value {
                        Value::Number(num) => num.as_u64(),
                        Value::String(s) => s.parse::<u64>().ok(),
                        _ => None,
                    }
                } else {
                    None
                }
            }
            Value::Number(num) => num.as_u64(),
            _ => None,
        }
    }

    /// Read a relationship as a JSON value.
    ///
    /// Acquires its own `store()` read guard. Bulk loops that already
    /// hold a guard (e.g. alongside repeated
    /// `read_node_as_value_with_store` calls) should use
    /// [`Self::read_relationship_as_value_with_store`] instead to avoid
    /// a second acquisition per element — see that method's doc comment.
    pub(in crate::executor) fn read_relationship_as_value(
        &self,
        rel: &RelationshipInfo,
    ) -> Result<Value> {
        let store = self.store();
        self.read_relationship_as_value_with_store(&store, rel)
    }

    /// Same as [`Self::read_relationship_as_value`], but for callers
    /// that already hold a `store()` read guard.
    ///
    /// phase8_neo4j-concurrency-gaps §2 — mirrors
    /// `Executor::read_node_as_value_with_store`: bulk loops that
    /// materialise both a node and its relationship per element (e.g.
    /// `Expand`'s target loop) would otherwise take a SECOND
    /// independent `self.store()` acquisition here on top of the one
    /// already held for the node read. Threading the held guard through
    /// avoids that, and — critically — avoids a same-thread recursive
    /// acquire of the non-reentrant `parking_lot::RwLock` while an outer
    /// guard from the same call chain is still alive (see the
    /// `parking-lot-rwlock-does-not-allow-recursive-acquire` anti-pattern
    /// entry).
    pub(in crate::executor) fn read_relationship_as_value_with_store(
        &self,
        store: &RecordStore,
        rel: &RelationshipInfo,
    ) -> Result<Value> {
        let type_name = self
            .catalog()
            .get_type_name(rel.type_id)?
            .unwrap_or_else(|| format!("type_{}", rel.type_id));

        let properties_value = store
            .load_relationship_properties(rel.id)?
            .unwrap_or_else(|| Value::Object(Map::new()));

        let properties_map = match properties_value {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("value".to_string(), other);
                map
            }
        };

        // Add _nexus_id for internal ID extraction (e.g., for type() function)
        // Add type property to identify this as a relationship object in deduplication
        let mut rel_obj = properties_map;
        rel_obj.insert("_nexus_id".to_string(), Value::Number(rel.id.into()));
        rel_obj.insert("type".to_string(), Value::String(type_name));

        // Return only the properties as a flat object, matching Neo4j's format
        Ok(Value::Object(rel_obj))
    }

    /// Phase 2.4.2: Optimize result_set_as_rows to reduce intermediate copies
    pub(in crate::executor) fn result_set_as_rows(
        &self,
        context: &ExecutionContext,
    ) -> Vec<HashMap<String, Value>> {
        // Pre-size the result vector to avoid reallocations
        let capacity = context.result_set.rows.len();
        let mut result = Vec::with_capacity(capacity);

        for row in &context.result_set.rows {
            // Pre-size HashMap based on column count
            let mut map = HashMap::with_capacity(context.result_set.columns.len());
            for (idx, column) in context.result_set.columns.iter().enumerate() {
                if idx < row.values.len() {
                    // Use reference when possible, only clone when necessary
                    map.insert(column.clone(), row.values[idx].clone());
                } else {
                    map.insert(column.clone(), Value::Null);
                }
            }
            result.push(map);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    //! phase0_fix-cypher-oom-process-abort §4.2 — unit coverage for the
    //! byte budget check in [`Executor::apply_cartesian_product`]. The
    //! integration-level regression test (the §1.1 minimal repro shape
    //! surviving end-to-end instead of aborting the process) lives in
    //! `crates/nexus-core/tests/cypher_oom_guard_test.rs`; these tests
    //! pin the ceiling itself: it fires deterministically, is
    //! configurable via `ExecutorConfig::cartesian_product_max_bytes`,
    //! and does not reject legitimate small products under the default
    //! budget.

    use super::*;
    use crate::testing::create_test_executor;
    use serde_json::json;

    #[test]
    fn apply_cartesian_product_rejects_when_budget_is_absurdly_low() {
        let (mut executor, _ctx) = create_test_executor();
        // A trivial 2x2 product estimates to 2 * 2 * columns * 32 bytes
        // (>= 128 bytes even at columns=1). A 1-byte budget must reject
        // it regardless of how small the product actually is.
        executor.config.cartesian_product_max_bytes = 1;

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

        let result = executor.apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)]);

        match result {
            Err(Error::OutOfMemory(msg)) => {
                assert!(
                    msg.contains("Cartesian product"),
                    "OutOfMemory message should name the offending operation: {msg}"
                );
            }
            other => {
                panic!("expected Err(Error::OutOfMemory(_)) under a 1-byte budget, got {other:?}")
            }
        }
    }

    #[test]
    fn apply_cartesian_product_succeeds_under_default_budget() {
        // Same shape as the low-budget test above, but with the
        // default (1 GiB) budget left untouched — proves the rejection
        // above comes specifically from the configured ceiling, not
        // from `apply_cartesian_product` being broken for any input.
        let (mut executor, _ctx) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));

        executor
            .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
            .expect("a 2x2 product must stay well under the default 1 GiB budget");

        assert_eq!(
            context.get_variable("a"),
            Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
        );
        assert_eq!(
            context.get_variable("b"),
            Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
        );
    }

    /// phase0_fix-materialize-recrosses-aligned-columns — DISCRIMINATING.
    /// After `apply_cartesian_product` aligns two columns to length 4
    /// (`a=[1,1,2,2]`, `b=[3,4,3,4]`, each index = one output row), the
    /// aligned materialiser must ZIP them into exactly 4 rows, while the
    /// general materialiser RE-crosses them into 4*4 = 16. The `k`-column
    /// gap is `N^(k-1)`; at query scale (`N=384`, `k=3`) that same
    /// re-cross is `384^3 ≈ 56.6M` rows (~13 GB), which froze the host.
    #[test]
    fn materialize_aligned_rows_zips_instead_of_recrossing() {
        let (mut executor, _ctx) = create_test_executor();

        let mut context = ExecutionContext::new(HashMap::new(), None);
        context.set_variable("a", Value::Array(vec![json!(1), json!(2)]));
        executor
            .apply_cartesian_product(&mut context, "b", vec![json!(3), json!(4)])
            .expect("2x2 product stays under the default budget");

        // Preconditions: both columns are aligned to length 4.
        assert_eq!(
            context.get_variable("a"),
            Some(&Value::Array(vec![json!(1), json!(1), json!(2), json!(2)]))
        );
        assert_eq!(
            context.get_variable("b"),
            Some(&Value::Array(vec![json!(3), json!(4), json!(3), json!(4)]))
        );

        // The general materialiser RE-crosses the aligned columns: 4 x 4 = 16.
        // This is the over-production the fix avoids (documented, not desired).
        let recrossed = executor.materialize_rows_from_variables(&context);
        assert_eq!(
            recrossed.len(),
            16,
            "materialize_rows_from_variables re-crosses aligned columns (N^k); \
             this pins the bug the aligned path must avoid"
        );

        // The aligned materialiser ZIPS: exactly the 4 rows the columns
        // already represent, in index order.
        let zipped = executor.materialize_aligned_rows(&context);
        assert_eq!(
            zipped.len(),
            4,
            "materialize_aligned_rows must zip aligned columns to N rows, not N^k"
        );

        let mut pairs: Vec<(i64, i64)> = zipped
            .iter()
            .map(|row| {
                (
                    row["a"].as_i64().expect("a is an integer"),
                    row["b"].as_i64().expect("b is an integer"),
                )
            })
            .collect();
        pairs.sort_unstable();
        assert_eq!(
            pairs,
            vec![(1, 3), (1, 4), (2, 3), (2, 4)],
            "zipped rows must be the exact index-aligned (a, b) pairs"
        );
    }
}
