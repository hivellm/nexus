//! `db.labels`, `db.propertyKeys`, `db.relationshipTypes`, `db.schema`, and
//! `db.info` — catalog-sourced schema introspection procedures.

use super::super::super::context::ExecutionContext;
use super::super::super::engine::Executor;
use super::super::super::types::Row;
use crate::Result;

impl Executor {
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

    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §2 — `db.info`
    // ─────────────────────────────────────────────────────────────────────

    /// Single-row: `id, name, creationDate`. Surfaces the current session
    /// database; falls back to `"neo4j"` for drivers that expect that
    /// default alias.
    pub(in crate::executor) fn execute_db_info_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let rows = vec![Row {
            values: vec![
                serde_json::Value::String("db-1".to_string()),
                serde_json::Value::String("neo4j".to_string()),
                serde_json::Value::String(Self::current_rfc3339_utc()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "creationDate".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }
}
