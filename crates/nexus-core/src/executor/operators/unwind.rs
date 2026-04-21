//! UNWIND / IndexScan / LOAD CSV operators. `execute_unwind` expands a
//! list expression into one row per element; `execute_load_csv` pulls
//! CSV rows from a URL/path; `execute_index_scan_new` dispatches to
//! the label/knn indexes.

use super::super::context::ExecutionContext;
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::Row;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_unwind(
        &self,
        context: &mut ExecutionContext,
        expression: &str,
        variable: &str,
    ) -> Result<()> {
        // Materialize rows from variables if needed (like execute_distinct does)
        if context.result_set.rows.is_empty() && !context.variables.is_empty() {
            let rows = self.materialize_rows_from_variables(context);
            self.update_result_set_from_rows(context, &rows);
        }

        // Parse the expression string
        let mut parser_instance = parser::CypherParser::new(expression.to_string());
        let parsed_expr = parser_instance.parse_expression().map_err(|e| {
            Error::CypherSyntax(format!("Failed to parse UNWIND expression: {}", e))
        })?;

        // If no existing rows, evaluate expression once and create new rows
        if context.result_set.rows.is_empty() {
            // Evaluate expression with empty row context
            let empty_row = HashMap::new();
            let list_value =
                self.evaluate_projection_expression(&empty_row, context, &parsed_expr)?;

            // Convert to array if needed
            let list_items = match list_value {
                Value::Array(items) => items,
                Value::Null => Vec::new(), // NULL list produces no rows
                other => vec![other],      // Single value wraps into single-item list
            };

            // Add variable as column
            context.result_set.columns.push(variable.to_string());

            // Create one row per list item
            for item in list_items {
                let row = Row { values: vec![item] };
                context.result_set.rows.push(row);
            }
        } else {
            // Expand existing rows: for each existing row, evaluate expression and create N new rows
            let existing_rows = std::mem::take(&mut context.result_set.rows);
            let existing_columns = context.result_set.columns.clone();

            // Find or add variable column index
            let var_col_idx = if let Some(idx) = self.get_column_index(variable, &existing_columns)
            {
                idx
            } else {
                // Add new column
                context.result_set.columns.push(variable.to_string());
                existing_columns.len()
            };

            // For each existing row, evaluate expression and create new rows with each list item
            for existing_row in existing_rows.iter() {
                // Convert Row to HashMap for evaluation
                let row_map = self.row_to_map(existing_row, &existing_columns);

                // Evaluate expression in context of this row
                let list_value =
                    self.evaluate_projection_expression(&row_map, context, &parsed_expr)?;

                // Convert to array if needed
                let list_items = match list_value {
                    Value::Array(items) => items,
                    Value::Null => Vec::new(), // NULL list produces no rows
                    other => vec![other],      // Single value wraps into single-item list
                };

                if list_items.is_empty() {
                    // Empty list produces no rows (Cartesian product with empty set)
                    continue;
                }

                for item in &list_items {
                    let mut new_values = existing_row.values.clone();

                    // If var_col_idx equals existing length, append; otherwise replace
                    if var_col_idx >= new_values.len() {
                        new_values.resize(var_col_idx + 1, Value::Null);
                    }
                    new_values[var_col_idx] = item.clone();

                    let new_row = Row { values: new_values };
                    context.result_set.rows.push(new_row);
                }
            }
        }

        Ok(())
    }

    /// Convert Row to HashMap for expression evaluation
    pub(in crate::executor) fn row_to_map(
        &self,
        row: &Row,
        columns: &[String],
    ) -> HashMap<String, Value> {
        let mut map = HashMap::new();
        for (idx, col_name) in columns.iter().enumerate() {
            if let Some(value) = row.values.get(idx) {
                map.insert(col_name.clone(), value.clone());
            }
        }
        map
    }

    /// Execute new index scan operation
    pub(in crate::executor) fn execute_index_scan_new(
        &self,
        context: &mut ExecutionContext,
        _index_name: &str,
        label: &str,
    ) -> Result<()> {
        // Get label ID from catalog
        let label_id = self.catalog().get_or_create_label(label)?;

        // Execute node by label scan
        let nodes = self.execute_node_by_label(label_id)?;
        context.set_variable("n", Value::Array(nodes));

        Ok(())
    }

    /// phase6_opencypher-advanced-types §3.4 — execute a composite
    /// B-tree seek.
    ///
    /// Resolves the label through the catalog, then looks up a
    /// composite index whose `property_keys` start with the supplied
    /// prefix keys. Missing index / label → empty result (the planner
    /// only emits this operator when a matching index exists, so a
    /// miss in practice means the index was dropped between planning
    /// and execution — falling back to empty is safer than raising a
    /// runtime error against a user-visible operator boundary).
    pub(in crate::executor) fn execute_composite_btree_seek(
        &self,
        context: &mut ExecutionContext,
        label: &str,
        variable: &str,
        prefix: &[(String, serde_json::Value)],
    ) -> Result<()> {
        use crate::index::PropertyValue;

        let label_id = match self.catalog().get_label_id(label) {
            Ok(id) => id,
            Err(_) => {
                context.set_variable(variable, Value::Array(Vec::new()));
                return Ok(());
            }
        };

        let prefix_keys: Vec<String> = prefix.iter().map(|(k, _)| k.clone()).collect();

        let Some(registry) = self.composite_btree() else {
            context.set_variable(variable, Value::Array(Vec::new()));
            return Ok(());
        };

        // Find any composite index whose key list starts with the
        // requested prefix. Prefer an exact-arity match (single pass
        // over the registry, stable order).
        let mut matched: Option<_> = None;
        for (lbl, keys, _unique, _name) in registry.list() {
            if lbl != label_id {
                continue;
            }
            if keys.len() < prefix_keys.len() {
                continue;
            }
            if keys[..prefix_keys.len()] != prefix_keys[..] {
                continue;
            }
            matched = registry.find(lbl, &keys);
            break;
        }

        let Some(index_arc) = matched else {
            context.set_variable(variable, Value::Array(Vec::new()));
            return Ok(());
        };

        let prefix_pv: Vec<PropertyValue> = prefix
            .iter()
            .map(|(_, v)| json_to_property_value(v))
            .collect();

        let node_ids = {
            let idx = index_arc.read();
            if prefix_pv.len() == idx.property_keys.len() {
                idx.seek_exact(&prefix_pv)
            } else {
                idx.seek_prefix(&prefix_pv)
            }
        };

        // Materialise each id as a node-shaped map, matching what
        // NodeByLabel emits through `read_node_as_value`.
        let nodes: Vec<Value> = node_ids
            .into_iter()
            .filter_map(|id| self.read_node_as_value(id).ok())
            .filter(|v| !matches!(v, Value::Null))
            .collect();
        context.set_variable(variable, Value::Array(nodes));
        Ok(())
    }
}

