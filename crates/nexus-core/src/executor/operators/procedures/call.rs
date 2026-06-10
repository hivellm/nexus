//! CALL procedure dispatch — routes a procedure name to its executor method.
//! Built-in `db.*`, `dbms.*`, `db.index.fulltext.*`, `spatial.*`, and
//! `apoc.*` procedures all funnel through `execute_call_procedure`.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::parser;
use super::super::super::types::{ResultSet, Row};
use crate::graph::{algorithms::Graph, procedures::ProcedureRegistry};
use crate::{Error, Result};
use serde_json::Value;
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_call_procedure(
        &self,
        context: &mut ExecutionContext,
        procedure_name: &str,
        arguments: &[parser::Expression],
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Handle built-in db.* procedures that don't need Graph
        match procedure_name {
            "db.labels" => {
                return self.execute_db_labels_procedure(context, yield_columns);
            }
            "db.propertyKeys" => {
                return self.execute_db_property_keys_procedure(context, yield_columns);
            }
            "db.relationshipTypes" => {
                return self.execute_db_relationship_types_procedure(context, yield_columns);
            }
            "db.schema" => {
                return self.execute_db_schema_procedure(context, yield_columns);
            }
            // phase6_opencypher-system-procedures §4, §5, §6 — extended
            // `db.*` / `dbms.*` surface. All procedures are read-only
            // introspection sourced from catalog + in-memory registries.
            "db.indexes" => {
                return self.execute_db_indexes_procedure(context, yield_columns, None);
            }
            "db.indexDetails" => {
                let name = match arguments.first() {
                    Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                        Value::String(s) => s,
                        other => {
                            return Err(Error::CypherExecution(format!(
                                "ERR_INVALID_ARG_TYPE: db.indexDetails requires a STRING \
                                 index name (got {:?})",
                                other
                            )));
                        }
                    },
                    None => {
                        return Err(Error::CypherExecution(
                            "ERR_MISSING_ARG: db.indexDetails requires an index name".to_string(),
                        ));
                    }
                };
                return self.execute_db_indexes_procedure(context, yield_columns, Some(&name));
            }
            "db.constraints" => {
                return self.execute_db_constraints_procedure(context, yield_columns);
            }
            "db.info" => {
                return self.execute_db_info_procedure(context, yield_columns);
            }
            "dbms.components" => {
                return self.execute_dbms_components_procedure(context, yield_columns);
            }
            "dbms.procedures" => {
                return self.execute_dbms_procedures_procedure(context, yield_columns);
            }
            "dbms.functions" => {
                return self.execute_dbms_functions_procedure(context, yield_columns);
            }
            "dbms.info" => {
                return self.execute_dbms_info_procedure(context, yield_columns);
            }
            "dbms.listConfig" => {
                let search = match arguments.first() {
                    Some(expr) => match self.evaluate_expression_in_context(context, expr)? {
                        Value::String(s) => s,
                        Value::Null => String::new(),
                        other => {
                            return Err(Error::CypherExecution(format!(
                                "ERR_INVALID_ARG_TYPE: dbms.listConfig requires a STRING \
                                 search pattern (got {:?})",
                                other
                            )));
                        }
                    },
                    None => String::new(),
                };
                return self.execute_dbms_list_config_procedure(context, yield_columns, &search);
            }
            "dbms.showCurrentUser" => {
                return self.execute_dbms_show_current_user_procedure(context, yield_columns);
            }
            // phase6_opencypher-fulltext-search — Neo4j-compatible
            // `db.index.fulltext.*` surface backed by Tantivy.
            "db.index.fulltext.createNodeIndex" => {
                return self.execute_fts_create(context, arguments, yield_columns, true);
            }
            "db.index.fulltext.createRelationshipIndex" => {
                return self.execute_fts_create(context, arguments, yield_columns, false);
            }
            "db.index.fulltext.queryNodes" => {
                return self.execute_fts_query(context, arguments, yield_columns);
            }
            "db.index.fulltext.queryRelationships" => {
                return self.execute_fts_query(context, arguments, yield_columns);
            }
            "db.index.fulltext.drop" => {
                return self.execute_fts_drop(context, arguments, yield_columns);
            }
            "db.index.fulltext.awaitEventuallyConsistentIndexRefresh" => {
                return self.execute_fts_await_refresh(context, yield_columns);
            }
            "db.index.fulltext.listAvailableAnalyzers" => {
                return self.execute_fts_list_analyzers(context, yield_columns);
            }
            _ => {}
        }

        // phase6_opencypher-geospatial-predicates §7 — spatial.*
        // procedures. `spatial.nearest` needs access to the shared
        // R-tree index registry and goes through a dedicated
        // executor method; the pure-value family
        // (`bbox` / `distance` / `withinBBox` / `withinDistance`
        // / `azimuth` / `interpolate`) routes through the
        // value-only dispatcher the same way apoc.* does.
        if procedure_name == "spatial.nearest" {
            return self.execute_spatial_nearest(context, arguments, yield_columns);
        }
        if procedure_name == "spatial.addPoint" {
            return self.execute_spatial_add_point(context, arguments, yield_columns);
        }
        if procedure_name.starts_with("spatial.") {
            let mut arg_values: Vec<serde_json::Value> = Vec::with_capacity(arguments.len());
            for arg_expr in arguments {
                arg_values.push(self.evaluate_expression_in_context(context, arg_expr)?);
            }
            if let Some(spatial_result) =
                crate::spatial::dispatch(procedure_name, arg_values.clone())?
            {
                let rows: Vec<Row> = spatial_result
                    .rows
                    .into_iter()
                    .map(|values| Row { values })
                    .collect();
                let columns = if let Some(yield_cols) = yield_columns {
                    yield_cols.clone()
                } else {
                    spatial_result.columns
                };
                context.set_columns_and_rows(columns, rows);
                return Ok(());
            }
            // Unknown spatial.* name — surface it explicitly so
            // the caller doesn't fall back into the legacy
            // registry path with its broken arg-packing.
            return Err(Error::CypherExecution(format!(
                "ERR_PROC_NOT_FOUND: `{procedure_name}` is not a known spatial.* procedure. \
                 Known: {:?}",
                crate::spatial::list_procedures(),
            )));
        }

        // phase6_opencypher-apoc-ecosystem — route apoc.* procedures
        // through the in-tree registry. The registry evaluates every
        // argument expression in the current context, passes the
        // resulting JSON values to the APOC handler, and feeds the
        // returned `(columns, rows)` back into the execution context
        // the way other procedures do.
        if procedure_name.starts_with("apoc.") {
            let mut arg_values: Vec<serde_json::Value> = Vec::with_capacity(arguments.len());
            for arg_expr in arguments {
                arg_values.push(self.evaluate_expression_in_context(context, arg_expr)?);
            }
            if let Some(apoc_result) = crate::apoc::dispatch(procedure_name, arg_values)? {
                let rows: Vec<Row> = apoc_result
                    .rows
                    .into_iter()
                    .map(|values| Row { values })
                    .collect();
                let columns = if let Some(yield_cols) = yield_columns {
                    yield_cols.clone()
                } else {
                    apoc_result.columns
                };
                context.set_columns_and_rows(columns, rows);
                return Ok(());
            }
        }

        // Get procedure registry (for now, create a new one - in full implementation would be shared)
        let registry = ProcedureRegistry::new();

        // Find procedure
        let procedure = registry.get(procedure_name).ok_or_else(|| {
            Error::CypherSyntax(format!("Procedure '{}' not found", procedure_name))
        })?;

        // Evaluate arguments
        let mut args_map = HashMap::new();
        for arg_expr in arguments {
            // Evaluate argument expression
            // For now, we'll use a simple evaluation - in a full implementation,
            // we'd need to evaluate expressions in the context of current rows
            let arg_value = self.evaluate_expression_in_context(context, arg_expr)?;
            // Use the expression string representation as key (simplified)
            args_map.insert("arg".to_string(), arg_value);
        }

        // Convert args_map to the format expected by procedures (HashMap<String, Value>)
        // For now, we'll create a simple graph from the current engine state
        // In a full implementation, we'd convert the entire graph from Engine
        let graph = Graph::new(); // Empty graph for now - full implementation would convert from Engine

        // Check if procedure supports streaming and use it for better memory efficiency
        let use_streaming = procedure.supports_streaming();

        if use_streaming {
            // Use streaming execution for better memory efficiency
            use std::sync::{Arc, Mutex};

            let rows = Arc::new(Mutex::new(Vec::new()));
            let columns = Arc::new(Mutex::new(Option::<Vec<String>>::None));

            let rows_clone = rows.clone();
            let columns_clone = columns.clone();

            procedure.execute_streaming(
                &graph,
                &args_map,
                Box::new(move |cols, row| {
                    // Store columns on first call
                    {
                        let mut cols_ref = columns_clone.lock().unwrap();
                        if cols_ref.is_none() {
                            *cols_ref = Some(cols.to_vec());
                        }
                    }

                    // Convert row to Row format
                    rows_clone.lock().unwrap().push(Row {
                        values: row.to_vec(),
                    });

                    Ok(())
                }),
            )?;

            let final_columns = columns.lock().unwrap().clone().ok_or_else(|| {
                Error::CypherSyntax("No columns returned from procedure".to_string())
            })?;

            // Filter columns based on YIELD clause if specified
            let filtered_columns = if let Some(yield_cols) = yield_columns {
                let mut filtered = Vec::new();
                for col in yield_cols {
                    if final_columns.iter().any(|c| c == col) {
                        filtered.push(col.clone());
                    }
                }
                filtered
            } else {
                final_columns
            };

            let final_rows = rows.lock().unwrap().clone();
            context.set_columns_and_rows(filtered_columns, final_rows);
        } else {
            // Use standard execution (collect all results first)
            let procedure_result = procedure
                .execute(&graph, &args_map)
                .map_err(|e| Error::CypherSyntax(format!("Procedure execution failed: {}", e)))?;

            // Convert procedure result to rows
            let mut rows = Vec::new();
            for procedure_row in &procedure_result.rows {
                rows.push(Row {
                    values: procedure_row.clone(),
                });
            }

            // Set columns and rows in context
            let columns = if let Some(yield_cols) = yield_columns {
                // Filter columns based on YIELD clause
                let mut filtered_columns = Vec::new();
                for col in yield_cols {
                    if procedure_result.columns.iter().any(|c| c == col) {
                        filtered_columns.push(col.clone());
                    }
                }
                filtered_columns
            } else {
                // Use all columns from procedure result
                procedure_result.columns.clone()
            };

            context.set_columns_and_rows(columns, rows);
        }

        Ok(())
    }
}
