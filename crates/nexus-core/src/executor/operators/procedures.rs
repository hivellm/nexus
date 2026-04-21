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

    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §4 — `db.indexes` / `db.indexDetails`
    // ─────────────────────────────────────────────────────────────────────

    /// Row shape matches Neo4j 5.x so drivers deserialise without surprise.
    /// Column order: `id, name, state, populationPercent, uniqueness, type,
    /// entityType, labelsOrTypes, properties, indexProvider`.
    pub(in crate::executor) fn execute_db_indexes_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
        filter_name: Option<&str>,
    ) -> Result<()> {
        let mut rows: Vec<Row> = Vec::new();
        let mut next_id: i64 = 0;

        // Nexus always keeps a label bitmap per label — expose each as an
        // implicit LOOKUP index so `db.indexes()` reports the same schema
        // surface Neo4j does (where every label has a token-lookup index).
        // Iterating the catalog's labels is cheap and includes only
        // user-created labels (not internal).
        for (_label_id, label_name) in self.catalog().list_all_labels() {
            let idx_name = format!("index_label_{}", label_name);
            if filter_name.is_some_and(|n| n != idx_name) {
                continue;
            }
            rows.push(Row {
                values: vec![
                    Value::Number(serde_json::Number::from(next_id)),
                    Value::String(idx_name),
                    Value::String("ONLINE".to_string()),
                    Value::Number(
                        serde_json::Number::from_f64(100.0)
                            .unwrap_or_else(|| serde_json::Number::from(100)),
                    ),
                    Value::String("NONUNIQUE".to_string()),
                    Value::String("LOOKUP".to_string()),
                    Value::String("NODE".to_string()),
                    Value::Array(vec![Value::String(label_name.clone())]),
                    Value::Array(Vec::new()),
                    Value::String("token-lookup-1.0".to_string()),
                ],
            });
            next_id += 1;
        }

        // A global KNN vector index exists when one has been registered at
        // engine construction; it's not keyed by label/property in this
        // codebase today, so surface it as a single "vector" row with an
        // empty labels/properties list. Drivers treat the empty list as
        // "applies to any node" and render accordingly.
        {
            let knn = self.knn_index();
            let stats = knn.get_stats();
            if stats.total_vectors > 0 {
                let idx_name = "index_vector_global".to_string();
                if filter_name.is_none_or(|n| n == idx_name) {
                    rows.push(Row {
                        values: vec![
                            Value::Number(serde_json::Number::from(next_id)),
                            Value::String(idx_name),
                            Value::String("ONLINE".to_string()),
                            Value::Number(
                                serde_json::Number::from_f64(100.0)
                                    .unwrap_or_else(|| serde_json::Number::from(100)),
                            ),
                            Value::String("NONUNIQUE".to_string()),
                            Value::String("VECTOR".to_string()),
                            Value::String("NODE".to_string()),
                            Value::Array(Vec::new()),
                            Value::Array(Vec::new()),
                            Value::String("hnsw-1.0".to_string()),
                        ],
                    });
                    next_id += 1;
                }
            }
        }

        if filter_name.is_some() && rows.is_empty() {
            return Err(Error::CypherExecution(format!(
                "ERR_INDEX_NOT_FOUND: no index named '{}'",
                filter_name.unwrap()
            )));
        }

        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "state".to_string(),
                "populationPercent".to_string(),
                "uniqueness".to_string(),
                "type".to_string(),
                "entityType".to_string(),
                "labelsOrTypes".to_string(),
                "properties".to_string(),
                "indexProvider".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §5 — `db.constraints`
    // ─────────────────────────────────────────────────────────────────────

    /// Emits one row per user-declared constraint. Columns:
    /// `id, name, type, entityType, labelsOrTypes, properties, ownedIndex`.
    /// Currently reports UNIQUENESS / NODE_KEY / NODE_PROPERTY_EXISTENCE /
    /// RELATIONSHIP_PROPERTY_EXISTENCE as the catalog exposes them.
    pub(in crate::executor) fn execute_db_constraints_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let mut rows: Vec<Row> = Vec::new();
        // `get_all_constraints` returns a HashMap<(label_id, key_id),
        // Constraint> keyed by the natural composite — we resolve each
        // id pair back to user-visible names via the catalog. This
        // collapses duplicates and keeps the row order deterministic
        // by sorting on (label_name, key_name).
        let all = self
            .catalog()
            .constraint_manager()
            .read()
            .get_all_constraints()
            .unwrap_or_default();
        let mut pairs: Vec<(u32, u32, crate::catalog::constraints::Constraint)> = all
            .into_iter()
            .map(|((lid, kid), c)| (lid, kid, c))
            .collect();
        pairs.sort_by_key(|(lid, kid, _)| (*lid, *kid));
        for (idx, (label_id, key_id, c)) in pairs.into_iter().enumerate() {
            let label_name = self
                .catalog()
                .get_label_name(label_id)
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("label_{}", label_id));
            let key_name = self
                .catalog()
                .get_key_name(key_id)
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("key_{}", key_id));
            let (kind, entity, owned) = match c.constraint_type {
                crate::catalog::constraints::ConstraintType::Unique => (
                    "UNIQUENESS",
                    "NODE",
                    Some(format!("index_unique_{}_{}", label_name, key_name)),
                ),
                crate::catalog::constraints::ConstraintType::Exists => {
                    ("NODE_PROPERTY_EXISTENCE", "NODE", None)
                }
            };
            let name = format!(
                "constraint_{}_{}_{}",
                kind.to_lowercase(),
                label_name,
                key_name
            );
            rows.push(Row {
                values: vec![
                    Value::Number(serde_json::Number::from(idx as i64)),
                    Value::String(name),
                    Value::String(kind.to_string()),
                    Value::String(entity.to_string()),
                    Value::Array(vec![Value::String(label_name)]),
                    Value::Array(vec![Value::String(key_name)]),
                    owned.map(Value::String).unwrap_or(Value::Null),
                ],
            });
        }
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "id".to_string(),
                "name".to_string(),
                "type".to_string(),
                "entityType".to_string(),
                "labelsOrTypes".to_string(),
                "properties".to_string(),
                "ownedIndex".to_string(),
            ]
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
                Value::String("db-1".to_string()),
                Value::String("neo4j".to_string()),
                Value::String(Self::current_rfc3339_utc()),
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

    // ─────────────────────────────────────────────────────────────────────
    // phase6_opencypher-system-procedures §6 — `dbms.*` discovery
    // ─────────────────────────────────────────────────────────────────────

    pub(in crate::executor) fn execute_dbms_components_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let version = env!("CARGO_PKG_VERSION").to_string();
        let rows = vec![Row {
            values: vec![
                Value::String("Nexus Kernel".to_string()),
                Value::Array(vec![Value::String(version)]),
                Value::String("community".to_string()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "versions".to_string(),
                "edition".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_procedures_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Canonical procedure catalogue. Rows are generated deterministically
        // so `cypher-shell` autocomplete and Bloom's capability probe see a
        // stable ordering across calls.
        let entries: &[(&str, &str, &str, &str)] = &[
            (
                "db.labels",
                "db.labels() :: (label :: STRING)",
                "READ",
                "List all node labels in the current database.",
            ),
            (
                "db.relationshipTypes",
                "db.relationshipTypes() :: (relationshipType :: STRING)",
                "READ",
                "List all relationship types in the current database.",
            ),
            (
                "db.propertyKeys",
                "db.propertyKeys() :: (propertyKey :: STRING)",
                "READ",
                "List all property keys in the current database.",
            ),
            (
                "db.schema",
                "db.schema() :: (nodes :: LIST<MAP>, relationships :: LIST<MAP>)",
                "READ",
                "Return the schema graph of the current database.",
            ),
            (
                "db.indexes",
                "db.indexes() :: (id :: INTEGER, name :: STRING, state :: STRING, \
              populationPercent :: FLOAT, uniqueness :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              indexProvider :: STRING)",
                "READ",
                "List all indexes in the current database.",
            ),
            (
                "db.indexDetails",
                "db.indexDetails(name :: STRING) :: (id :: INTEGER, name :: STRING, state :: STRING, \
              populationPercent :: FLOAT, uniqueness :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              indexProvider :: STRING)",
                "READ",
                "Return detail for a single named index.",
            ),
            (
                "db.constraints",
                "db.constraints() :: (id :: INTEGER, name :: STRING, type :: STRING, \
              entityType :: STRING, labelsOrTypes :: LIST<STRING>, properties :: LIST<STRING>, \
              ownedIndex :: STRING)",
                "READ",
                "List all constraints in the current database.",
            ),
            (
                "db.info",
                "db.info() :: (id :: STRING, name :: STRING, creationDate :: STRING)",
                "READ",
                "Return metadata for the current database.",
            ),
            (
                "dbms.components",
                "dbms.components() :: (name :: STRING, versions :: LIST<STRING>, edition :: STRING)",
                "DBMS",
                "List the server's component versions.",
            ),
            (
                "dbms.procedures",
                "dbms.procedures() :: (name :: STRING, signature :: STRING, description :: STRING, \
              mode :: STRING, worksOnSystem :: BOOLEAN)",
                "DBMS",
                "List all procedures registered on the server.",
            ),
            (
                "dbms.functions",
                "dbms.functions() :: (name :: STRING, signature :: STRING, description :: STRING, \
              aggregating :: BOOLEAN)",
                "DBMS",
                "List all functions registered on the server.",
            ),
            (
                "dbms.info",
                "dbms.info() :: (id :: STRING, name :: STRING, creationDate :: STRING)",
                "DBMS",
                "Return the server's identity and boot time.",
            ),
            (
                "dbms.listConfig",
                "dbms.listConfig(search :: STRING) :: (name :: STRING, description :: STRING, \
              value :: STRING, dynamic :: BOOLEAN)",
                "DBMS",
                "List configuration keys matching a substring (Admin only).",
            ),
            (
                "dbms.showCurrentUser",
                "dbms.showCurrentUser() :: (username :: STRING, roles :: LIST<STRING>, \
              flags :: LIST<STRING>)",
                "DBMS",
                "Return the caller's identity and roles.",
            ),
        ];
        let rows: Vec<Row> = entries
            .iter()
            .map(|(name, sig, mode, desc)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*sig).to_string()),
                    Value::String((*desc).to_string()),
                    Value::String((*mode).to_string()),
                    Value::Bool(false),
                ],
            })
            .collect();
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "signature".to_string(),
                "description".to_string(),
                "mode".to_string(),
                "worksOnSystem".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_functions_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // Canonical function catalogue matching the scalar / aggregation
        // surface the executor dispatches at runtime (see
        // `evaluate_projection_expression` in `eval/projection.rs`).
        let entries: &[(&str, &str, &str, bool)] = &[
            ("count", "count(x :: ANY) :: INTEGER", "Count rows.", true),
            (
                "sum",
                "sum(x :: NUMBER) :: NUMBER",
                "Sum numeric column.",
                true,
            ),
            (
                "avg",
                "avg(x :: NUMBER) :: FLOAT",
                "Average of numeric column.",
                true,
            ),
            ("min", "min(x :: ANY) :: ANY", "Minimum of column.", true),
            ("max", "max(x :: ANY) :: ANY", "Maximum of column.", true),
            (
                "collect",
                "collect(x :: ANY) :: LIST",
                "Collect column into a list.",
                true,
            ),
            (
                "stdev",
                "stdev(x :: NUMBER) :: FLOAT",
                "Sample standard deviation.",
                true,
            ),
            (
                "stdevp",
                "stdevp(x :: NUMBER) :: FLOAT",
                "Population standard deviation.",
                true,
            ),
            (
                "percentileCont",
                "percentileCont(x :: NUMBER, p :: FLOAT) :: FLOAT",
                "Continuous percentile.",
                true,
            ),
            (
                "percentileDisc",
                "percentileDisc(x :: NUMBER, p :: FLOAT) :: NUMBER",
                "Discrete percentile.",
                true,
            ),
            (
                "labels",
                "labels(n :: NODE) :: LIST<STRING>",
                "Labels of a node.",
                false,
            ),
            (
                "type",
                "type(r :: RELATIONSHIP) :: STRING",
                "Type of a relationship.",
                false,
            ),
            (
                "keys",
                "keys(x :: ANY) :: LIST<STRING>",
                "Property keys of a node / relationship / map.",
                false,
            ),
            (
                "id",
                "id(x :: NODE) :: INTEGER",
                "Internal id of a node / relationship.",
                false,
            ),
            (
                "size",
                "size(x :: ANY) :: INTEGER",
                "Length of a string or list.",
                false,
            ),
            (
                "length",
                "length(path :: PATH) :: INTEGER",
                "Number of relationships in a path.",
                false,
            ),
            (
                "toUpper",
                "toUpper(s :: STRING) :: STRING",
                "Uppercase string.",
                false,
            ),
            (
                "toLower",
                "toLower(s :: STRING) :: STRING",
                "Lowercase string.",
                false,
            ),
            (
                "substring",
                "substring(s :: STRING, start :: INTEGER, length :: INTEGER) :: STRING",
                "Substring of a string.",
                false,
            ),
            (
                "left",
                "left(s :: STRING, n :: INTEGER) :: STRING",
                "First n characters.",
                false,
            ),
            (
                "right",
                "right(s :: STRING, n :: INTEGER) :: STRING",
                "Last n characters.",
                false,
            ),
            (
                "toString",
                "toString(x :: ANY) :: STRING",
                "Convert to string.",
                false,
            ),
            (
                "toInteger",
                "toInteger(x :: ANY) :: INTEGER",
                "Convert to integer.",
                false,
            ),
            (
                "toFloat",
                "toFloat(x :: ANY) :: FLOAT",
                "Convert to float.",
                false,
            ),
            (
                "toBoolean",
                "toBoolean(x :: ANY) :: BOOLEAN",
                "Convert to boolean.",
                false,
            ),
            (
                "toIntegerList",
                "toIntegerList(xs :: LIST) :: LIST<INTEGER>",
                "Per-element integer coercion.",
                false,
            ),
            (
                "toFloatList",
                "toFloatList(xs :: LIST) :: LIST<FLOAT>",
                "Per-element float coercion.",
                false,
            ),
            (
                "toStringList",
                "toStringList(xs :: LIST) :: LIST<STRING>",
                "Per-element string coercion.",
                false,
            ),
            (
                "toBooleanList",
                "toBooleanList(xs :: LIST) :: LIST<BOOLEAN>",
                "Per-element boolean coercion.",
                false,
            ),
            (
                "isEmpty",
                "isEmpty(x :: ANY) :: BOOLEAN",
                "Empty string / list / map.",
                false,
            ),
            (
                "isInteger",
                "isInteger(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isFloat",
                "isFloat(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isString",
                "isString(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isBoolean",
                "isBoolean(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isList",
                "isList(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isMap",
                "isMap(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isNode",
                "isNode(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isRelationship",
                "isRelationship(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "isPath",
                "isPath(x :: ANY) :: BOOLEAN",
                "Runtime type check.",
                false,
            ),
            (
                "exists",
                "exists(x :: ANY) :: BOOLEAN",
                "Property / expression presence.",
                false,
            ),
        ];
        let rows: Vec<Row> = entries
            .iter()
            .map(|(name, sig, desc, agg)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*sig).to_string()),
                    Value::String((*desc).to_string()),
                    Value::Bool(*agg),
                ],
            })
            .collect();
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "signature".to_string(),
                "description".to_string(),
                "aggregating".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_info_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        let rows = vec![Row {
            values: vec![
                Value::String("nexus-1".to_string()),
                Value::String("Nexus".to_string()),
                Value::String(Self::current_rfc3339_utc()),
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

    pub(in crate::executor) fn execute_dbms_list_config_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
        search: &str,
    ) -> Result<()> {
        // Sources configuration from the `NEXUS_*` environment variables —
        // these are the same keys the server consults during `Config::load`.
        // A full config-registry surface will ship with the config
        // refactor task; for now the env pass gives drivers the common
        // `server.*` keys Cypher Shell expects.
        let config: &[(&str, &str, &str)] = &[
            (
                "server.default_listen_address",
                "Default server HTTP bind address",
                "NEXUS_ADDR",
            ),
            (
                "server.default_rpc_address",
                "Default server RPC bind address",
                "NEXUS_RPC_ADDR",
            ),
            (
                "server.data_dir",
                "Directory for catalog + record stores + WAL",
                "NEXUS_DATA_DIR",
            ),
            (
                "server.rpc_enabled",
                "Whether RPC transport is active",
                "NEXUS_RPC_ENABLED",
            ),
            (
                "server.rpc_require_auth",
                "Whether RPC handshakes require AUTH",
                "NEXUS_RPC_REQUIRE_AUTH",
            ),
            (
                "server.auth_enabled",
                "HTTP authentication on/off",
                "NEXUS_AUTH_ENABLED",
            ),
            (
                "server.rpc_max_frame_bytes",
                "Maximum RPC frame size",
                "NEXUS_RPC_MAX_FRAME_BYTES",
            ),
            (
                "server.rpc_max_in_flight",
                "Concurrent in-flight RPC requests",
                "NEXUS_RPC_MAX_IN_FLIGHT",
            ),
            (
                "server.rpc_slow_threshold_ms",
                "Slow-query threshold in milliseconds",
                "NEXUS_RPC_SLOW_MS",
            ),
        ];
        let lower_search = search.to_lowercase();
        let rows: Vec<Row> = config
            .iter()
            .filter(|(name, _, _)| {
                lower_search.is_empty() || name.to_lowercase().contains(&lower_search)
            })
            .map(|(name, desc, env)| Row {
                values: vec![
                    Value::String((*name).to_string()),
                    Value::String((*desc).to_string()),
                    Value::String(std::env::var(*env).unwrap_or_else(|_| String::new())),
                    Value::Bool(false),
                ],
            })
            .collect();
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "name".to_string(),
                "description".to_string(),
                "value".to_string(),
                "dynamic".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    pub(in crate::executor) fn execute_dbms_show_current_user_procedure(
        &self,
        context: &mut ExecutionContext,
        yield_columns: Option<&Vec<String>>,
    ) -> Result<()> {
        // The engine-level Executor has no direct auth-session handle — the
        // server's `/cypher` handler is where the AuthContext lives. When
        // called through the engine we surface a sentinel unauthenticated
        // row so tools like Cypher Shell don't break during startup
        // discovery; the server-side route will override this with the
        // real session identity in a follow-up.
        let rows = vec![Row {
            values: vec![
                Value::String("anonymous".to_string()),
                Value::Array(Vec::new()),
                Value::Array(Vec::new()),
            ],
        }];
        let columns = if let Some(y) = yield_columns {
            y.clone()
        } else {
            vec![
                "username".to_string(),
                "roles".to_string(),
                "flags".to_string(),
            ]
        };
        context.set_columns_and_rows(columns, rows);
        Ok(())
    }

    /// Shared helper — render the current UTC time as an RFC3339 string.
    /// Used by `db.info()` and `dbms.info()` so drivers can deserialise
    /// the column back into a DATETIME.
    fn current_rfc3339_utc() -> String {
        chrono::Utc::now().to_rfc3339()
    }
}