/// Map a JSON scalar into the [`crate::index::PropertyValue`] shape used
/// as the key type of the composite B-tree. Non-scalar inputs (maps,
/// arrays) collapse to `Null` — the planner only emits scalar prefix
/// components today, so this is never exercised on a well-formed plan.
fn json_to_property_value(v: &serde_json::Value) -> crate::index::PropertyValue {
    use crate::index::PropertyValue;
    match v {
        serde_json::Value::Null => PropertyValue::Null,
        serde_json::Value::Bool(b) => PropertyValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                PropertyValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                PropertyValue::Float(f)
            } else {
                PropertyValue::Null
            }
        }
        serde_json::Value::String(s) => PropertyValue::String(s.clone()),
        _ => PropertyValue::Null,
    }
}

// Anchor impl block so the helper above is at module scope and the
// earlier `impl Executor { ... }` is properly closed.
impl Executor {
    /// Execute LOAD CSV operator
    pub(in crate::executor) fn execute_load_csv(
        &self,
        context: &mut ExecutionContext,
        url: &str,
        variable: &str,
        with_headers: bool,
        field_terminator: Option<&str>,
    ) -> Result<()> {
        use std::fs;
        use std::io::{BufRead, BufReader};

        // Extract file path from URL (file:///path/to/file.csv or file://path/to/file.csv)
        // Handle both absolute paths (file:///C:/path) and relative paths (file://path)
        // Also handle Windows paths with backslashes
        // Note: file:/// means absolute path (preserve leading slash), file:// means relative path
        let file_path_str = if url.starts_with("file:///") {
            // Absolute path: file:///path -> /path (preserve leading slash)
            let path = &url[7..];
            // On Windows, if path starts with /C:/, remove the leading / to get C:/
            // This handles file:///C:/path correctly
            #[cfg(windows)]
            {
                if path.len() >= 3
                    && path.chars().nth(0) == Some('/')
                    && path.chars().nth(1).map(|c| c.is_ascii_alphabetic()) == Some(true)
                    && path.chars().nth(2) == Some(':')
                {
                    &path[1..]
                } else {
                    path
                }
            }
            #[cfg(not(windows))]
            {
                path
            }
        } else if let Some(stripped) = url.strip_prefix("file://") {
            // Relative path: file://path -> path
            stripped
        } else {
            url
        };

        // Convert to PathBuf to handle path resolution properly
        use std::path::PathBuf;
        let path_buf = PathBuf::from(file_path_str);

        // Try to resolve the path - if it's relative or doesn't exist, try to find it
        let file_path = if path_buf.exists() {
            // Path exists, canonicalize it
            path_buf.canonicalize().unwrap_or(path_buf)
        } else if path_buf.is_relative() {
            // Relative path - try to resolve relative to current directory
            std::env::current_dir()
                .ok()
                .and_then(|cwd| {
                    let joined = cwd.join(&path_buf);
                    if joined.exists() {
                        joined.canonicalize().ok()
                    } else {
                        None
                    }
                })
                .unwrap_or(path_buf)
        } else {
            // Absolute path that doesn't exist - use as-is (will fail with proper error)
            path_buf
        };

        // Read CSV file
        let file = fs::File::open(&file_path).map_err(|e| {
            Error::Internal(format!(
                "Failed to open CSV file '{}': {}",
                file_path.display(),
                e
            ))
        })?;
        let reader = BufReader::new(file);
        let terminator = field_terminator.unwrap_or(",");
        let mut lines = reader.lines();

        // Skip header if WITH HEADERS
        let headers = if with_headers {
            if let Some(Ok(header_line)) = lines.next() {
                header_line
                    .split(terminator)
                    .map(|s| s.trim().to_string())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        // Parse CSV rows
        let mut rows = Vec::new();
        for line_result in lines {
            let line = line_result
                .map_err(|e| Error::Internal(format!("Failed to read CSV line: {}", e)))?;

            if line.trim().is_empty() {
                continue; // Skip empty lines
            }

            let fields: Vec<String> = line
                .split(terminator)
                .map(|s| s.trim().to_string())
                .collect();

            // Convert to Value based on whether we have headers
            let row_value = if with_headers && !headers.is_empty() {
                // Create a map with header keys
                let mut row_map = serde_json::Map::new();
                for (i, header) in headers.iter().enumerate() {
                    let field_value = if i < fields.len() {
                        Value::String(fields[i].clone())
                    } else {
                        Value::Null
                    };
                    row_map.insert(header.clone(), field_value);
                }
                Value::Object(row_map)
            } else {
                // Create an array of field values
                let field_values: Vec<Value> = fields.into_iter().map(Value::String).collect();
                Value::Array(field_values)
            };

            rows.push(row_value);
        }

        // Store rows in result_set
        context.result_set.rows.clear();
        context.result_set.columns = vec![variable.to_string()];

        for row_value in rows {
            context.result_set.rows.push(Row {
                values: vec![row_value],
            });
        }

        // Also update variables for compatibility
        if !context.result_set.rows.is_empty() {
            let row_maps = self.result_set_as_rows(context);
            self.update_variables_from_rows(context, &row_maps);
        }

        Ok(())
    }
}
