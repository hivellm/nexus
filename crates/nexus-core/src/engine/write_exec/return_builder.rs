//! Write-path RETURN construction: builds result rows/columns from the
//! bound node/relationship context, including the relationship-aware
//! evaluator and the complex-expression fallback that re-enters the full
//! executor. Extracted from `engine/write_exec.rs`.

use super::super::Engine;
use crate::executor::eval::temporal_value;
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
                            let mut value = map.get(property).cloned().unwrap_or(Value::Null);
                            // Defense in depth alongside the storage-boundary
                            // canonicalization in `resolve_property_expr_for_create`
                            // — this write path's own inline `RETURN` reads
                            // straight out of storage and never calls
                            // `Executor::execute`, so it must canonicalize
                            // independently (see `temporal_value::canonicalize_value_in_place`'s
                            // doc comment).
                            temporal_value::canonicalize_value_in_place(&mut value);
                            return Ok(value);
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
                // This fast path walks ONE variable's id list, so a RETURN
                // spanning two of them (`MERGE (a…) MERGE (b…) RETURN a.name,
                // b.name`) is not something it can build — it used to error out
                // here. Hand it to the executor instead, exactly as a complex
                // expression is handed over above: the write path's per-variable
                // lists are independent rather than row-aligned, so reconstructing
                // the rows here would mean inventing a row model the rest of the
                // engine does not use.
                return self.build_return_result_with_executor(context, return_clause);
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
                    // Both arms below read straight out of storage and
                    // never call `Executor::execute` — this write path's
                    // own inline `RETURN`, so each must canonicalize a
                    // tagged intermediate temporal value independently
                    // (defense in depth alongside the storage-boundary fix
                    // in `resolve_property_expr_for_create`; see
                    // `temporal_value::canonicalize_value_in_place`'s doc
                    // comment).
                    let mut value = match &item.expression {
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
                    temporal_value::canonicalize_value_in_place(&mut value);
                    row_values.push(value);
                }

                rows.push(executor::Row { values: row_values });
            }
        }

        Ok(executor::ResultSet::new(columns, rows))
    }

    /// Materialise the write path's bindings into a read query and let the real
    /// executor build the RETURN.
    ///
    /// The write path models bindings as `variable -> Vec<node_id>`, which is NOT
    /// a row-aligned column set — the per-variable lists are independent (see
    /// `process_match_clause_multi`, and why relationship `MERGE` takes their
    /// cartesian while `CREATE` zips aligned executor columns). So it cannot
    /// reconstruct multi-variable rows itself without inventing a second row
    /// model. It hands the ids to the executor instead, as
    /// `MATCH (a), (b) WHERE id(a) IN [...] AND id(b) IN [...] RETURN ...`, and
    /// the executor applies the row semantics it already has.
    ///
    /// Every variable the RETURN actually references contributes a pattern and an
    /// id filter. Selecting them from the RETURN rather than from the context is
    /// what makes this deterministic: the previous version took
    /// `context.keys().next()` — an arbitrary key out of a `HashMap` — which
    /// happened to be right only while exactly one variable was ever bound.
    pub(super) fn build_return_result_with_executor(
        &mut self,
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Result<executor::ResultSet> {
        let mut referenced = Self::context_vars_referenced_by(context, return_clause);
        if referenced.is_empty() {
            // A RETURN that names no variable at all — `count(*)`, a literal — still
            // needs the write's rows to count. Materialise every binding, sorted so
            // the generated query does not depend on `HashMap` iteration order.
            // (The previous version reached for `context.keys().next()` here, which
            // was right only because exactly one variable was ever bound.)
            referenced = {
                let mut all: Vec<String> = context.keys().cloned().collect();
                all.sort();
                all
            };
        }

        let empty_columns = || -> Vec<String> {
            return_clause
                .items
                .iter()
                .map(|item| item.alias.clone().unwrap_or_else(|| "?column?".to_string()))
                .collect()
        };

        if referenced.is_empty() {
            return Err(Error::CypherExecution(
                "No context variable for complex RETURN".to_string(),
            ));
        }

        // Any referenced variable bound to nothing collapses the whole row set —
        // the patterns below are conjunctive.
        if referenced
            .iter()
            .any(|var| context.get(var).is_none_or(|ids| ids.is_empty()))
        {
            return Ok(executor::ResultSet::new(empty_columns(), vec![]));
        }

        let patterns = referenced
            .iter()
            .map(|var| format!("({})", var))
            .collect::<Vec<_>>()
            .join(", ");
        let filters = referenced
            .iter()
            .map(|var| {
                let ids = context
                    .get(var)
                    .map(|ids| {
                        ids.iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                format!("id({}) IN [{}]", var, ids)
            })
            .collect::<Vec<_>>()
            .join(" AND ");

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

        let query_str = format!("MATCH {} WHERE {} RETURN {}", patterns, filters, return_str);

        // Execute through the full executor
        let query_obj = executor::Query {
            cypher: query_str,
            params: std::collections::HashMap::new(),
        };

        self.executor.execute(&query_obj)
    }

    /// The context variables this RETURN refers to, in first-appearance order so
    /// the generated query is stable across runs (a `HashMap`'s iteration order is
    /// not). Walks the whole expression tree, so a variable mentioned only inside
    /// a function call or arithmetic still contributes its id filter.
    fn context_vars_referenced_by(
        context: &HashMap<String, Vec<u64>>,
        return_clause: &executor::parser::ReturnClause,
    ) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for item in &return_clause.items {
            Self::collect_context_refs(context, &item.expression, &mut out);
        }
        out
    }

    /// Depth-first walk collecting every referenced name that the write context
    /// actually binds. Names it does not bind are ignored rather than rejected —
    /// a comprehension's own loop variable, for instance, is a reference here but
    /// is bound by the comprehension, not by the write path.
    fn collect_context_refs(
        context: &HashMap<String, Vec<u64>>,
        expr: &executor::parser::Expression,
        out: &mut Vec<String>,
    ) {
        let referenced = match expr {
            executor::parser::Expression::Variable(name) => Some(name),
            executor::parser::Expression::PropertyAccess { variable, .. } => Some(variable),
            _ => None,
        };
        if let Some(name) = referenced
            && context.contains_key(name)
            && !out.contains(name)
        {
            out.push(name.clone());
        }
        for child in crate::executor::semantic_validation::child_exprs(expr) {
            Self::collect_context_refs(context, child, out);
        }
    }
}
