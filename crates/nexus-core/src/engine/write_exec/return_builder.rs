//! Write-path RETURN construction: builds result rows/columns from the
//! bound node/relationship context, including the relationship-aware
//! evaluator and the complex-expression fallback that re-enters the full
//! executor. Extracted from `engine/write_exec.rs`.

use super::super::Engine;
use crate::{Error, Result, executor};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

impl Engine {
    /// Build return result with support for relationship variables
    pub(super) fn build_return_result_with_rels(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        // Check if any return item references a relationship variable
        let has_rel_refs = return_clause
            .items
            .iter()
            .any(|item| self.expression_references_rel(&item.expression, rel_context));

        if !has_rel_refs || rel_context.is_empty() {
            // No relationship references, use regular handling
            return self.build_return_result(context, return_clause);
        }

        // Build result with relationship variable support
        let mut columns = Vec::new();
        let mut row_values = Vec::new();

        for item in &return_clause.items {
            let col_name = item
                .alias
                .clone()
                .unwrap_or_else(|| self.expression_to_string(&item.expression));
            columns.push(col_name);

            let value =
                self.evaluate_return_expression_with_rels(&item.expression, context, rel_context)?;
            row_values.push(value);
        }

        Ok(executor::ResultSet::new(
            columns,
            vec![executor::Row { values: row_values }],
        ))
    }

    /// Check if an expression references a relationship variable
    pub(super) fn expression_references_rel(
        &self,
        expr: &executor::parser::Expression,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
    ) -> bool {
        match expr {
            executor::parser::Expression::Variable(v) => rel_context.contains_key(v),
            executor::parser::Expression::FunctionCall { args, .. } => args
                .iter()
                .any(|arg| self.expression_references_rel(arg, rel_context)),
            executor::parser::Expression::PropertyAccess { variable, .. } => {
                rel_context.contains_key(variable)
            }
            _ => false,
        }
    }

    /// Evaluate a return expression with relationship variable support
    pub(super) fn evaluate_return_expression_with_rels(
        &self,
        expr: &executor::parser::Expression,
        _context: &HashMap<String, Vec<u64>>,
        rel_context: &HashMap<String, Vec<(u64, String)>>,
    ) -> Result<Value> {
        match expr {
            executor::parser::Expression::FunctionCall { name, args } => {
                let func_name = name.to_lowercase();
                if func_name == "type" && args.len() == 1 {
                    // type(r) - return relationship type
                    if let executor::parser::Expression::Variable(var) = &args[0] {
                        if let Some(entries) = rel_context.get(var) {
                            if let Some((_rel_id, rel_type)) = entries.last() {
                                return Ok(Value::String(rel_type.clone()));
                            }
                        }
                    }
                }
                // #14: `count(r)` over an upserted relationship variable —
                // number of distinct edges merged across all UNWIND rows.
                if func_name == "count" && args.len() == 1 {
                    if let executor::parser::Expression::Variable(var) = &args[0] {
                        if let Some(entries) = rel_context.get(var) {
                            let mut ids: Vec<u64> = entries.iter().map(|(id, _)| *id).collect();
                            ids.sort_unstable();
                            ids.dedup();
                            return Ok(Value::Number((ids.len() as u64).into()));
                        }
                    }
                }
                // For other functions, return null for now
                Ok(Value::Null)
            }
            executor::parser::Expression::Variable(var) => {
                if let Some(entries) = rel_context.get(var) {
                    if let Some((rel_id, rel_type)) = entries.last() {
                        // Return relationship as object
                        let mut obj = Map::new();
                        obj.insert("_id".to_string(), Value::Number((*rel_id).into()));
                        obj.insert("_type".to_string(), Value::String(rel_type.clone()));
                        return Ok(Value::Object(obj));
                    }
                }
                Ok(Value::Null)
            }
            // `RETURN r.prop` in the same statement as the MERGE/CREATE
            // that bound `r`. This arm was missing — PropertyAccess on a
            // relationship variable fell through to the catch-all Null
            // below, so `MERGE (a)-[r:T {v:9}]->(b) RETURN r.v` projected
            // null even though the property was persisted correctly
            // (harness case 11b; found live on the 2.5.0-dev image).
            executor::parser::Expression::PropertyAccess { variable, property } => {
                if let Some(entries) = rel_context.get(variable) {
                    if let Some((rel_id, _)) = entries.last() {
                        let props = self.storage.load_relationship_properties(*rel_id)?;
                        if let Some(Value::Object(map)) = props {
                            return Ok(map.get(property).cloned().unwrap_or(Value::Null));
                        }
                    }
                }
                Ok(Value::Null)
            }
            _ => Ok(Value::Null),
        }
    }

