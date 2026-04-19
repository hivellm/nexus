//! Scan operators and filter-push-down helpers. `execute_node_by_label` and
//! `execute_all_nodes_scan` materialise source variables; `try_index_based_filter`
//! plus its `parse_equality_filter` / `parse_range_filter` helpers attempt to
//! push a Filter down into an index lookup.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::types::Row;
use super::super::{MAX_INTERMEDIATE_ROWS, push_with_row_cap};
use crate::{Error, Result};
use serde_json::Value;

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

            // Skip deleted nodes
            if let Ok(node_record) = self.store().read_node(node_id_u64) {
                if node_record.is_deleted() {
                    continue;
                }
            }

            match self.read_node_as_value(node_id_u64)? {
                Value::Null => continue,
                value => results.push(value),
            }
        }

        Ok(results)
    }

    /// Execute AllNodesScan operator (scan all nodes regardless of label)
    pub(in crate::executor) fn execute_all_nodes_scan(&self) -> Result<Vec<Value>> {
        // Get the total number of nodes from the store
        let total_nodes = self.store().node_count();
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
            // Skip deleted nodes
            if let Ok(node_record) = self.store().read_node(node_id) {
                if node_record.is_deleted() {
                    continue;
                }

                // Read the node as a value
                match self.read_node_as_value(node_id)? {
                    Value::Null => continue,
                    value => {
                        results.push(value);
                    }
                }
            } else {
            }
        }

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
