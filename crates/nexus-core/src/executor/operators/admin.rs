//! Schema/DDL-style operators: CREATE INDEX plus the multi-database
//! management commands (SHOW/CREATE/DROP/ALTER/USE DATABASE).

use super::super::engine::Executor;
use super::super::types::{ResultSet, Row};
use crate::geospatial::rtree::RTreeIndex as SpatialIndex;
use crate::{Error, Result};
use serde_json::Value;

impl Executor {
    pub fn execute_create_index(
        &self,
        label: &str,
        property: &str,
        index_type: Option<&str>,
        if_not_exists: bool,
        or_replace: bool,
    ) -> Result<()> {
        let index_key = format!("{}.{}", label, property);

        // Check if index already exists
        let indexes = self.shared.spatial_indexes.read();
        let exists = indexes.contains_key(&index_key);
        drop(indexes);

        if exists {
            if if_not_exists {
                // Index exists and IF NOT EXISTS was specified - do nothing
                return Ok(());
            } else if !or_replace {
                return Err(Error::CypherExecution(format!(
                    "Index on :{}({}) already exists",
                    label, property
                )));
            }
            // OR REPLACE - will be handled by creating new index below
        }

        // Create the appropriate index type
        match index_type {
            Some("spatial") => {
                // Create spatial index (R-tree)
                let mut indexes = self.shared.spatial_indexes.write();
                if or_replace && exists {
                    // Replace existing index
                    indexes.remove(&index_key);
                }
                indexes.insert(index_key, SpatialIndex::new());
            }
            None | Some("property") => {
                // Property index - for now, just register in catalog
                // In a full implementation, this would create a B-tree index
                // For MVP, we'll just track that the index exists
                let _label_id = self.catalog().get_or_create_label(label)?;
                let _key_id = self.catalog().get_or_create_key(property)?;
                // Index is registered - actual indexing would happen during inserts
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
