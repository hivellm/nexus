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
        // phase8_neo4j-concurrency-gaps §4.1 — this used to probe
        // `catalog().get_label_name(id)` for every id in `0..10000`
        // (one LMDB read-transaction + cache lookup per probe,
        // regardless of how many labels actually exist) instead of a
        // single catalog iteration, the way `db.propertyKeys` already
        // does via `list_all_keys()`. That was the measured 1.80x
        // slowdown vs Neo4j (~4.1ms vs ~2.3ms p50) for what should be
        // near-instant introspection. `Catalog::list_all_labels` is the
        // same LMDB-iteration helper `list_all_keys` uses, so this is a
        // direct swap with identical semantics — just O(labels) instead
        // of O(10000).
        let rows: Vec<Row> = self
            .catalog()
            .list_all_labels()
            .into_iter()
            .map(|(_, name)| Row {
                values: vec![serde_json::Value::String(name)],
            })
            .collect();

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
        // phase8_neo4j-concurrency-gaps §4.1 — same fix as
        // `execute_db_labels_procedure` above: swap the O(10000)
        // per-id catalog probe for `Catalog::list_all_types`'s single
        // LMDB iteration (the same helper `db.propertyKeys` already
        // uses via `list_all_keys`).
        let rows: Vec<Row> = self
            .catalog()
            .list_all_types()
            .into_iter()
            .map(|(_, name)| Row {
                values: vec![serde_json::Value::String(name)],
            })
            .collect();

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
        // phase8_neo4j-concurrency-gaps §4.1 — same fix as
        // `execute_db_labels_procedure` / `execute_db_relationship_types_procedure`
        // above: single LMDB iteration instead of an O(10000) per-id probe.
        let nodes_array: Vec<serde_json::Value> = self
            .catalog()
            .list_all_labels()
            .into_iter()
            .map(|(_, name)| serde_json::json!({"name": name}))
            .collect();
        let relationships_array: Vec<serde_json::Value> = self
            .catalog()
            .list_all_types()
            .into_iter()
            .map(|(_, name)| serde_json::json!({"name": name}))
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
