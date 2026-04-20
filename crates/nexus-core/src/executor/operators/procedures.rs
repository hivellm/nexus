//! CALL procedure dispatch and the built-in `db.*` introspection procedures
//! (labels, property keys, relationship types, schema). Custom GDS
//! procedures also route through `execute_call_procedure`.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::{ResultSet, Row};
use crate::graph::{algorithms::Graph, procedures::ProcedureRegistry};
use crate::{Error, Result};
use serde_json::{Map, Value};
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
            _ => {}
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

    /// Execute db.labels() procedure
    pub(in crate::executor) fn execute_db_labels_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all labels from catalog - iterate through all label IDs
        // We'll scan from 0 to a reasonable max (or use stats)
        let mut labels = Vec::new();

        // Try to get labels by iterating through possible IDs
        // This is a workaround - ideally Catalog would have list_all_labels()
        for label_id in 0..10000u32 {
            if let Ok(Some(label_name)) = self.catalog().get_label_name(label_id) {
                labels.push(label_name);
            }
        }

        // Convert to rows
        let mut rows = Vec::new();
        for label in labels {
            rows.push(Row {
                values: vec![serde_json::Value::String(label)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            // Use YIELD columns if specified
            yield_cols.clone()
        } else {
            // Default column name
            vec!["label".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.propertyKeys() procedure
    pub(in crate::executor) fn execute_db_property_keys_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all property keys from catalog using public method
        let property_keys: Vec<String> = self
            .catalog()
            .list_all_keys()
            .into_iter()
            .map(|(_, name)| name)
            .collect();

        // Convert to rows
        let mut rows = Vec::new();
        for key in property_keys {
            rows.push(Row {
                values: vec![serde_json::Value::String(key)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["propertyKey".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.relationshipTypes() procedure
    pub(in crate::executor) fn execute_db_relationship_types_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all relationship types from catalog - iterate through possible IDs
        let mut rel_types = Vec::new();

        // Try to get types by iterating through possible IDs
        for type_id in 0..10000u32 {
            if let Ok(Some(type_name)) = self.catalog().get_type_name(type_id) {
                rel_types.push(type_name);
            }
        }

        // Convert to rows
        let mut rows = Vec::new();
        for rel_type in rel_types {
            rows.push(Row {
                values: vec![serde_json::Value::String(rel_type)],
            });
        }

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["relationshipType".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Execute db.schema() procedure
    pub(in crate::executor) fn execute_db_schema_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Get all labels and relationship types from catalog
        let mut labels = Vec::new();
        for label_id in 0..10000u32 {
            if let Ok(Some(label_name)) = self.catalog().get_label_name(label_id) {
                labels.push(label_name);
            }
        }

        let mut rel_types = Vec::new();
        for type_id in 0..10000u32 {
            if let Ok(Some(type_name)) = self.catalog().get_type_name(type_id) {
                rel_types.push(type_name);
            }
        }

        // Convert to JSON arrays
        let nodes_array: Vec<serde_json::Value> = labels
            .into_iter()
            .map(|l| serde_json::json!({"name": l}))
            .collect();
        let relationships_array: Vec<serde_json::Value> = rel_types
            .into_iter()
            .map(|t| serde_json::json!({"name": t}))
            .collect();

        // Create result row
        let rows = vec![Row {
            values: vec![
                serde_json::Value::Array(nodes_array),
                serde_json::Value::Array(relationships_array),
            ],
        }];

        // Set columns based on YIELD clause
        let columns = if let Some(yield_cols) = yield_columns {
            yield_cols.clone()
        } else {
            vec!["nodes".to_string(), "relationships".to_string()]
        };

        context.set_columns_and_rows(columns, rows);
        Ok(())
    }
}
