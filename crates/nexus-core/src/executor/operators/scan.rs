//! Scan operators and filter-push-down helpers. `execute_node_by_label` and
//! `execute_all_nodes_scan` materialise source variables; `try_index_based_filter`
//! plus its `parse_equality_filter` / `parse_range_filter` helpers attempt to
//! push a Filter down into an index lookup.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::Row;
use super::super::{MAX_INTERMEDIATE_ROWS, push_with_row_cap};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

/// Convert a `serde_json::Value` produced by evaluating a correlated seek
/// key (e.g. `r.s`) into the `PropertyValue` the property index is keyed
/// on. Mirrors the plan-time literal match in `node_index_seek_for`
/// (`planner/queries/strategy.rs:1316-1327`): string → `String`, integer
/// number → `Integer`, float number → `Float`, bool → `Boolean`. `Null`,
/// arrays, and objects are not indexable scalars — `None` tells the caller
/// the key matches no node for that driving row (not an error).
fn json_value_to_property_value(value: &Value) -> Option<crate::index::PropertyValue> {
    match value {
        Value::String(s) => Some(crate::index::PropertyValue::String(s.clone())),
        Value::Number(n) => match n.as_i64() {
            Some(i) => Some(crate::index::PropertyValue::Integer(i)),
            None => n.as_f64().map(crate::index::PropertyValue::Float),
        },
        Value::Bool(b) => Some(crate::index::PropertyValue::Boolean(*b)),
        Value::Null | Value::Array(_) | Value::Object(_) => None,
    }
}

impl Executor {
    pub(in crate::executor) fn execute_node_by_label(&self, label_id: u32) -> Result<Vec<Value>> {
        // Always use label_index - label_id 0 is valid (it's the first label)
        let bitmap = self.label_index().get_nodes(label_id)?;

        // CRITICAL FIX: Deduplicate node IDs to avoid returning duplicate nodes
        // Use HashSet to track seen node IDs since bitmap should already be unique
        use std::collections::HashSet;
        let mut seen_node_ids = HashSet::new();
        let cap_hint = (bitmap.len() as usize).min(MAX_INTERMEDIATE_ROWS);
        let mut results = Vec::with_capacity(cap_hint);

        // phase8_neo4j-concurrency-gaps §2 — acquire the `store` read
        // guard ONCE for the whole label scan instead of once per
        // candidate node via `read_node_as_value`. This is the exact
        // per-iteration-lock-acquisition-in-a-loop pattern from the
        // `per-iteration-rwlock-re-acquisition-in-scan-loops-collapses-
        // under-thread-count` knowledge entry: `execute_node_by_label`
        // is the dominant materialiser behind
        // `traversal.small_two_hop_from_hub`'s concurrency ceiling
        // whenever the pattern falls back to NodeByLabel + Filter
        // (e.g. an unindexed property predicate) — every candidate
        // node under the label was independently re-acquiring the
        // single `ExecutorShared.store` `parking_lot::RwLock`. Nothing
        // else in this loop body touches `self.store()`, so holding
        // the guard for the whole scan is safe.
        let store = self.store();
        for node_id in bitmap.iter() {
            if results.len() >= MAX_INTERMEDIATE_ROWS {
                return Err(Error::OutOfMemory(format!(
                    "NodeByLabel scan would return more than {} rows \
                     (MAX_INTERMEDIATE_ROWS); add LIMIT or narrow the predicate",
                    MAX_INTERMEDIATE_ROWS
                )));
            }
            let node_id_u64 = node_id as u64;

            // Skip if we've already seen this node ID (shouldn't happen, but safety check)
            if !seen_node_ids.insert(node_id_u64) {
                continue;
            }

            // phase8_neo4j-concurrency-gaps §2 — the standalone
            // deleted-node pre-check used to call `self.store()` here
            // AND `read_node_as_value` immediately below independently
            // re-reads the same header and already returns `Value::Null`
            // for a deleted node (which every caller here already
            // treats as "skip"). That made this loop take two store
            // read-lock acquisitions per candidate node for no
            // behavioural difference — removed to halve the lock churn
            // on this hot path.
            match self.read_node_as_value_with_store(&store, node_id_u64)? {
                Value::Null => continue,
                value => results.push(value),
            }
        }
        drop(store);

        Ok(results)
    }

