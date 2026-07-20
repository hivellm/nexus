//! Central operator dispatch: `execute_operator` pattern-matches on an
//! `Operator` variant and calls into the right operator module. Also
//! hosts the two row-access helpers (`extract_value_from_row`,
//! `get_column_index`) shared by several operators.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::{
    Direction, IndexType, JoinType, Operator, ProjectionItem, ResultSet, Row,
};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_operator(
        &self,
        context: &mut ExecutionContext,
        operator: &Operator,
    ) -> Result<()> {
        match operator {
            Operator::NodeByLabel { label_id, variable } => {
                let nodes = self.execute_node_by_label(*label_id)?;
                tracing::debug!(
                    "execute_operator NodeByLabel: found {} nodes for label_id {}, variable '{}'",
                    nodes.len(),
                    label_id,
                    variable
                );
                self.seed_scan_variable(context, variable, nodes)?;
                tracing::debug!(
                    "execute_operator NodeByLabel: result_set now has {} rows",
                    context.result_set.rows.len()
                );
            }
            Operator::NodeIndexSeek {
                label_id,
                key_id,
                value,
                key_expression,
                variable,
            } => {
                if let Some(expr) = key_expression {
                    // Correlated seek — evaluate the key per driving row
                    // instead of scanning the label and cross-joining. See
                    // `phase0_fix-correlated-predicate-index-seek` §3.
                    self.execute_correlated_index_seek(
                        context, *label_id, *key_id, expr, variable,
                    )?;
                } else {
                    let nodes = self.execute_node_index_seek(*label_id, *key_id, value)?;
                    tracing::debug!(
                        "execute_operator NodeIndexSeek: found {} nodes for label_id {}/key_id {}, variable '{}'",
                        nodes.len(),
                        label_id,
                        key_id,
                        variable
                    );
                    self.seed_scan_variable(context, variable, nodes)?;
                }
            }
            Operator::AllNodesScan { variable } => {
                let nodes = self.execute_all_nodes_scan()?;

                // CRITICAL FIX: Always clear result_set.rows before regenerating from variables
                context.result_set.rows.clear();

                // CRITICAL FIX: Apply Cartesian product if there are existing variables
                if !context.variables.is_empty() {
                    self.apply_cartesian_product(context, variable, nodes)?;
                } else {
                    context.set_variable(variable, Value::Array(nodes));
                }

                // CRITICAL FIX: Materialize rows from variables so Project can process them
                let rows = self.materialize_rows_from_variables(context);
                self.update_result_set_from_rows(context, &rows);
            }
            Operator::Filter { predicate } => {
                self.execute_filter(context, predicate)?;
            }
            Operator::OptionalFilter {
                predicate,
                optional_vars,
            } => {
                self.execute_optional_filter(context, predicate, optional_vars)?;
            }
            Operator::Expand {
                type_ids,
                direction,
                source_var,
                target_var,
                rel_var,
                optional,
            } => {
                self.execute_expand(
                    context, type_ids, *direction, source_var, target_var, rel_var, *optional,
                    None, // Cache not available at this level
                )?;
            }
            Operator::Project { items } => {
                self.execute_project(context, items)?;
            }
            Operator::With { items, distinct } => {
                self.execute_with(context, items, *distinct)?;
            }
            Operator::Limit { count } => {
                self.execute_limit(context, *count)?;
            }
            Operator::Sort { columns, ascending } => {
                self.execute_sort(context, columns, ascending)?;
            }
            Operator::Aggregate {
                group_by,
                aggregations,
                projection_items,
                output_order,
                source: _,
                streaming_optimized: _,
                push_down_optimized: _,
            } => {
                // Use projection items if available, otherwise call without them
                if let Some(items) = projection_items {
                    self.execute_aggregate_with_projections(
                        context,
                        group_by,
                        aggregations,
                        Some(items.as_slice()),
                        output_order.as_deref(),
                    )?;
                } else {
                    self.execute_aggregate(
                        context,
                        group_by,
                        aggregations,
                        output_order.as_deref(),
                    )?;
                }
            }
            Operator::Union {
                left,
                right,
                distinct,
            } => {
                self.execute_union(context, left, right, *distinct)?;
            }
            Operator::EnsureNullRowIfEmpty { vars } => {
                // phase8_optional-match-empty-driver: a top-level
                // OPTIONAL MATCH against an empty driver must
                // surface ONE row with the optional vars bound to
                // NULL (Neo4j contract). The planner inserts this
                // operator after the first OPTIONAL pattern's
                // operators when no prior driver exists.
                if context.result_set.rows.is_empty() {
                    for v in vars {
                        // Bind the variable to NULL in the
                        // execution context so subsequent operators
                        // (Project, Filter on optional vars,
                        // count(...)) see a NULL value rather than
                        // a missing variable.
                        context.set_variable(v, Value::Null);
                    }
                    // Emit a single empty-shaped row. Project /
                    // OptionalFilter downstream rebuild the
                    // visible columns from the variables map.
                    context
                        .result_set
                        .rows
                        .push(crate::executor::types::Row { values: Vec::new() });
                    tracing::debug!(
                        "EnsureNullRowIfEmpty: emitted NULL fallback row for vars {:?}",
                        vars
                    );
                }
            }
            Operator::Create {
                pattern,
                external_id_expr,
                conflict_policy,
            } => {
                let resolved_external_id = if let Some(expr) = external_id_expr.as_ref() {
                    Some(self.resolve_external_id(expr, &context.params)?)
                } else {
                    None
                };
                let policy = super::create::ast_conflict_policy_to_storage(*conflict_policy);
                // phase6_opencypher-subquery-transactions — CREATE
                // reachable through the dispatch path comes from
                // nested subqueries (e.g. `CALL { … CREATE … }`).
                // Pick the right backend:
                //
                // * a context that already carries node references
                //   (`_nexus_id` on any variable, or any data row) →
                //   `execute_create_with_context` so the CREATE
                //   joins those existing nodes (MATCH+CREATE shape);
                // * an empty context (standalone CREATE inside a
                //   subquery driven by a single empty driving row) →
                //   `execute_create_pattern_with_variables`, which
                //   handles anonymous nodes and writes them out
                //   directly. The empty-context path is what the
                //   top-level `execute()` loop uses for standalone
                //   `CREATE …` queries (mod.rs:232) — we just
                //   re-use it from inside the operator dispatcher.
                // Use the row-aware `execute_create_with_context`
                // whenever the outer context carries ANY scope state
                // — variable bindings, populated rows, or both — so
                // CREATE expressions like `{x: i}` resolve `i`
                // against the current row. The standalone
                // `execute_create_pattern_with_variables` path is
                // only safe for the empty-driver case (e.g. a
                // standalone `CALL { CREATE (:T) }` with no preceding
                // clause), where there is no row scope to resolve.
                let context_has_scope = !context.variables.is_empty()
                    || context.result_set.rows.iter().any(|r| !r.values.is_empty());
                if context_has_scope {
                    self.execute_create_with_context(
                        context,
                        pattern,
                        resolved_external_id,
                        policy,
                    )?;
                } else {
                    let (created_nodes, created_rels) = self
                        .execute_create_pattern_with_variables(
                            pattern,
                            resolved_external_id,
                            policy,
                            &context.params,
                        )?;
                    // Register inverse ops on the compensating-undo
                    // buffer (no-op outside a `CALL { … } IN
                    // TRANSACTIONS` batch). The empty-scope path
                    // doesn't have a row to thread through, so we
                    // register from the dispatcher using the IDs the
                    // create helper just minted.
                    use super::super::context::CompensatingUndoOp;
                    for node_id in created_nodes.values() {
                        context.push_undo(CompensatingUndoOp::DeleteNode(*node_id));
                    }
                    for rel_info in created_rels.values() {
                        context.push_undo(CompensatingUndoOp::DeleteRelationship(rel_info.id));
                    }
                    // Surface created entities into the inner ctx so a
                    // following RETURN clause can reference them.
                    let mut columns: Vec<String> = Vec::new();
                    let mut row_values: Vec<Value> = Vec::new();
                    for (var, node_id) in &created_nodes {
                        if let Ok(v) = self.read_node_as_value(*node_id) {
                            context.set_variable(var, v.clone());
                            columns.push(var.clone());
                            row_values.push(v);
                        }
                    }
                    for (var, rel_info) in &created_rels {
                        if let Ok(v) = self.read_relationship_as_value(rel_info) {
                            context.set_variable(var, v.clone());
                            columns.push(var.clone());
                            row_values.push(v);
                        }
                    }
                    if !columns.is_empty() {
                        context.result_set.columns = columns;
                        context.result_set.rows = vec![Row { values: row_values }];
                    }
                }
            }
            Operator::Delete { variables } => {
                self.execute_delete(context, variables, false)?;
            }
            Operator::DetachDelete { variables } => {
                self.execute_delete(context, variables, true)?;
            }
            Operator::Join {
                left,
                right,
                join_type,
                condition,
            } => {
                self.execute_join(context, left, right, *join_type, condition.as_deref())?;
            }
            Operator::IndexScan { index_name, label } => {
                self.execute_index_scan_new(context, index_name, label)?;
            }
            Operator::CompositeBtreeSeek {
                label,
                variable,
                prefix,
            } => {
                self.execute_composite_btree_seek(context, label, variable, prefix)?;
            }
            Operator::Distinct { columns } => {
                self.execute_distinct(context, columns)?;
            }
            Operator::Unwind {
                expression,
                variable,
            } => {
                self.execute_unwind(context, expression, variable)?;
            }
            Operator::VariableLengthPath {
                type_id,
                direction,
                source_var,
                target_var,
                rel_var,
                path_var,
                quantifier,
            } => {
                self.execute_variable_length_path(
                    context, *type_id, *direction, source_var, target_var, rel_var, path_var,
                    quantifier,
                )?;
            }
            Operator::QuantifiedExpand {
                source_var,
                target_var,
                hops,
                inner_nodes,
                inner_where,
                min_length,
                max_length,
                optional,
                mode,
            } => {
                self.execute_quantified_expand(
                    context,
                    source_var,
                    target_var,
                    hops,
                    inner_nodes,
                    inner_where.as_ref(),
                    *min_length,
                    *max_length,
                    *optional,
                    *mode,
                )?;
            }
            Operator::CallProcedure {
                procedure_name,
                arguments,
                yield_columns,
            } => {
                self.execute_call_procedure(
                    context,
                    procedure_name,
                    arguments,
                    yield_columns.as_ref(),
                )?;
            }
            Operator::LoadCsv {
                url,
                variable,
                with_headers,
                field_terminator,
            } => {
                self.execute_load_csv(
                    context,
                    url,
                    variable,
                    *with_headers,
                    field_terminator.as_deref(),
                )?;
            }
            Operator::CreateIndex {
                label,
                property,
                index_type,
                if_not_exists,
                or_replace,
            } => {
                self.execute_create_index(
                    label,
                    property,
                    index_type.as_deref(),
                    *if_not_exists,
                    *or_replace,
                )?;
                // Return empty result set for CREATE INDEX
                context.result_set = ResultSet::new(
                    vec!["index".to_string()],
                    vec![Row {
                        values: vec![Value::String(format!(
                            "{}.{}.{}",
                            label,
                            property,
                            index_type.as_deref().unwrap_or("property")
                        ))],
                    }],
                );
            }
            Operator::ShowDatabases => {
                context.result_set = self.execute_show_databases()?;
            }
            Operator::CreateDatabase {
                name,
                if_not_exists,
            } => {
                context.result_set = self.execute_create_database(name, *if_not_exists)?;
            }
            Operator::DropDatabase { name, if_exists } => {
                context.result_set = self.execute_drop_database(name, *if_exists)?;
            }
            Operator::AlterDatabase {
                name,
                read_only,
                option,
            } => {
                context.result_set =
                    self.execute_alter_database(name, *read_only, option.clone())?;
            }
            Operator::UseDatabase { name } => {
                context.result_set = self.execute_use_database(name)?;
            }
            &Operator::HashJoin { .. } => {
                return Err(Error::Internal(
                    "HashJoin operator not implemented".to_string(),
                ));
            }
            Operator::CallSubquery {
                inner_query,
                in_transactions,
                batch_size,
                concurrency,
                on_error,
                status_var,
                import_list,
            } => {
                self.execute_call_subquery(
                    context,
                    inner_query,
                    *in_transactions,
                    *batch_size,
                    *concurrency,
                    on_error,
                    status_var.as_deref(),
                    import_list.as_deref(),
                )?;
            }
            Operator::SpatialSeek {
                index_id,
                variable,
                mode,
            } => {
                self.execute_spatial_seek(context, index_id, variable, mode)?;
            }
        }
        Ok(())
    }

    /// Execute Join operator

    /// Extract value from a row for a given column name.
    /// Handles PropertyAccess columns (like "n.value") by extracting from the node object.
    pub(in crate::executor) fn extract_value_from_row(
        &self,
        row: &Row,
        column: &str,
        columns: &[String],
    ) -> Option<Value> {
        // First try direct column lookup
        if let Some(idx) = columns.iter().position(|c| c == column) {
            if idx < row.values.len() {
                return Some(row.values[idx].clone());
            }
        }

        // If column is a PropertyAccess (like "n.value"), extract from node object
        if column.contains('.') {
            let parts: Vec<&str> = column.split('.').collect();
            if parts.len() == 2 {
                let var_name = parts[0];
                let prop_name = parts[1];

                // Find the variable in columns
                if let Some(var_idx) = columns.iter().position(|c| c == var_name) {
                    if var_idx < row.values.len() {
                        // Extract property from the node object
                        if let Value::Object(obj) = &row.values[var_idx] {
                            // Node objects can have properties directly or nested
                            if let Some(val) = obj.get(prop_name) {
                                return Some(val.clone());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Get the index of a column by name
    pub(in crate::executor) fn get_column_index(
        &self,
        column_name: &str,
        columns: &[String],
    ) -> Option<usize> {
        columns.iter().position(|col| col == column_name)
    }

    /// Shared post-scan wiring used by `NodeByLabel` and `NodeIndexSeek`:
    /// strips relationship objects from the context, clears stale rows,
    /// applies a Cartesian product or sets the variable directly, then
    /// materialises the result set. Behaviour is identical for both scan
    /// operators — only the node-sourcing step differs.
    fn seed_scan_variable(
        &self,
        context: &mut ExecutionContext,
        variable: &str,
        nodes: Vec<Value>,
    ) -> crate::Result<()> {
        // CRITICAL FIX: Remove relationship objects from variables before creating cartesian product
        // Relationship objects have a "type" property - filter them out to avoid contamination
        context.variables.retain(|_var_name, var_value| {
            let is_relationship = if let Value::Object(obj) = var_value {
                obj.contains_key("type") // Relationships have "type" property
            } else if let Value::Array(arr) = var_value {
                // Check if array contains relationship objects
                arr.iter().any(|v| {
                    if let Value::Object(obj) = v {
                        obj.contains_key("type")
                    } else {
                        false
                    }
                })
            } else {
                false
            };
            !is_relationship // Keep only non-relationship variables
        });

        // CRITICAL FIX: Always clear result_set.rows before regenerating from variables
        context.result_set.rows.clear();

        context.variables.remove(variable);

        // CRITICAL FIX: Apply Cartesian product if there are existing variables
        if !context.variables.is_empty() {
            self.apply_cartesian_product(context, variable, nodes)?;
        } else {
            context.set_variable(variable, Value::Array(nodes));
        }

        // CRITICAL FIX: Materialize rows from variables so Project can process them
        let rows = self.materialize_rows_from_variables(context);
        self.update_result_set_from_rows(context, &rows);
        Ok(())
    }
}
