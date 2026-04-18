//! Expand and delete operators. `execute_expand` drives relationship
//! traversal (with optional LEFT-OUTER semantics); `execute_delete`
//! is a shim since actual deletion happens at the engine/lib level
//! before execution reaches here.

use super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::engine::Executor;
use super::super::parser;
use super::super::push_with_row_cap;
use super::super::types::{Direction, Operator, ResultSet, Row};
use crate::relationship::{TraversalAction, TraversalError, TraversalVisitor};
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    /// Execute Expand operator
    #[allow(clippy::too_many_arguments)]
    pub(in crate::executor) fn execute_expand(
        &self,
        context: &mut ExecutionContext,
        type_ids: &[u32],
        direction: Direction,
        source_var: &str,
        target_var: &str,
        rel_var: &str,
        optional: bool,
        cache: Option<&crate::cache::MultiLayerCache>,
    ) -> Result<()> {
        // TRACE: Log input source and check for relationships
        let rows_source = if !context.result_set.rows.is_empty() {
            "result_set.rows"
        } else {
            "variables"
        };
        tracing::trace!(
            "execute_expand: input rows from {} (result_set.rows.len()={}, variables.len()={})",
            rows_source,
            context.result_set.rows.len(),
            context.variables.len()
        );

        // Use result_set rows instead of variables to maintain row context from previous operators
        // CRITICAL: Always use result_set_as_rows if available, as it preserves row context
        // from previous operators (like NodeByLabel which creates multiple rows)
        let rows = if !context.result_set.rows.is_empty() {
            let rows_from_result_set = self.result_set_as_rows(context);
            tracing::debug!(
                "Expand: result_set has {} rows, converted to {} row maps",
                context.result_set.rows.len(),
                rows_from_result_set.len()
            );

            // CRITICAL: Don't filter rows by source_var here - process all rows
            // The filtering will happen later when we try to get source_value from each row
            // This ensures we don't accidentally skip valid rows
            // Only use rows_from_result_set directly - don't filter yet
            rows_from_result_set
        } else {
            let materialized = self.materialize_rows_from_variables(context);
            materialized
        };

        // DEBUG: Log number of input rows for debugging relationship expansion issues
        // This helps identify if Expand is receiving all source nodes correctly
        if !rows.is_empty() && !source_var.is_empty() {
            tracing::debug!(
                "Expand operator: processing {} input rows for source_var '{}'",
                rows.len(),
                source_var
            );
            // Log source node IDs to verify all nodes are being processed
            for (idx, row) in rows.iter().enumerate() {
                if let Some(source_value) = row.get(source_var) {
                    if let Some(source_id) = Self::extract_entity_id(source_value) {
                        tracing::debug!(
                            "Expand input row {}: source_var '{}' = node_id {}",
                            idx,
                            source_var,
                            source_id
                        );
                    } else {
                        tracing::debug!(
                            "Expand input row {}: source_var '{}' = {:?} (no entity ID)",
                            idx,
                            source_var,
                            source_value
                        );
                    }
                } else {
                    tracing::debug!(
                        "Expand input row {}: source_var '{}' not found in row (keys: {:?})",
                        idx,
                        source_var,
                        row.keys().collect::<Vec<_>>()
                    );
                }
            }
        }

        let mut expanded_rows = Vec::new();

        // Special case: if source_var is empty or rows is empty, scan all relationships directly
        // This handles queries like MATCH ()-[r:MENTIONS]->() RETURN count(r)
        // Phase 3 Deep Optimization: Use catalog metadata for count queries when possible
        if source_var.is_empty() || rows.is_empty() {
            // Phase 3 Optimization: For count-only queries, use catalog metadata if available
            // This is much faster than scanning all relationships
            if rel_var.is_empty() && !target_var.is_empty() {
                // This looks like a count query - try to use metadata
                // For now, fall back to scanning (will optimize in future)
            }

            // Scan all relationships from storage
            let total_rels = self.store().relationship_count();
            for rel_id in 0..total_rels {
                if let Ok(rel_record) = self.store().read_rel(rel_id) {
                    if rel_record.is_deleted() {
                        continue;
                    }

                    // Copy type_id to local variable (rel_record is packed struct)
                    let record_type_id = rel_record.type_id;
                    let matches_type = type_ids.is_empty() || type_ids.contains(&record_type_id);
                    if !matches_type {
                        continue;
                    }

                    let rel_info = RelationshipInfo {
                        id: rel_id,
                        source_id: rel_record.src_id,
                        target_id: rel_record.dst_id,
                        type_id: rel_record.type_id,
                    };

                    // For bidirectional patterns, return each relationship twice (once for each direction)
                    let directions_to_emit = match direction {
                        Direction::Outgoing | Direction::Incoming => vec![direction],
                        Direction::Both => vec![Direction::Outgoing, Direction::Incoming],
                    };

                    for emit_direction in directions_to_emit {
                        let mut new_row = HashMap::new();

                        // CRITICAL FIX: Determine source and target based on direction
                        // When scanning all relationships (no source nodes provided),
                        // we need to populate BOTH source and target nodes
                        let (source_id, target_id) = match emit_direction {
                            Direction::Outgoing => (rel_record.src_id, rel_record.dst_id),
                            Direction::Incoming => (rel_record.dst_id, rel_record.src_id),
                            Direction::Both => unreachable!(),
                        };

                        // Add source node if source_var is specified
                        if !source_var.is_empty() {
                            let source_node = self.read_node_as_value(source_id)?;
                            new_row.insert(source_var.to_string(), source_node);
                        }

                        // Add target node if target_var is specified
                        if !target_var.is_empty() {
                            let target_node = self.read_node_as_value(target_id)?;
                            new_row.insert(target_var.to_string(), target_node);
                        }

                        // Add relationship if rel_var is specified
                        if !rel_var.is_empty() {
                            let relationship_value = self.read_relationship_as_value(&rel_info)?;
                            new_row.insert(rel_var.to_string(), relationship_value);
                        }

                        push_with_row_cap(&mut expanded_rows, new_row, "Expand (source-less)")?;
                    }
                }
            }
        } else {
            // Normal case: expand from source nodes
            // Only apply target filtering if the target variable is already populated
            // (this happens when we're doing a join-like operation, not a pure expansion)
            let allowed_target_ids: Option<std::collections::HashSet<u64>> =
                if target_var.is_empty() {
                    None
                } else {
                    context
                        .get_variable(target_var)
                        .and_then(|value| match value {
                            Value::Array(values) => {
                                let ids: std::collections::HashSet<u64> =
                                    values.iter().filter_map(Self::extract_entity_id).collect();
                                // Only use the set if it's not empty (empty set means "filter everything out")
                                if ids.is_empty() { None } else { Some(ids) }
                            }
                            _ => None,
                        })
                };

            for (row_idx, row) in rows.iter().enumerate() {
                // CRITICAL: Get source_value from row first, then fallback to context variables
                // This ensures we process each row independently
                let source_value = row
                    .get(source_var)
                    .cloned()
                    .or_else(|| {
                        // If not in row, try to get from context variables
                        // But if it's an Array, we should have already materialized rows
                        // This fallback should only happen in edge cases
                        context.get_variable(source_var).cloned()
                    })
                    .unwrap_or(Value::Null);

                // Handle rows that don't have a valid source value
                if source_value.is_null() {
                    if optional {
                        // OPTIONAL MATCH semantics: preserve the row with NULL for target and rel
                        // This handles chained OPTIONAL MATCHes where the previous optional produced NULL
                        let mut new_row = row.clone();
                        if !target_var.is_empty() {
                            new_row.insert(target_var.to_string(), Value::Null);
                        }
                        if !rel_var.is_empty() {
                            new_row.insert(rel_var.to_string(), Value::Null);
                        }
                        push_with_row_cap(
                            &mut expanded_rows,
                            new_row,
                            "Expand (optional, null source)",
                        )?;
                    } else {
                        tracing::debug!(
                            "Expand: skipping row {} of {} - source_var '{}' is Null",
                            row_idx + 1,
                            rows.len(),
                            source_var
                        );
                    }
                    continue;
                }

                tracing::debug!(
                    "Expand: processing row {} of {}, source_var '{}' = {:?}",
                    row_idx + 1,
                    rows.len(),
                    source_var,
                    if let Some(id) = Self::extract_entity_id(&source_value) {
                        format!("node_id {}", id)
                    } else {
                        format!("{:?}", source_value)
                    }
                );

                // CRITICAL FIX: Handle case where source_value might be an Array
                // This can happen if materialize_rows_from_variables didn't work correctly
                // or if we're in an edge case. If it's an Array, we need to process each element
                // as a separate source node to ensure all nodes are processed.
                // HOWEVER: If source_value is already a single node (not an Array), we should NOT
                // treat it as an Array. This prevents duplicate processing when materialize_rows_from_variables
                // already created proper rows.
                let source_nodes = match &source_value {
                    Value::Array(arr) if !arr.is_empty() => {
                        // Only process as Array if it's actually an Array
                        // This should only happen in edge cases where materialize_rows_from_variables
                        // didn't work correctly
                        arr.clone()
                    }
                    other => {
                        // If it's not an Array, treat as single source node
                        // This is the normal case when rows are properly materialized
                        vec![other.clone()]
                    }
                };

                // Process each source node in the array
                for (source_idx, source_value) in source_nodes.iter().enumerate() {
                    let source_id = match Self::extract_entity_id(source_value) {
                        Some(id) => id,
                        None => {
                            tracing::debug!(
                                "Expand: skipping source node {} (index {}) - no entity ID found",
                                source_idx + 1,
                                source_idx
                            );
                            continue;
                        }
                    };

                    tracing::debug!(
                        "Expand: processing source node {} (index {}) - node_id {} for source_var '{}' (row {}/{})",
                        source_idx + 1,
                        source_idx,
                        source_id,
                        source_var,
                        row_idx + 1,
                        rows.len()
                    );

                    // Phase 8.3: Try to use relationship property index if there are property filters
                    // First, try to get pre-filtered relationships from the index
                    let relationships =
                        if self.enable_relationship_optimizations && !rel_var.is_empty() {
                            // Try to use property index to pre-filter relationships
                            if let Some(indexed_rel_ids) = self
                                .use_relationship_property_index_for_expand(
                                    type_ids, context, rel_var,
                                )?
                            {
                                // Convert relationship IDs to RelationshipInfo
                                let mut indexed_rels = Vec::new();
                                for rel_id in indexed_rel_ids {
                                    if let Ok(rel_record) = self.store().read_rel(rel_id) {
                                        if !rel_record.is_deleted() {
                                            // Copy fields to local variables to avoid packed struct reference issues
                                            let record_type_id = rel_record.type_id;
                                            let record_src_id = rel_record.src_id;
                                            let record_dst_id = rel_record.dst_id;

                                            // Check if relationship matches type and direction filters
                                            let matches_type = type_ids.is_empty()
                                                || type_ids.contains(&record_type_id);
                                            let matches_direction = match direction {
                                                Direction::Outgoing => record_src_id == source_id,
                                                Direction::Incoming => record_dst_id == source_id,
                                                Direction::Both => {
                                                    record_src_id == source_id
                                                        || record_dst_id == source_id
                                                }
                                            };
                                            if matches_type && matches_direction {
                                                indexed_rels.push(RelationshipInfo {
                                                    id: rel_id,
                                                    source_id: record_src_id,
                                                    target_id: record_dst_id,
                                                    type_id: record_type_id,
                                                });
                                            }
                                        }
                                    }
                                }
                                if !indexed_rels.is_empty() {
                                    indexed_rels
                                } else {
                                    // Fallback to standard lookup
                                    self.find_relationships(source_id, type_ids, direction, cache)?
                                }
                            } else {
                                // No index optimization available, use standard lookup
                                self.find_relationships(source_id, type_ids, direction, cache)?
                            }
                        } else {
                            // Standard lookup
                            self.find_relationships(source_id, type_ids, direction, cache)?
                        };

                    tracing::debug!(
                        "Expand: found {} relationships for source node_id {}",
                        relationships.len(),
                        source_id
                    );

                    if relationships.is_empty() {
                        // LEFT OUTER JOIN semantics: preserve row with NULL values when optional=true
                        if optional {
                            // Create a row with NULL for target and relationship variables
                            let mut new_row = row.clone();
                            if !target_var.is_empty() {
                                new_row.insert(target_var.to_string(), Value::Null);
                            }
                            if !rel_var.is_empty() {
                                new_row.insert(rel_var.to_string(), Value::Null);
                            }
                            push_with_row_cap(
                                &mut expanded_rows,
                                new_row,
                                "Expand (optional, no match)",
                            )?;
                        } else {
                            tracing::debug!(
                                "Expand: source node_id {} has no relationships matching criteria, skipping",
                                source_id
                            );
                        }
                        continue;
                    }

                    // Phase 8.3: Apply additional property index filtering if enabled
                    // (for cases where we couldn't pre-filter but can post-filter)
                    let filtered_relationships = if self.enable_relationship_optimizations {
                        self.filter_relationships_by_property_index(
                            &relationships,
                            type_ids.first().copied(),
                            context,
                            rel_var,
                        )?
                    } else {
                        relationships
                    };

                    for (rel_idx, rel_info) in filtered_relationships.iter().enumerate() {
                        let target_id = match direction {
                            Direction::Outgoing => rel_info.target_id,
                            Direction::Incoming => rel_info.source_id,
                            Direction::Both => {
                                // For bidirectional, determine the "other end" based on which end is the source
                                if rel_info.source_id == source_id {
                                    rel_info.target_id
                                } else {
                                    rel_info.source_id
                                }
                            }
                        };

                        let target_node = self.read_node_as_value(target_id)?;

                        // CRITICAL FIX: Check if target variable is already bound in the row
                        // If so, we must ensure the relationship's target matches the bound value
                        // This prevents Cartesian product issues where Expand overwrites the target variable
                        if let Some(existing_target_value) = row.get(target_var) {
                            if let Some(existing_id) =
                                Self::extract_entity_id(existing_target_value)
                            {
                                if existing_id != target_id {
                                    tracing::debug!(
                                        "Expand: skipping relationship {} (rel_id: {}) - target_id {} does not match existing bound value {} in row",
                                        rel_idx + 1,
                                        rel_info.id,
                                        target_id,
                                        existing_id
                                    );
                                    continue;
                                }
                            }
                        }

                        if let Some(ref allowed) = allowed_target_ids {
                            // Only filter if allowed set is non-empty and doesn't contain target
                            if !allowed.is_empty() && !allowed.contains(&target_id) {
                                tracing::debug!(
                                    "Expand: skipping relationship {} (rel_id: {}) - target_id {} not in allowed set",
                                    rel_idx + 1,
                                    rel_info.id,
                                    target_id
                                );
                                continue;
                            }
                        }

                        // CRITICAL FIX: Clone row first to preserve all existing variables
                        // Then update/add source, target, and relationship variables
                        // This ensures all variables from previous operators are preserved
                        let mut new_row = row.clone();
                        // Update source variable (may already exist, but ensure it's correct)
                        new_row.insert(source_var.to_string(), source_value.clone());
                        // Update/add target variable
                        new_row.insert(target_var.to_string(), target_node);
                        // Update/add relationship variable if specified
                        if !rel_var.is_empty() {
                            let relationship_value = self.read_relationship_as_value(rel_info)?;
                            new_row.insert(rel_var.to_string(), relationship_value);
                        }

                        tracing::debug!(
                            "Expand: adding expanded row {} for source node_id {} (relationship {}: rel_id={}, source={}, target={})",
                            expanded_rows.len() + 1,
                            source_id,
                            rel_idx + 1,
                            rel_info.id,
                            rel_info.source_id,
                            rel_info.target_id
                        );
                        push_with_row_cap(&mut expanded_rows, new_row, "Expand")?;
                    }
                }
            }
        }

        tracing::debug!(
            "Expand: created {} expanded rows from {} input rows",
            expanded_rows.len(),
            rows.len()
        );

        // CRITICAL DEBUG: Log detailed information about expanded rows for debugging
        if !expanded_rows.is_empty() {
            tracing::debug!(
                "Expand: Expanded rows summary - Total: {}, Source nodes processed: {}",
                expanded_rows.len(),
                rows.len()
            );
            // Log first few expanded rows for debugging
            for (idx, expanded_row) in expanded_rows.iter().take(5).enumerate() {
                let row_keys: Vec<String> = expanded_row.keys().cloned().collect();
                tracing::debug!(
                    "Expand: Expanded row {} has variables: {:?}",
                    idx + 1,
                    row_keys
                );
            }
        }

        // If no rows were expanded but we had input rows, preserve columns to indicate MATCH was executed but returned empty
        if expanded_rows.is_empty() && !rows.is_empty() {
            // Preserve columns to indicate MATCH was executed but returned empty
            // This will be detected by Aggregate operator via has_match_columns check
            // Don't clear columns - they indicate that MATCH was executed
            tracing::warn!(
                "Expand: No expanded rows created from {} input rows - this may indicate a problem",
                rows.len()
            );
            context.result_set.rows.clear();
            // CRITICAL FIX: Clear variables related to this Expand operation to prevent Project
            // from materializing rows from variables when no relationships were found.
            // This ensures that queries like MATCH (a)-[r:KNOWS]->(b) RETURN a.name don't return
            // rows for nodes that don't have the specified relationship type.
            if !source_var.is_empty() {
                context.variables.remove(source_var);
            }
            if !target_var.is_empty() {
                context.variables.remove(target_var);
            }
            if !rel_var.is_empty() {
                context.variables.remove(rel_var);
            }
        } else {
            // CRITICAL: Always update result_set with all expanded rows
            // This ensures all relationships are included in the result
            // CRITICAL FIX: Clear result_set.rows BEFORE updating to avoid mixing old and new rows
            // This prevents missing rows when Expand processes multiple source nodes
            let rows_before_clear = context.result_set.rows.len();
            context.result_set.rows.clear();
            self.update_variables_from_rows(context, &expanded_rows);
            self.update_result_set_from_rows(context, &expanded_rows);

            // Verify that all expanded rows were added to result_set
            tracing::debug!(
                "Expand: result_set had {} rows before clear, now has {} rows after update (expected {} expanded rows)",
                rows_before_clear,
                context.result_set.rows.len(),
                expanded_rows.len()
            );

            // Assert that all expanded rows were added
            if context.result_set.rows.len() != expanded_rows.len() {
                tracing::warn!(
                    "Expand: Mismatch! result_set has {} rows but {} expanded rows were created - some rows may have been lost in deduplication",
                    context.result_set.rows.len(),
                    expanded_rows.len()
                );
            }
        }

        Ok(())
    }

    /// Execute DELETE or DETACH DELETE operator
    /// Note: This collects node IDs but doesn't actually delete them.
    /// Actual deletion must be handled at Engine level (lib.rs) before executor runs.
    pub(in crate::executor) fn execute_delete(
        &self,
        context: &mut ExecutionContext,
        _variables: &[String],
        _detach: bool,
    ) -> Result<()> {
        // DELETE is handled at Engine level (lib.rs) like CREATE
        // This function is called AFTER deletion has already occurred
        // We just need to clear the result set

        // Clear the result set since deleted nodes shouldn't be returned
        context.result_set.rows.clear();
        context.variables.clear();

        Ok(())
    }
}