    /// Seed a scan from the typed property index. Returns only the nodes
    /// whose `(label_id, key_id)` property equals `value`. Falls back to a
    /// full label scan when no PropertyIndex handle is installed (test
    /// harness executors built outside an engine).
    pub(in crate::executor) fn execute_node_index_seek(
        &self,
        label_id: u32,
        key_id: u32,
        value: &crate::index::PropertyValue,
    ) -> Result<Vec<Value>> {
        let Some(prop_idx) = self.property_index() else {
            return self.execute_node_by_label(label_id);
        };
        let bitmap = prop_idx.find_exact(label_id, key_id, value.clone())?;
        use std::collections::HashSet;
        let cap_hint = (bitmap.len() as usize).min(MAX_INTERMEDIATE_ROWS);
        let mut seen = HashSet::new();
        let mut results = Vec::with_capacity(cap_hint);
        // phase8_neo4j-concurrency-gaps §2 — same acquire-once pattern
        // as `execute_node_by_label` above: one `store()` guard for the
        // whole seek instead of one per matched node.
        let store = self.store();
        for node_id in bitmap.iter() {
            if results.len() >= MAX_INTERMEDIATE_ROWS {
                return Err(Error::OutOfMemory(format!(
                    "NodeIndexSeek would return more than {} rows \
                     (MAX_INTERMEDIATE_ROWS); add LIMIT or narrow the predicate",
                    MAX_INTERMEDIATE_ROWS
                )));
            }
            let node_id_u64 = node_id as u64;
            if !seen.insert(node_id_u64) {
                continue;
            }
            // phase8_neo4j-concurrency-gaps §2 — see the identical
            // removal + rationale in `execute_node_by_label` above:
            // `read_node_as_value` already filters deleted nodes.
            match self.read_node_as_value_with_store(&store, node_id_u64)? {
                Value::Null => continue,
                v => results.push(v),
            }
        }
        drop(store);
        Ok(results)
    }

