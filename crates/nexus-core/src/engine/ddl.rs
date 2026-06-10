//! DDL command execution: CREATE/DROP INDEX, CREATE/DROP CONSTRAINT,
//! SHOW FUNCTIONS / CONSTRAINTS, CREATE/DROP FUNCTION, LOAD CSV,
//! CALL subquery. Extracted from `engine/mod.rs`.

use super::Engine;
use crate::{Error, Result, catalog, executor};

/// ISSUE #22: cap on the number of rows the legacy CALL IN TRANSACTIONS
/// engine path may materialize inside its single wrapper transaction.
/// Returns the structured `ERR_CALL_IN_TX_RESULT_TOO_LARGE` error past
/// the cap so an enormous subquery fails fast instead of OOMing the
/// server. Default cap is 1M rows; override with
/// `NEXUS_CALL_IN_TX_MAX_ROWS` for constrained deployments (and tests).
pub(super) fn check_call_in_tx_result_cap(row_count: usize) -> Result<()> {
    const CALL_IN_TX_MAX_ROWS: usize = 1_000_000;
    let max_rows = std::env::var("NEXUS_CALL_IN_TX_MAX_ROWS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(CALL_IN_TX_MAX_ROWS);
    if row_count > max_rows {
        return Err(Error::CypherExecution(format!(
            "ERR_CALL_IN_TX_RESULT_TOO_LARGE: CALL {{ ... }} IN TRANSACTIONS \
             produced {row_count} rows (cap {max_rows}). The legacy engine path \
             materializes the whole result in one transaction; reduce the \
             subquery result size (e.g. add a LIMIT or filter).",
        )));
    }
    Ok(())
}

impl Engine {
    /// Execute index management commands (CREATE INDEX, DROP INDEX)
    pub(super) fn execute_index_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut result_rows = Vec::new();
        let columns = vec!["index".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateIndex(create_index) => {
                    // phase6_opencypher-advanced-types §3 — composite
                    // B-tree: any index defined over 2+ properties goes
                    // to the dedicated composite registry, not the
                    // single-column property index.
                    if create_index.properties.len() > 1 {
                        let label_id = self.catalog.get_or_create_label(&create_index.label)?;
                        for prop in &create_index.properties {
                            let _ = self.catalog.get_or_create_key(prop)?;
                        }
                        self.indexes.composite_btree.register(
                            label_id,
                            create_index.properties.clone(),
                            false,
                            create_index.name.clone(),
                            create_index.if_not_exists,
                        )?;
                        let joined = create_index.properties.join(", ");
                        let index_name = format!(":{}({})", create_index.label, joined);
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String("Composite index created".to_string()),
                            ],
                        });
                        continue;
                    }
                    // Get label and property IDs
                    let label_id = self.catalog.get_or_create_label(&create_index.label)?;
                    let property_key_id = self.catalog.get_or_create_key(&create_index.property)?;

                    // Check if index already exists
                    let index_exists = self
                        .indexes
                        .property_index
                        .has_index(label_id, property_key_id);

                    // Handle OR REPLACE
                    if create_index.or_replace && index_exists {
                        // Drop existing index first
                        self.indexes
                            .property_index
                            .drop_index(label_id, property_key_id)?;
                    }

                    // Handle IF NOT EXISTS
                    if !create_index.or_replace && create_index.if_not_exists && index_exists {
                        // Index already exists and IF NOT EXISTS was specified, skip
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(format!(
                                    ":{}({})",
                                    create_index.label, create_index.property
                                )),
                                serde_json::Value::String(
                                    "Index already exists, skipped".to_string(),
                                ),
                            ],
                        });
                        continue;
                    }

                    // Check if index already exists (error if not IF NOT EXISTS or OR REPLACE)
                    if !create_index.or_replace && !create_index.if_not_exists && index_exists {
                        return Err(Error::CypherExecution(format!(
                            "Index on :{}({}) already exists",
                            create_index.label, create_index.property
                        )));
                    }

                    // Check if this is a spatial index
                    let is_spatial = create_index.index_type.as_deref() == Some("spatial");

                    if is_spatial {
                        // Spatial indexes are handled by the executor
                        // Create the spatial index through executor
                        self.executor.execute_create_index(
                            &create_index.label,
                            &create_index.property,
                            Some("spatial"),
                            create_index.if_not_exists,
                            create_index.or_replace,
                        )?;

                        // Return success message
                        let index_name =
                            format!(":{}({})", create_index.label, create_index.property);

                        // Check if index was replaced (we need to check executor's spatial_indexes)
                        // For now, assume it was created unless or_replace was used
                        let message = if create_index.or_replace {
                            format!("Spatial index {} replaced", index_name)
                        } else {
                            format!("Spatial index {} created", index_name)
                        };
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String(message),
                            ],
                        });
                    } else {
                        // Create the property index structure
                        self.indexes
                            .property_index
                            .create_index(label_id, property_key_id)?;

                        // Populate index with existing nodes that have this label and property
                        self.populate_index(label_id, property_key_id)?;

                        // Persist the definition so the index survives a
                        // restart (issue #11).
                        self.catalog
                            .persist_property_index(label_id, property_key_id)?;

                        // Return success message
                        let index_name =
                            format!(":{}({})", create_index.label, create_index.property);
                        let message = if create_index.or_replace && index_exists {
                            format!("Index {} replaced", index_name)
                        } else {
                            format!("Index {} created", index_name)
                        };
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(index_name),
                                serde_json::Value::String(message),
                            ],
                        });
                    }
                }
                executor::parser::Clause::DropIndex(drop_index) => {
                    // Get label and property IDs
                    let label_id = match self.catalog.get_label_id(&drop_index.label) {
                        Ok(id) => id,
                        Err(_) if drop_index.if_exists => {
                            // Label doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    let property_key_id = match self.catalog.get_key_id(&drop_index.property) {
                        Ok(id) => id,
                        Err(_) if drop_index.if_exists => {
                            // Property doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Check if index exists
                    if !self
                        .indexes
                        .property_index
                        .has_index(label_id, property_key_id)
                    {
                        if drop_index.if_exists {
                            // Index doesn't exist and IF EXISTS was specified, skip
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Index on :{}({}) does not exist",
                                drop_index.label, drop_index.property
                            )));
                        }
                    }

                    // Drop the index
                    self.indexes
                        .property_index
                        .drop_index(label_id, property_key_id)?;
                    // Remove the durable definition so it is not rebuilt on
                    // the next restart (issue #11).
                    self.catalog
                        .remove_property_index(label_id, property_key_id)?;

                    // Return success message
                    let index_name = format!(":{}({})", drop_index.label, drop_index.property);
                    let index_name_clone = index_name.clone();
                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(index_name),
                            serde_json::Value::String(format!(
                                "Index {} dropped",
                                index_name_clone
                            )),
                        ],
                    });
                }
                _ => {}
            }
        }

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        Ok(executor::ResultSet::new(columns, result_rows))
    }

    /// Populate an index with existing nodes that have the specified label and property
    pub(super) fn populate_index(&mut self, label_id: u32, property_key_id: u32) -> Result<()> {
        use crate::index::PropertyValue;
        use serde_json::Value as JsonValue;

        // Get property key name
        let property_name = self.catalog.get_key_name(property_key_id)?.ok_or_else(|| {
            Error::CypherExecution(format!("Property key {} not found", property_key_id))
        })?;

        // Get all nodes with this label
        let label_bitmap = self
            .indexes
            .label_index
            .get_nodes_with_labels(&[label_id])?;

        // Iterate through all nodes with this label
        for node_id in label_bitmap.iter() {
            let node_id_u64 = node_id as u64;

            // Load node properties
            if let Some(JsonValue::Object(props)) =
                self.storage.load_node_properties(node_id_u64)?
            {
                // Check if this node has the property we're indexing
                if let Some(prop_value) = props.get(&property_name) {
                    // Convert JSON value to PropertyValue
                    let property_value = match prop_value {
                        JsonValue::String(s) => PropertyValue::String(s.clone()),
                        JsonValue::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                PropertyValue::Integer(i)
                            } else if let Some(f) = n.as_f64() {
                                PropertyValue::Float(f)
                            } else {
                                continue; // Skip invalid number
                            }
                        }
                        JsonValue::Bool(b) => PropertyValue::Boolean(*b),
                        JsonValue::Null => PropertyValue::Null,
                        _ => continue, // Skip arrays and objects
                    };

                    // Add to index
                    self.indexes.property_index.add_property(
                        node_id_u64,
                        label_id,
                        property_key_id,
                        property_value,
                    )?;
                }
            }
        }

        Ok(())
    }

    /// Execute constraint management commands (CREATE CONSTRAINT, DROP CONSTRAINT)
    pub(super) fn execute_constraint_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        // Note on locking: the legacy UNIQUE / EXISTS path takes the
        // constraint-manager write lock lazily inside each branch so
        // the extended-kind path (NODE_KEY / PROPERTY_TYPE /
        // RELATIONSHIP_PROPERTY_EXISTENCE) can take `&mut self` for
        // the programmatic registration APIs without a borrow clash.
        let mut result_rows = Vec::new();
        let columns = vec!["constraint".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::CreateConstraint(create_constraint) => {
                    // phase6_opencypher-constraint-enforcement — NODE
                    // KEY, relationship NOT NULL, and property-type
                    // constraints route through the extended
                    // registration APIs. The legacy UNIQUE / EXISTS
                    // path stays on the LMDB-backed constraint
                    // manager below.
                    match create_constraint.constraint_type {
                        executor::parser::ConstraintType::NodeKey => {
                            let props: Vec<&str> = create_constraint
                                .properties
                                .iter()
                                .map(|s| s.as_str())
                                .collect();
                            self.add_node_key_constraint(
                                &create_constraint.label,
                                &props,
                                create_constraint.name.as_deref(),
                            )?;
                            let display = format!(
                                "NODE_KEY :{} ({})",
                                create_constraint.label,
                                create_constraint.properties.join(", "),
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        executor::parser::ConstraintType::PropertyType => {
                            let ty_name =
                                create_constraint.property_type.clone().unwrap_or_default();
                            let ty = crate::constraints::ScalarType::parse(&ty_name)?;
                            match create_constraint.entity {
                                executor::parser::ConstraintEntity::Node => {
                                    self.add_property_type_constraint(
                                        &create_constraint.label,
                                        &create_constraint.property,
                                        ty,
                                        create_constraint.name.as_deref(),
                                    )?;
                                }
                                executor::parser::ConstraintEntity::Relationship => {
                                    self.add_rel_property_type_constraint(
                                        &create_constraint.label,
                                        &create_constraint.property,
                                        ty,
                                        create_constraint.name.as_deref(),
                                    )?;
                                }
                            }
                            let display = format!(
                                "PROPERTY_TYPE :{}({}) IS :: {}",
                                create_constraint.label,
                                create_constraint.property,
                                ty.name()
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        executor::parser::ConstraintType::Exists
                            if matches!(
                                create_constraint.entity,
                                executor::parser::ConstraintEntity::Relationship
                            ) =>
                        {
                            self.add_rel_not_null_constraint(
                                &create_constraint.label,
                                &create_constraint.property,
                                create_constraint.name.as_deref(),
                            )?;
                            let display = format!(
                                "RELATIONSHIP_PROPERTY_EXISTENCE :{}({})",
                                create_constraint.label, create_constraint.property,
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(display.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {display} created"
                                    )),
                                ],
                            });
                            continue;
                        }
                        _ => {}
                    }
                    // Get label ID
                    let label_id = self.catalog.get_or_create_label(&create_constraint.label)?;

                    // Get property key ID
                    let property_key_id = self
                        .catalog
                        .get_or_create_key(&create_constraint.property)?;

                    // Convert parser constraint type to catalog constraint type.
                    // NODE_KEY and PROPERTY_TYPE were already handled
                    // above; only UNIQUE and (node-scope) EXISTS reach
                    // this point.
                    let constraint_type = match create_constraint.constraint_type {
                        executor::parser::ConstraintType::Unique => {
                            catalog::constraints::ConstraintType::Unique
                        }
                        executor::parser::ConstraintType::Exists => {
                            catalog::constraints::ConstraintType::Exists
                        }
                        executor::parser::ConstraintType::NodeKey
                        | executor::parser::ConstraintType::PropertyType => {
                            unreachable!("handled above")
                        }
                    };

                    // Take the constraint-manager write lock only
                    // for the legacy path — the extended-kind
                    // registration above needs &mut self and can't
                    // share the lock.
                    let mut constraint_manager = self.catalog.constraint_manager().write();

                    // Check if constraint already exists
                    let constraint_exists = constraint_manager
                        .has_constraint(constraint_type, label_id, property_key_id)
                        .unwrap_or(false);

                    // Handle IF NOT EXISTS
                    if create_constraint.if_not_exists && constraint_exists {
                        // Constraint already exists and IF NOT EXISTS was specified, skip
                        let constraint_name = format!(
                            ":{}({}) IS {}",
                            create_constraint.label,
                            create_constraint.property,
                            match constraint_type {
                                catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                catalog::constraints::ConstraintType::Exists => "EXISTS",
                            }
                        );
                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(constraint_name.clone()),
                                serde_json::Value::String(
                                    "Constraint already exists, skipped".to_string(),
                                ),
                            ],
                        });
                        continue;
                    }

                    // Create constraint
                    match constraint_manager.create_constraint(
                        constraint_type,
                        label_id,
                        property_key_id,
                    ) {
                        Ok(_) => {
                            // Constraint created successfully
                            let constraint_name = format!(
                                ":{}({}) IS {}",
                                create_constraint.label,
                                create_constraint.property,
                                match constraint_type {
                                    catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                    catalog::constraints::ConstraintType::Exists => "EXISTS",
                                }
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(constraint_name.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {} created",
                                        constraint_name
                                    )),
                                ],
                            });
                        }
                        Err(Error::CypherExecution(_)) if create_constraint.if_not_exists => {
                            // Constraint already exists and IF NOT EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    }
                }
                executor::parser::Clause::DropConstraint(drop_constraint) => {
                    // Get label ID
                    let label_id = match self.catalog.get_label_id(&drop_constraint.label) {
                        Ok(id) => id,
                        Err(_) if drop_constraint.if_exists => {
                            // Label doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Get property key ID
                    let property_key_id = match self.catalog.get_key_id(&drop_constraint.property) {
                        Ok(id) => id,
                        Err(_) if drop_constraint.if_exists => {
                            // Property doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Err(e) => return Err(e),
                    };

                    // Convert parser constraint type to catalog constraint type
                    let constraint_type = match drop_constraint.constraint_type {
                        executor::parser::ConstraintType::Unique => {
                            catalog::constraints::ConstraintType::Unique
                        }
                        executor::parser::ConstraintType::Exists => {
                            catalog::constraints::ConstraintType::Exists
                        }
                        // NODE_KEY / PROPERTY_TYPE drop is a no-op in
                        // this release — the in-memory extended
                        // registry is recreated per engine lifetime
                        // and DROP CONSTRAINT wiring for the new
                        // kinds lands alongside the LMDB persistence
                        // follow-up. Report success so DDL scripts
                        // stay idempotent.
                        executor::parser::ConstraintType::NodeKey
                        | executor::parser::ConstraintType::PropertyType => {
                            continue;
                        }
                    };

                    let mut constraint_manager = self.catalog.constraint_manager().write();

                    // Drop constraint
                    match constraint_manager.drop_constraint(
                        constraint_type,
                        label_id,
                        property_key_id,
                    ) {
                        Ok(true) => {
                            // Constraint dropped successfully
                            let constraint_name = format!(
                                ":{}({}) IS {}",
                                drop_constraint.label,
                                drop_constraint.property,
                                match constraint_type {
                                    catalog::constraints::ConstraintType::Unique => "UNIQUE",
                                    catalog::constraints::ConstraintType::Exists => "EXISTS",
                                }
                            );
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(constraint_name.clone()),
                                    serde_json::Value::String(format!(
                                        "Constraint {} dropped",
                                        constraint_name
                                    )),
                                ],
                            });
                        }
                        Ok(false) if drop_constraint.if_exists => {
                            // Constraint doesn't exist and IF EXISTS was specified, skip
                            continue;
                        }
                        Ok(false) => {
                            return Err(Error::CypherExecution(format!(
                                "Constraint does not exist on :{} ({})",
                                drop_constraint.label, drop_constraint.property
                            )));
                        }
                        Err(e) => return Err(e),
                    }
                }
                _ => {}
            }
        }

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        Ok(executor::ResultSet::new(columns, result_rows))
    }

    /// Execute function management commands (SHOW FUNCTIONS, CREATE FUNCTION, DROP FUNCTION)
    pub(super) fn execute_function_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut result_rows = Vec::new();
        let columns = vec!["function".to_string(), "message".to_string()];

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::ShowFunctions => {
                    // List all registered UDFs
                    let udf_names = self.executor.udf_registry().list();

                    // Also get UDFs from catalog (signatures only)
                    let catalog_udfs = self.catalog.list_udfs().unwrap_or_default();

                    // Combine and deduplicate
                    let mut all_functions: std::collections::HashSet<String> =
                        udf_names.into_iter().collect();
                    for name in catalog_udfs {
                        all_functions.insert(name);
                    }

                    // Sort for consistent output
                    let mut sorted_functions: Vec<String> = all_functions.into_iter().collect();
                    sorted_functions.sort();

                    for func_name in sorted_functions {
                        // Try to get signature from catalog
                        let description = if let Ok(Some(sig)) = self.catalog.get_udf(&func_name) {
                            sig.description
                                .unwrap_or_else(|| format!("Function {} registered", func_name))
                        } else {
                            format!("Function {} registered", func_name)
                        };

                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(func_name),
                                serde_json::Value::String(description),
                            ],
                        });
                    }

                    // If no functions, return empty result
                    if result_rows.is_empty() {
                        return Ok(executor::ResultSet::new(
                            vec!["function".to_string()],
                            vec![],
                        ));
                    }
                }
                executor::parser::Clause::ShowConstraints => {
                    // Get all constraints from catalog
                    let constraint_mgr = self.catalog.constraint_manager();
                    let constraints = constraint_mgr.read().get_all_constraints()?;

                    // Sort by label_id and property_key_id for consistent output
                    let mut sorted_constraints: Vec<_> = constraints.into_iter().collect();
                    sorted_constraints.sort_by(|a, b| {
                        a.0.cmp(&b.0) // Sort by (label_id, property_key_id) tuple
                    });

                    for ((label_id, prop_key_id), constraint) in sorted_constraints {
                        // Get label name
                        let label_name = self
                            .catalog
                            .get_label_name(label_id)?
                            .unwrap_or_else(|| format!("Label_{}", label_id));

                        // Get property key name
                        let prop_name = self
                            .catalog
                            .get_key_name(prop_key_id)?
                            .unwrap_or_else(|| format!("Property_{}", prop_key_id));

                        // Determine constraint type string
                        let constraint_type = match constraint.constraint_type {
                            catalog::constraints::ConstraintType::Unique => "UNIQUE",
                            catalog::constraints::ConstraintType::Exists => "EXISTS",
                        };

                        // Create description in Neo4j format
                        let description = match constraint.constraint_type {
                            catalog::constraints::ConstraintType::Unique => {
                                format!(
                                    "CONSTRAINT ON (n:{}) ASSERT n.{} IS UNIQUE",
                                    label_name, prop_name
                                )
                            }
                            catalog::constraints::ConstraintType::Exists => {
                                format!(
                                    "CONSTRAINT ON (n:{}) ASSERT exists(n.{})",
                                    label_name, prop_name
                                )
                            }
                        };

                        result_rows.push(executor::Row {
                            values: vec![
                                serde_json::Value::String(label_name),
                                serde_json::Value::String(prop_name),
                                serde_json::Value::String(constraint_type.to_string()),
                                serde_json::Value::String(description),
                            ],
                        });
                    }

                    // Return result with appropriate columns
                    return Ok(executor::ResultSet::new(
                        vec![
                            "label".to_string(),
                            "property".to_string(),
                            "type".to_string(),
                            "description".to_string(),
                        ],
                        result_rows,
                    ));
                }
                executor::parser::Clause::CreateFunction(create_function) => {
                    // Check if function already exists
                    let function_exists =
                        self.executor.udf_registry().contains(&create_function.name)
                            || self
                                .catalog
                                .get_udf(&create_function.name)
                                .unwrap_or(None)
                                .is_some();

                    if function_exists {
                        if create_function.if_not_exists {
                            // Function already exists and IF NOT EXISTS was specified, skip
                            result_rows.push(executor::Row {
                                values: vec![
                                    serde_json::Value::String(create_function.name.clone()),
                                    serde_json::Value::String(
                                        "Function already exists, skipped".to_string(),
                                    ),
                                ],
                            });
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Function '{}' already exists",
                                create_function.name
                            )));
                        }
                    }

                    // Convert parser UdfParameter to udf::UdfParameter
                    let udf_parameters: Vec<crate::udf::UdfParameter> = create_function
                        .parameters
                        .iter()
                        .map(|p| crate::udf::UdfParameter {
                            name: p.name.clone(),
                            param_type: p.param_type.clone(),
                            required: p.required,
                            default: p.default.clone(),
                        })
                        .collect();

                    // Create UDF signature
                    let signature = crate::udf::UdfSignature {
                        name: create_function.name.clone(),
                        parameters: udf_parameters,
                        return_type: create_function.return_type.clone(),
                        description: create_function.description.clone(),
                    };

                    // Store signature in catalog
                    self.catalog.store_udf(&signature)?;

                    // Note: The actual function implementation must be registered via API/plugin system
                    // CREATE FUNCTION only stores the signature
                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(create_function.name.clone()),
                            serde_json::Value::String(format!(
                                "Function signature '{}' stored. Implementation must be registered via API/plugin system.",
                                create_function.name
                            )),
                        ],
                    });
                }
                executor::parser::Clause::DropFunction(drop_function) => {
                    // Check if function exists
                    let function_exists =
                        self.executor.udf_registry().contains(&drop_function.name)
                            || self
                                .catalog
                                .get_udf(&drop_function.name)
                                .unwrap_or(None)
                                .is_some();

                    if !function_exists {
                        if drop_function.if_exists {
                            // Function doesn't exist and IF EXISTS was specified, skip
                            continue;
                        } else {
                            return Err(Error::CypherExecution(format!(
                                "Function '{}' does not exist",
                                drop_function.name
                            )));
                        }
                    }

                    // Remove from UDF registry if registered
                    if self.executor.udf_registry().contains(&drop_function.name) {
                        self.executor
                            .udf_registry_mut()
                            .unregister(&drop_function.name)?;
                    }

                    // Remove from catalog
                    self.catalog.remove_udf(&drop_function.name)?;

                    result_rows.push(executor::Row {
                        values: vec![
                            serde_json::Value::String(drop_function.name.clone()),
                            serde_json::Value::String(format!(
                                "Function '{}' dropped",
                                drop_function.name
                            )),
                        ],
                    });
                }
                _ => {}
            }
        }

        // If no rows were added (all commands were skipped), return empty result
        if result_rows.is_empty() {
            return Ok(executor::ResultSet::new(vec![], vec![]));
        }

        Ok(executor::ResultSet::new(columns, result_rows))
    }

    /// Execute LOAD CSV commands
    /// LOAD CSV loads CSV data and binds each row to a variable
    /// Typically used with FOREACH or UNWIND to process rows
    pub(super) fn execute_load_csv_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        use std::fs;
        use std::path::Path;

        let mut all_rows = Vec::new();
        let mut columns = Vec::new();

        for clause in &ast.clauses {
            if let executor::parser::Clause::LoadCsv(load_csv) = clause {
                // Extract file path from URL (support file:///path/to/file.csv)
                let file_path = if load_csv.url.starts_with("file:///") {
                    let path = &load_csv.url[8..]; // Remove "file:///"
                    // On Windows, if path starts with /C:/, remove the leading / to get C:/
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
                } else if load_csv.url.starts_with("file://") {
                    &load_csv.url[7..] // Remove "file://"
                } else {
                    &load_csv.url // Use as-is if no protocol
                };

                let path = Path::new(file_path);
                if !path.exists() {
                    return Err(Error::CypherExecution(format!(
                        "CSV file not found: {}",
                        file_path
                    )));
                }

                // Read CSV file
                let content = fs::read_to_string(path).map_err(|e| {
                    Error::CypherExecution(format!("Failed to read CSV file: {}", e))
                })?;

                // Parse CSV lines
                let field_terminator = load_csv.field_terminator.as_deref().unwrap_or(",");
                let mut lines = content.lines();

                // Skip header if WITH HEADERS
                if load_csv.with_headers {
                    lines.next(); // Skip header line
                }

                // Parse each row
                for line in lines {
                    if line.trim().is_empty() {
                        continue;
                    }

                    // Simple CSV parsing (split by field terminator)
                    // Note: This doesn't handle quoted fields with commas inside
                    // For production, should use a proper CSV parser library
                    let fields: Vec<String> = line
                        .split(field_terminator)
                        .map(|s| s.trim().to_string())
                        .collect();

                    // Convert fields to JSON array
                    let row_value: serde_json::Value =
                        fields.into_iter().map(serde_json::Value::String).collect();

                    all_rows.push(executor::Row {
                        values: vec![row_value],
                    });
                }

                // Set columns if not already set
                if columns.is_empty() {
                    columns = vec![load_csv.variable.clone()];
                }
            }
        }

        Ok(executor::ResultSet::new(columns, all_rows))
    }

    /// Execute CALL subquery commands
    pub(super) fn execute_call_subquery_commands(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<executor::ResultSet> {
        let mut all_results = Vec::new();
        let mut columns = Vec::new();

        for clause in &ast.clauses {
            if let executor::parser::Clause::CallSubquery(call_subquery) = clause {
                if call_subquery.in_transactions {
                    // phase6_opencypher-subquery-transactions — the
                    // extended suffix clauses (IN CONCURRENT, ON ERROR
                    // non-FAIL, REPORT STATUS) land with the planner
                    // operator in a later slice of the task. Reject
                    // loudly here instead of silently ignoring fields
                    // the caller spelled out, so production users do
                    // not get FAIL semantics when they asked for
                    // RETRY / CONTINUE / BREAK.
                    if call_subquery.concurrency.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             IN CONCURRENT TRANSACTIONS lands with \
                             the planner operator in a follow-up \
                             slice of phase6_opencypher-subquery-\
                             transactions"
                                .to_string(),
                        ));
                    }
                    if !matches!(
                        call_subquery.on_error,
                        executor::parser::OnErrorPolicy::Fail
                    ) {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             ON ERROR CONTINUE / BREAK / RETRY \
                             lands with the planner operator in a \
                             follow-up slice of \
                             phase6_opencypher-subquery-transactions"
                                .to_string(),
                        ));
                    }
                    if call_subquery.status_var.is_some() {
                        return Err(Error::CypherExecution(
                            "ERR_CALL_IN_TX_NOT_IMPLEMENTED: \
                             REPORT STATUS AS <var> lands with the \
                             planner operator in a follow-up slice \
                             of phase6_opencypher-subquery-\
                             transactions"
                                .to_string(),
                        ));
                    }
                    // `... IN TRANSACTIONS OF n ROWS` controls *commit
                    // granularity*, not re-execution: the inner subquery runs
                    // ONCE and its writes are committed (here, in a single
                    // transaction). The previous implementation re-ran the
                    // whole subquery every loop iteration against the same
                    // dataset and only broke when it returned zero rows or
                    // fewer than `batch_size`; for any subquery returning
                    // `>= batch_size` stable rows (e.g. a backfill
                    // `CALL { ... } IN TRANSACTIONS OF 1000 ROWS`) the
                    // termination condition was never met — an infinite loop
                    // that pinned the engine write lock at 100% CPU with no
                    // active-query log (issue #12). Run once and commit.
                    // #22: `OF n ROWS` per-batch commit granularity is not yet
                    // implemented — the subquery runs once and its full result
                    // is materialized in a single transaction. Cap the
                    // materialized result so an enormous subquery returns a
                    // clear, bounded error instead of OOMing the server while
                    // building `all_results` + the response.
                    let _batch_size = call_subquery.batch_size.unwrap_or(1000);
                    let mut tx = self.transaction_manager.write().begin_write()?;
                    let subquery_result = self.execute_cypher_ast(&call_subquery.query)?;
                    if let Err(e) = check_call_in_tx_result_cap(subquery_result.rows.len()) {
                        // Clean abort: release the wrapper write
                        // transaction before surfacing the cap error so
                        // nothing is committed and no write lock leaks.
                        self.transaction_manager.write().abort(&mut tx).ok();
                        return Err(e);
                    }
                    if columns.is_empty() {
                        columns = subquery_result.columns.clone();
                    }
                    all_results.extend(subquery_result.rows);
                    self.transaction_manager.write().commit(&mut tx)?;
                } else {
                    // Execute subquery normally (no batching)
                    let subquery_result = self.execute_cypher_ast(&call_subquery.query)?;

                    if columns.is_empty() {
                        columns = subquery_result.columns.clone();
                    }
                    all_results.extend(subquery_result.rows);
                }
            }
        }

        Ok(executor::ResultSet::new(columns, all_results))
    }
}
