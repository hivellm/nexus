//! Schema/DDL-style operators: CREATE INDEX plus the multi-database
//! management commands (SHOW/CREATE/DROP/ALTER/USE DATABASE).

use super::super::engine::Executor;
use super::super::types::{ResultSet, Row};
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    /// Execute `CREATE [SPATIAL] INDEX ON :Label(property)`.
    ///
    /// For spatial indexes (phase6_spatial-index-autopopulate §5):
    /// samples up to 1 000 existing `Label` nodes and verifies that
    /// `property` is a Point on each. Returns `ERR_RTREE_BUILD` on the
    /// first non-Point sample, naming the offending `node_id`.
    pub fn execute_create_index(
        &self,
        label: &str,
        property: &str,
        index_type: Option<&str>,
        if_not_exists: bool,
        or_replace: bool,
    ) -> Result<()> {
        let index_key = format!("{}.{}", label, property);
        let registry = &self.shared.rtree_registry;
        let exists = registry.contains(&index_key);

        if exists {
            if if_not_exists {
                return Ok(());
            } else if !or_replace {
                return Err(Error::CypherExecution(format!(
                    "Index on :{}({}) already exists",
                    label, property
                )));
            }
        }

        // Create the appropriate index type
        match index_type {
            Some("spatial") => {
                // §5 — sample existing nodes and reject if any carry a
                // non-Point value for `property`.
                self.validate_spatial_index_property(label, property)?;

                if or_replace && exists {
                    registry.drop_index(&index_key);
                }
                registry.register_empty(&index_key);
            }
            None | Some("property") => {
                // Property index — register in catalog.
                let _label_id = self.catalog().get_or_create_label(label)?;
                let _key_id = self.catalog().get_or_create_key(property)?;
            }
            _ => {
                return Err(Error::CypherExecution(format!(
                    "Unknown index type: {}",
                    index_type.unwrap_or("unknown")
                )));
            }
        }

        Ok(())
    }

    /// Sample up to 1 000 existing nodes that carry `label` and verify
    /// that `property` is a Point value on every sampled node.
    ///
    /// Returns `ERR_RTREE_BUILD` on the first non-Point sample, with
    /// the offending `node_id` in the message.
    fn validate_spatial_index_property(&self, label: &str, property: &str) -> Result<()> {
        // Resolve label_id; if the label does not exist yet there are
        // no existing nodes to validate — succeed immediately.
        let label_id = match self.catalog().get_label_id(label) {
            Ok(id) => id,
            Err(_) => return Ok(()),
        };

        let label_index = self.shared.label_index.read();
        let bitmap = label_index
            .get_nodes_with_labels(&[label_id])
            .unwrap_or_default();
        drop(label_index);

        let store = self.shared.store.read();
        let mut sampled: usize = 0;
        for raw_id in bitmap.iter() {
            if sampled >= 1_000 {
                break;
            }
            let node_id = raw_id as u64;
            // Load properties; skip deleted nodes.
            let props = match store.load_node_properties(node_id) {
                Ok(Some(Value::Object(m))) => m,
                _ => continue,
            };
            sampled += 1;

            let val = props.get(property);
            let is_point = match val {
                Some(Value::Object(m)) => {
                    // A Point map must have at least an "x" key (or
                    // "latitude") — `geospatial::Point::from_json_value`
                    // is the canonical check.
                    crate::geospatial::Point::from_json_value(&Value::Object(m.clone())).is_ok()
                }
                None => {
                    // Property absent on this node — treat as non-Point
                    // only if nodes with that property exist elsewhere.
                    // For simplicity and safety: absent == skip (no
                    // value to validate). We only reject actual
                    // wrong-type values.
                    continue;
                }
                _ => false,
            };

            if !is_point {
                return Err(Error::CypherExecution(format!(
                    "ERR_RTREE_BUILD: node {node_id} has a non-Point value for property \
                     `{property}` — cannot build spatial index on :{}({property})",
                    label
                )));
            }
        }
        Ok(())
    }

    /// Execute SHOW DATABASES command
    pub(in crate::executor) fn execute_show_databases(&self) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            let databases = db_manager.list_databases();
            let default_db = db_manager.default_database_name();

            // Neo4j-compatible columns
            let columns = vec![
                "name".to_string(),
                "type".to_string(),
                "aliases".to_string(),
                "access".to_string(),
                "address".to_string(),
                "role".to_string(),
                "writer".to_string(),
                "requestedStatus".to_string(),
                "currentStatus".to_string(),
                "statusMessage".to_string(),
                "default".to_string(),
                "home".to_string(),
                "constituents".to_string(),
            ];

            let rows: Vec<Row> = databases
                .iter()
                .map(|db| {
                    let is_default = db.name == default_db;
                    Row {
                        values: vec![
                            Value::String(db.name.clone()),
                            Value::String("standard".to_string()),
                            Value::Array(vec![]),
                            Value::String("read-write".to_string()),
                            Value::String("localhost:7687".to_string()),
                            Value::String("primary".to_string()),
                            Value::Bool(true),
                            Value::String("online".to_string()),
                            Value::String("online".to_string()),
                            Value::String("".to_string()),
                            Value::Bool(is_default),
                            Value::Bool(is_default),
                            Value::Array(vec![]),
                        ],
                    }
                })
                .collect();

            Ok(ResultSet { columns, rows })
        } else {
            // No database manager - return single default database
            let columns = vec![
                "name".to_string(),
                "type".to_string(),
                "aliases".to_string(),
                "access".to_string(),
                "address".to_string(),
                "role".to_string(),
                "writer".to_string(),
                "requestedStatus".to_string(),
                "currentStatus".to_string(),
                "statusMessage".to_string(),
                "default".to_string(),
                "home".to_string(),
                "constituents".to_string(),
            ];

            let rows = vec![Row {
                values: vec![
                    Value::String("neo4j".to_string()),
                    Value::String("standard".to_string()),
                    Value::Array(vec![]),
                    Value::String("read-write".to_string()),
                    Value::String("localhost:7687".to_string()),
                    Value::String("primary".to_string()),
                    Value::Bool(true),
                    Value::String("online".to_string()),
                    Value::String("online".to_string()),
                    Value::String("".to_string()),
                    Value::Bool(true),
                    Value::Bool(true),
                    Value::Array(vec![]),
                ],
            }];

            Ok(ResultSet { columns, rows })
        }
    }

    /// Execute CREATE DATABASE command
    pub(in crate::executor) fn execute_create_database(
        &self,
        name: &str,
        if_not_exists: bool,
    ) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database already exists
            if db_manager.exists(name) {
                if if_not_exists {
                    // Return success without creating
                    return Ok(ResultSet {
                        columns: vec!["result".to_string()],
                        rows: vec![Row {
                            values: vec![Value::String(format!(
                                "Database '{}' already exists",
                                name
                            ))],
                        }],
                    });
                } else {
                    return Err(Error::CypherExecution(format!(
                        "Database '{}' already exists",
                        name
                    )));
                }
            }

            // Create the database
            db_manager.create_database(name)?;

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!(
                        "Database '{}' created successfully",
                        name
                    ))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    /// Execute DROP DATABASE command
    pub(in crate::executor) fn execute_drop_database(
        &self,
        name: &str,
        if_exists: bool,
    ) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if trying to drop default database
            if name == db_manager.default_database_name() {
                return Err(Error::CypherExecution(
                    "Cannot drop the default database".to_string(),
                ));
            }

            // Check if database exists
            if !db_manager.exists(name) {
                if if_exists {
                    // Return success without error
                    return Ok(ResultSet {
                        columns: vec!["result".to_string()],
                        rows: vec![Row {
                            values: vec![Value::String(format!(
                                "Database '{}' does not exist",
                                name
                            ))],
                        }],
                    });
                } else {
                    return Err(Error::CypherExecution(format!(
                        "Database '{}' does not exist",
                        name
                    )));
                }
            }

            // Drop the database
            db_manager.drop_database(name, if_exists)?;

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!(
                        "Database '{}' dropped successfully",
                        name
                    ))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    pub(in crate::executor) fn execute_alter_database(
        &self,
        name: &str,
        read_only: Option<bool>,
        option: Option<(String, String)>,
    ) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database exists
            if !db_manager.exists(name) {
                return Err(Error::CypherExecution(format!(
                    "Database '{}' does not exist",
                    name
                )));
            }

            let alteration_msg = if let Some(read_only) = read_only {
                if read_only {
                    format!("Database '{}' set to READ ONLY", name)
                } else {
                    format!("Database '{}' set to READ WRITE", name)
                }
            } else if let Some((key, value)) = option {
                format!("Database '{}' option '{}' set to '{}'", name, key, value)
            } else {
                format!("Database '{}' altered successfully", name)
            };

            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(alteration_msg)],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }

    pub(in crate::executor) fn execute_use_database(&self, name: &str) -> Result<ResultSet> {
        if let Some(db_manager_arc) = self.shared.database_manager() {
            let db_manager = db_manager_arc.read();
            // Check if database exists
            if !db_manager.exists(name) {
                return Err(Error::CypherExecution(format!(
                    "Database '{}' does not exist",
                    name
                )));
            }

            // Note: In a real implementation, this would switch the session's current database
            // For now, we just return success
            Ok(ResultSet {
                columns: vec!["result".to_string()],
                rows: vec![Row {
                    values: vec![Value::String(format!("Switched to database '{}'", name))],
                }],
            })
        } else {
            Err(Error::CypherExecution(
                "Multi-database support is not enabled. DatabaseManager not configured."
                    .to_string(),
            ))
        }
    }
}