    /// Execute a correlated `NodeIndexSeek` whose seek key is evaluated per
    /// driving row (`key_expression: Some(expr)`, e.g. `r.s` from
    /// `UNWIND $rows AS r MATCH (a:P {id: r.s})`) instead of a single
    /// plan-time constant. For each driving row: evaluate `expr` against
    /// that row's bindings, convert the result to a `PropertyValue`, and
    /// seek the property index directly for that row only — the full
    /// label × driving-row cross product is never materialised.
    ///
    /// A key that evaluates to `Null`/a non-scalar, or one that matches no
    /// node, drops only that driving row's output (the query keeps going,
    /// never errors); a key matching K nodes duplicates the driving row K
    /// times. See `phase0_fix-correlated-predicate-index-seek` §3.
    pub(in crate::executor) fn execute_correlated_index_seek(
        &self,
        context: &mut ExecutionContext,
        label_id: u32,
        key_id: u32,
        key_expression: &parser::Expression,
        variable: &str,
    ) -> Result<()> {
        // Determine the driving rows in whichever representation the
        // pipeline currently holds them — mirrors the two cases
        // `seed_scan_main_loop` branches on for the constant-key path.
        let driving_rows: Vec<HashMap<String, Value>> = if !context.variables.is_empty() {
            // Case A: columnar variables (e.g. after a prior MATCH/WITH).
            context.variables.remove(variable);
            self.materialize_rows_from_variables(context)
        } else if !context.result_set.rows.is_empty() {
            // Case B: fresh UNWIND — rows live in `result_set`, variables
            // are still empty.
            let columns = context.result_set.columns.clone();
            context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &columns))
                .collect()
        } else {
            Vec::new()
        };

        if driving_rows.is_empty() {
            // No driving binding to evaluate the row-local key against —
            // there is no row context for `key_expression`, so there is
            // nothing to join. Emit an empty result rather than falling
            // back to a full scan.
            context.variables.remove(variable);
            context.set_variable(variable, Value::Array(Vec::new()));
            context.result_set.columns = vec![variable.to_string()];
            context.result_set.rows.clear();
            return Ok(());
        }

        // The planner only emits `key_expression: Some(_)` when a
        // PropertyIndex handle already backs `(label_id, key_id)` (§2.3).
        // A missing handle here means a test harness built an executor
        // without installing one — fail loudly rather than silently
        // degrading to an unindexed scan under an operator name that
        // promises a seek.
        let Some(prop_idx) = self.property_index() else {
            return Err(Error::internal(
                "NodeIndexSeek with a correlated key_expression requires a PropertyIndex \
                 handle, but none is installed on this executor",
            ));
        };

        // Columns the joined output rows carry: every driving-row column
        // plus the seek's own target variable. Computed up front so a
        // fully-unmatched driving set still clears stale bindings instead
        // of leaving old data behind in `context.variables`.
        let mut columns: std::collections::HashSet<String> = std::collections::HashSet::new();
        for row in &driving_rows {
            columns.extend(row.keys().cloned());
        }
        columns.insert(variable.to_string());

        // phase8_neo4j-concurrency-gaps §2 pattern: acquire the store
        // guard once for the whole per-row seek instead of once per
        // matched node.
        let store = self.store();
        let mut output_rows: Vec<HashMap<String, Value>> = Vec::new();
        for driving_row in &driving_rows {
            let key_value =
                self.evaluate_projection_expression(driving_row, context, key_expression)?;
            let Some(pv) = json_value_to_property_value(&key_value) else {
                // Null / non-scalar key: matches nothing — no row for
                // this driving row, but the query keeps going.
                continue;
            };
            let bitmap = match prop_idx.find_exact(label_id, key_id, pv) {
                Ok(bitmap) => bitmap,
                Err(e) => {
                    drop(store);
                    return Err(e);
                }
            };
            for node_id in bitmap.iter() {
                if output_rows.len() >= MAX_INTERMEDIATE_ROWS {
                    drop(store);
                    return Err(Error::out_of_memory(format!(
                        "Correlated NodeIndexSeek would return more than {} rows \
                         (MAX_INTERMEDIATE_ROWS); add LIMIT or narrow the query",
                        MAX_INTERMEDIATE_ROWS
                    )));
                }
                let node_value = match self.read_node_as_value_with_store(&store, node_id as u64) {
                    Ok(v) => v,
                    Err(e) => {
                        drop(store);
                        return Err(e);
                    }
                };
                if matches!(node_value, Value::Null) {
                    continue;
                }
                let mut joined = driving_row.clone();
                joined.insert(variable.to_string(), node_value);
                output_rows.push(joined);
            }
        }
        drop(store);

        // Write the joined rows back in both representations downstream
        // operators read: columnar per-variable arrays (the same shape
        // `apply_cartesian_product` leaves behind) and the row-oriented
        // `result_set`, via the same `update_result_set_from_rows` helper
        // the constant-key path shares with every other scan operator.
        for column in &columns {
            let values: Vec<Value> = output_rows
                .iter()
                .map(|row| row.get(column).cloned().unwrap_or(Value::Null))
                .collect();
            context.set_variable(column, Value::Array(values));
        }
        self.update_result_set_from_rows(context, &output_rows);
        Ok(())
    }

    /// Execute AllNodesScan operator (scan all nodes regardless of label)
    pub(in crate::executor) fn execute_all_nodes_scan(&self) -> Result<Vec<Value>> {
        // phase8_neo4j-concurrency-gaps §2 — acquire the `store` read
        // guard ONCE for the entire scan: `node_count()` and every
        // `read_node_as_value_with_store` call below now share it,
        // instead of one acquisition for the count plus one more per
        // candidate node.
        let store = self.store();
        let total_nodes = store.node_count();
        let cap_hint = (total_nodes as usize).min(MAX_INTERMEDIATE_ROWS);
        let mut results = Vec::with_capacity(cap_hint);

        // Scan all node IDs from 0 to total_nodes-1
        for node_id in 0..total_nodes {
            if results.len() >= MAX_INTERMEDIATE_ROWS {
                return Err(Error::OutOfMemory(format!(
                    "AllNodesScan would return more than {} rows \
                     (MAX_INTERMEDIATE_ROWS); add a label predicate or LIMIT",
                    MAX_INTERMEDIATE_ROWS
                )));
            }
            // phase8_neo4j-concurrency-gaps §2 — see the identical
            // removal + rationale in `execute_node_by_label` above:
            // `read_node_as_value` already filters deleted nodes, so the
            // separate `self.store().read_node()` pre-check this loop
            // used to do was a second lock acquisition per candidate for
            // no behavioural difference.
            match self.read_node_as_value_with_store(&store, node_id)? {
                Value::Null => continue,
                value => {
                    results.push(value);
                }
            }
        }
        drop(store);

        Ok(results)
    }

    /// Try to execute filter using index-based optimization (Phase 5 optimization)
    ///
    /// This method attempts to use property indexes to accelerate WHERE clauses
    /// by avoiding full table scans for equality and range queries.
    pub(in crate::executor) fn try_index_based_filter(
        &self,
        context: &mut ExecutionContext,
        predicate: &str,
    ) -> Result<Option<Vec<Row>>> {
        if let Some(cache) = &context.cache {
            let cache_lock = cache.read();
            let property_index = cache_lock.property_index_manager();

            // Parse simple equality patterns: variable.property = 'value'
            if let Some((var_name, prop_name, value)) = self.parse_equality_filter(predicate) {
                // Check if we have an index for this property
                let has_index = property_index.indexed_properties().contains(&prop_name);

                if has_index {
                    // Use existing index to find matching entities
                    let entity_ids = property_index.find_exact(&prop_name, &value);

                    if !entity_ids.is_empty() {
                        // Convert entity IDs to rows - this would need more context in production
                        // For now, return None to use regular filtering
                        // TODO: Implement full row construction from indexed entities
                        return Ok(None);
                    }
                } else {
                    // AUTO-INDEXING: Track property access for potential automatic indexing
                    // This brings Nexus closer to Neo4j's automatic indexing behavior
                    let mut stats = self.property_access_stats.write();
                    let count = stats.entry(prop_name.clone()).or_insert(0);
                    *count += 1;

                    // Log opportunity and suggest manual indexing for now
                    if *count % 10 == 0 {
                        // Actionable tuning hint, not a hot-path event. `debug!`
                        // keeps it reachable via `RUST_LOG=nexus_core=debug`
                        // when the operator is investigating slow queries
                        // without spamming default logs.
                        tracing::debug!(
                            prop = %prop_name,
                            accesses = *count,
                            "index opportunity: property seen in WHERE without index; \
                             CREATE INDEX ON :<Label>({0}) to accelerate",
                            prop_name,
                        );
                    }

                    // TODO: Implement automatic background index creation when count exceeds threshold
                    // This would create indexes automatically in a background thread

                    // TODO: Implement automatic index creation when count exceeds threshold
                    // This would create indexes automatically after observing enough usage

                    // For now, fall back to regular filtering
                }
            }

            // Parse range patterns: variable.property > value, variable.property < value
            if let Some((var_name, prop_name, op, value)) = self.parse_range_filter(predicate) {
                if property_index.indexed_properties().contains(&prop_name) {
                    let entity_ids = match op.as_str() {
                        ">" => {
                            // For greater than, find from value to max
                            let max_value = "~~~~~~~~~~"; // High value for range end
                            property_index.find_range(&prop_name, &value, max_value)
                        }
                        "<" => {
                            // For less than, find from min to value
                            let min_value = ""; // Empty string as min
                            property_index.find_range(&prop_name, min_value, &value)
                        }
                        ">=" => {
                            let max_value = "~~~~~~~~~~";
                            property_index.find_range(&prop_name, &value, max_value)
                        }
                        "<=" => {
                            let min_value = "";
                            property_index.find_range(&prop_name, min_value, &value)
                        }
                        _ => Vec::new(),
                    };

                    if !entity_ids.is_empty() {
                        // TODO: Convert to rows
                        return Ok(None);
                    }
                }
            }
        }

        // No index optimization applicable, use regular filtering
        Ok(None)
    }

    /// Parse simple equality filter: variable.property = 'value'
    pub(in crate::executor) fn parse_equality_filter(
        &self,
        predicate: &str,
    ) -> Option<(String, String, String)> {
        let predicate = predicate.trim();

        // Look for pattern: variable.property = 'value' or variable.property = value
        if let Some(eq_pos) = predicate.find(" = ") {
            let left = predicate[..eq_pos].trim();
            let right = predicate[eq_pos + 3..].trim();

            // Parse left side: variable.property
            if let Some(dot_pos) = left.find('.') {
                let var_name = left[..dot_pos].to_string();
                let prop_name = left[dot_pos + 1..].to_string();

                // Parse right side: remove quotes if present (support both single and double quotes)
                let value = if (right.starts_with('\'') && right.ends_with('\'') && right.len() > 1)
                    || (right.starts_with('"') && right.ends_with('"') && right.len() > 1)
                {
                    right[1..right.len() - 1].to_string()
                } else {
                    right.to_string()
                };

                return Some((var_name, prop_name, value));
            }
        }

        None
    }
    /// Parse range filter: variable.property > value, variable.property < value, etc.
    pub(in crate::executor) fn parse_range_filter(
        &self,
        predicate: &str,
    ) -> Option<(String, String, String, String)> {
        let predicate = predicate.trim();

        // Look for range operators
        let operators = [">=", "<=", ">", "<"];

        for &op in &operators {
            if let Some(op_pos) = predicate.find(op) {
                let left = predicate[..op_pos].trim();
                let right = predicate[op_pos + op.len()..].trim();

                // Parse left side: variable.property
                if let Some(dot_pos) = left.find('.') {
                    let var_name = left[..dot_pos].to_string();
                    let prop_name = left[dot_pos + 1..].to_string();

                    // Parse right side: remove quotes if present (support both single and double quotes)
                    let value =
                        if (right.starts_with('\'') && right.ends_with('\'') && right.len() > 1)
                            || (right.starts_with('"') && right.ends_with('"') && right.len() > 1)
                        {
                            right[1..right.len() - 1].to_string()
                        } else {
                            right.to_string()
                        };

                    return Some((var_name, prop_name, op.to_string(), value));
                }
            }
        }

        None
    }
}