    pub(super) fn build_return_result(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        if return_clause.items.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        // Check if we have any complex expressions (function calls, aggregations)
        // If so, delegate to the full executor by converting to a query
        let has_complex_expressions = return_clause.items.iter().any(|item| {
            !matches!(
                &item.expression,
                executor::parser::Expression::Variable(_)
                    | executor::parser::Expression::PropertyAccess { .. }
            )
        });

        if has_complex_expressions {
            // For complex expressions, we need to use the full executor
            // Build a complete query with the context data materialized
            return self.build_return_result_with_executor(context, return_clause);
        }

        // Simple case: only variables and property access
        // Determine which variable(s) we need nodes from
        let mut var_for_iteration: Option<String> = None;
        let mut columns = Vec::new();

        for item in &return_clause.items {
            let (var, col_name) = match &item.expression {
                executor::parser::Expression::Variable(var) => {
                    let col = item.alias.clone().unwrap_or_else(|| var.clone());
                    (var.clone(), col)
                }
                executor::parser::Expression::PropertyAccess { variable, property } => {
                    let col = item
                        .alias
                        .clone()
                        .unwrap_or_else(|| format!("{}.{}", variable, property));
                    (variable.clone(), col)
                }
                _ => unreachable!("Complex expressions should be handled above"),
            };

            if var_for_iteration.is_none() {
                var_for_iteration = Some(var.clone());
            } else if var_for_iteration.as_ref() != Some(&var) {
                return Err(Error::CypherExecution(
                    "Multiple different variables in RETURN not supported for write queries"
                        .to_string(),
                ));
            }
            columns.push(col_name);
        }

        let var_name = match var_for_iteration {
            Some(v) => v,
            None => {
                return Ok(executor::ResultSet::new(columns, vec![]));
            }
        };

        let node_ids = context.get(&var_name).cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        let mut rows = Vec::new();

        for node_id in node_ids {
            if seen.insert(node_id) {
                let mut row_values = Vec::new();

                for item in &return_clause.items {
                    let value = match &item.expression {
                        executor::parser::Expression::Variable(_) => {
                            self.node_to_result_value(node_id)?
                        }
                        executor::parser::Expression::PropertyAccess { property, .. } => {
                            // Get the property value from the node
                            let props = self.storage.load_node_properties(node_id)?;
                            tracing::info!(
                                "[build_return_result] node_id={}, loaded props={:?}",
                                node_id,
                                props
                            );
                            if let Some(Value::Object(map)) = props {
                                let result = map.get(property).cloned().unwrap_or(Value::Null);
                                tracing::info!(
                                    "[build_return_result] property={}, result={:?}",
                                    property,
                                    result
                                );
                                result
                            } else {
                                tracing::info!(
                                    "[build_return_result] property={}, no props found",
                                    property
                                );
                                Value::Null
                            }
                        }
                        _ => Value::Null,
                    };
                    row_values.push(value);
                }

                rows.push(executor::Row { values: row_values });
            }
        }

        Ok(executor::ResultSet::new(columns, rows))
    }

    pub(super) fn build_return_result_with_executor(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        // For complex expressions, convert the context into a MATCH query
        // and let the full executor handle it

        // Find the variable name from context
        let var_name = context.keys().next().ok_or_else(|| {
            Error::CypherExecution("No context variable for complex RETURN".to_string())
        })?;

        let node_ids = context.get(var_name).cloned().unwrap_or_default();

        if node_ids.is_empty() {
            // Build empty result with correct columns
            let columns = return_clause
                .items
                .iter()
                .map(|item| item.alias.clone().unwrap_or_else(|| "?column?".to_string()))
                .collect();
            return Ok(executor::ResultSet::new(columns, vec![]));
        }

        // Build a query like: MATCH (var) WHERE id(var) IN [ids] RETURN ...
        let ids_str = node_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let return_str = return_clause
            .items
            .iter()
            .map(|item| {
                let expr_str = self.expression_to_string(&item.expression);
                if let Some(alias) = &item.alias {
                    format!("{} AS {}", expr_str, alias)
                } else {
                    expr_str
                }
            })
            .collect::<Vec<_>>()
            .join(", ");

        let query_str = format!(
            "MATCH ({}) WHERE id({}) IN [{}] RETURN {}",
            var_name, var_name, ids_str, return_str
        );

        // Execute through the full executor
        let query_obj = executor::Query {
            cypher: query_str,
            params: std::collections::HashMap::new(),
        };

        self.executor.execute(&query_obj)
    }
}
