//! Node/relationship creation operators and expression coercion helpers.
//!
//! - `execute_create_pattern_with_variables` / `execute_create_pattern_internal`:
//!   realise a CREATE pattern into actual nodes/relationships, threading
//!   previously-bound variables into the pattern.
//! - `execute_create_with_context`: drives CREATE with upstream MATCH context.
//! - `expression_to_json_value` / `expression_to_string`: coerce parser
//!   expressions into property-value form for persistence.
//! - `check_constraints`: runs NOT NULL and uniqueness guards before insert.

use super::super::context::{ExecutionContext, RelationshipInfo};
use super::super::engine::Executor;
use super::super::parser;
use super::super::types::Row;
use crate::{Error, Result};
use serde_json::{Map, Value};
use std::collections::HashMap;

impl Executor {
    pub(in crate::executor) fn execute_create_pattern_with_variables(
        &self,
        pattern: &parser::Pattern,
    ) -> Result<(
        std::collections::HashMap<String, u64>,
        std::collections::HashMap<String, RelationshipInfo>,
    )> {
        let mut created_nodes: std::collections::HashMap<String, u64> =
            std::collections::HashMap::new();
        let mut created_relationships: std::collections::HashMap<String, RelationshipInfo> =
            std::collections::HashMap::new();

        // Call the original implementation
        self.execute_create_pattern_internal(
            pattern,
            &mut created_nodes,
            &mut created_relationships,
        )?;

        Ok((created_nodes, created_relationships))
    }

    /// Internal implementation of CREATE pattern execution
    pub(in crate::executor) fn execute_create_pattern_internal(
        &self,
        pattern: &parser::Pattern,
        created_nodes: &mut std::collections::HashMap<String, u64>,
        created_relationships: &mut std::collections::HashMap<String, RelationshipInfo>,
    ) -> Result<()> {
        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;

        // Phase 1 Optimization: Cache label lookups and batch catalog updates
        let mut label_cache: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        let mut label_count_updates: std::collections::HashMap<u32, u32> =
            std::collections::HashMap::new();

        // Phase 1.5.2: Pre-allocate label/type IDs in batches
        // Collect all unique labels and types from the pattern first
        let mut all_labels = std::collections::HashSet::new();
        let mut all_types = std::collections::HashSet::new();

        for element in &pattern.elements {
            match element {
                parser::PatternElement::Node(node) => {
                    for label in &node.labels {
                        all_labels.insert(label.as_str());
                    }
                }
                parser::PatternElement::Relationship(rel) => {
                    for rel_type in &rel.types {
                        all_types.insert(rel_type.as_str());
                    }
                }
            }
        }

        // Batch allocate all labels in a single transaction
        if !all_labels.is_empty() {
            let labels_vec: Vec<&str> = all_labels.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_labels(&labels_vec)?;
            label_cache.extend(batch_results);
        }

        // Batch allocate all types in a single transaction
        if !all_types.is_empty() {
            let types_vec: Vec<&str> = all_types.iter().copied().collect();
            let batch_results = self.catalog().batch_get_or_create_types(&types_vec)?;
            label_cache.extend(batch_results); // Reuse label_cache for types too
        }

        // Use the passed-in created_nodes HashMap (don't create a new one)
        let mut last_node_id: Option<u64> = None;
        let mut skip_next_node = false; // Flag to skip node already created in relationship

        // Process pattern elements in sequence
        // Pattern alternates: Node -> Relationship -> Node -> Relationship ...
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                parser::PatternElement::Node(node) => {
                    // Skip if this node was already created as part of the previous relationship
                    if skip_next_node {
                        skip_next_node = false;
                        continue;
                    }

                    // Phase 1.5.2: Build label bitmap with pre-allocated IDs
                    // All labels should already be in label_cache from batch allocation
                    let mut label_bits = 0u64;
                    let mut label_ids_for_update = Vec::new();
                    for label in &node.labels {
                        // Labels should already be in cache from batch allocation
                        // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                        let label_id = if let Some(&id) = label_cache.get(label) {
                            id
                        } else {
                            // Fallback: individual lookup (shouldn't happen with batch allocation)
                            let id = self.catalog().get_or_create_label(label)?;
                            label_cache.insert(label.clone(), id);
                            id
                        };

                        if label_id < 64 {
                            label_bits |= 1u64 << label_id;
                        }
                        label_ids_for_update.push(label_id);
                    }

                    // Phase 1 Optimization: Pre-size properties Map to avoid reallocations
                    let properties = if let Some(props_map) = &node.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        tracing::trace!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, properties={:?}",
                            node.variable,
                            node.labels,
                            serde_json::Value::Object(json_props.clone())
                        );
                        serde_json::Value::Object(json_props)
                    } else {
                        tracing::trace!(
                            "execute_create_pattern_internal: creating node with variable {:?}, labels {:?}, NO PROPERTIES",
                            node.variable,
                            node.labels
                        );
                        serde_json::Value::Null
                    };

                    // Check constraints before creating node
                    self.check_constraints(&label_ids_for_update, &properties)?;

                    // Create the node
                    let node_id = self
                        .store_mut()
                        .create_node_with_label_bits(&mut tx, label_bits, properties)?;

                    tracing::trace!(
                        "execute_create_pattern_internal: created node_id={}, variable={:?}",
                        node_id,
                        node.variable
                    );

                    // Phase 1 Optimization: Batch catalog metadata updates (defer to end)
                    for label_id in label_ids_for_update {
                        *label_count_updates.entry(label_id).or_insert(0) += 1;
                    }

                    // Store node ID if variable exists
                    if let Some(var) = &node.variable {
                        created_nodes.insert(var.clone(), node_id);
                    }

                    // Track last node for relationship creation
                    last_node_id = Some(node_id);
                }
                parser::PatternElement::Relationship(rel) => {
                    // Get source node (previous element should be a node)
                    let source_id = if i > 0 {
                        last_node_id.ok_or_else(|| {
                            Error::CypherExecution("Relationship must follow a node".to_string())
                        })?
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must start with a node".to_string(),
                        ));
                    };

                    // Get target node (next element should be a node)
                    let target_id = if i + 1 < pattern.elements.len() {
                        if let parser::PatternElement::Node(target_node) = &pattern.elements[i + 1]
                        {
                            // Phase 1 Optimization: Build label bitmap with cached lookups
                            let mut target_label_bits = 0u64;
                            let mut target_label_ids_for_update = Vec::new();
                            for label in &target_node.labels {
                                // Use cache if available, otherwise lookup and cache
                                let label_id = if let Some(&cached_id) = label_cache.get(label) {
                                    cached_id
                                } else {
                                    let id = self.catalog().get_or_create_label(label)?;
                                    label_cache.insert(label.clone(), id);
                                    id
                                };

                                if label_id < 64 {
                                    target_label_bits |= 1u64 << label_id;
                                }
                                target_label_ids_for_update.push(label_id);
                            }

                            // Phase 1 Optimization: Pre-size properties Map
                            let target_properties = if let Some(props_map) = &target_node.properties
                            {
                                let prop_count = props_map.properties.len();
                                let mut json_props = serde_json::Map::with_capacity(prop_count);
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create target node (we'll skip it in the next iteration)
                            let tid = self.store_mut().create_node_with_label_bits(
                                &mut tx,
                                target_label_bits,
                                target_properties,
                            )?;

                            // Phase 1 Optimization: Batch catalog metadata updates for target node
                            for label_id in target_label_ids_for_update {
                                *label_count_updates.entry(label_id).or_insert(0) += 1;
                            }

                            // Store target node ID if variable exists
                            if let Some(var) = &target_node.variable {
                                created_nodes.insert(var.clone(), tid);
                            }

                            last_node_id = Some(tid);

                            // Set flag to skip this node in the next iteration
                            skip_next_node = true;

                            tid
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    };

                    // Get relationship type
                    let rel_type = rel.types.first().ok_or_else(|| {
                        Error::CypherExecution("Relationship must have a type".to_string())
                    })?;

                    // Phase 1.5.2: Use pre-allocated type ID
                    // Type should already be in cache from batch allocation
                    // Fallback to individual lookup if not found (shouldn't happen, but be safe)
                    let type_id = if let Some(&id) = label_cache.get(rel_type) {
                        id
                    } else {
                        // Fallback: individual lookup (shouldn't happen with batch allocation)
                        let id = self.catalog().get_or_create_type(rel_type)?;
                        label_cache.insert(rel_type.to_string(), id);
                        id
                    };

                    // Phase 1 Optimization: Pre-size properties Map for relationships
                    let rel_properties = if let Some(props_map) = &rel.properties {
                        let prop_count = props_map.properties.len();
                        let mut json_props = serde_json::Map::with_capacity(prop_count);
                        for (key, value_expr) in &props_map.properties {
                            let json_value = self.expression_to_json_value(value_expr)?;
                            json_props.insert(key.clone(), json_value);
                        }
                        serde_json::Value::Object(json_props)
                    } else {
                        serde_json::Value::Null
                    };

                    // Clone properties for Phase 8 synchronization (before moving to create_relationship)
                    let rel_props_clone = rel_properties.clone();

                    // Acquire row locks on source and target nodes before creating relationship
                    let (_source_lock, _target_lock) =
                        self.acquire_relationship_locks(source_id, target_id)?;

                    // Create the relationship (locks held by guards)
                    let rel_id = self.store_mut().create_relationship(
                        &mut tx,
                        source_id,
                        target_id,
                        type_id,
                        rel_properties,
                    )?;

                    // Locks are released when guards are dropped

                    // Store relationship ID if variable exists
                    if let Some(var) = &rel.variable {
                        created_relationships.insert(
                            var.clone(),
                            RelationshipInfo {
                                id: rel_id,
                                source_id,
                                target_id,
                                type_id,
                            },
                        );
                    }

                    // Phase 8: Update RelationshipStorageManager and RelationshipPropertyIndex
                    if self.enable_relationship_optimizations {
                        if let Some(ref rel_storage) = self.shared.relationship_storage {
                            // Convert properties from JSON Value to HashMap<String, Value>
                            let mut props_map = std::collections::HashMap::new();
                            if let serde_json::Value::Object(obj) = &rel_props_clone {
                                for (key, value) in obj {
                                    props_map.insert(key.clone(), value.clone());
                                }
                            }

                            // Add relationship to specialized storage
                            if let Err(e) = rel_storage.write().create_relationship(
                                source_id,
                                target_id,
                                type_id,
                                props_map.clone(),
                            ) {
                                tracing::warn!(
                                    "Failed to update RelationshipStorageManager: {}",
                                    e
                                );
                                // Don't fail the operation, just log the warning
                            }

                            // Update property index if there are properties
                            if !props_map.is_empty() {
                                if let Some(ref prop_index) =
                                    self.shared.relationship_property_index
                                {
                                    if let Err(e) = prop_index
                                        .write()
                                        .index_properties(rel_id, type_id, &props_map)
                                    {
                                        tracing::warn!(
                                            "Failed to update RelationshipPropertyIndex: {}",
                                            e
                                        );
                                        // Don't fail the operation, just log the warning
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;

        // Phase 1 Optimization: Batch apply catalog metadata updates (reduces I/O)
        // Convert HashMap to Vec for batch update
        let updates: Vec<(u32, u32)> = label_count_updates.into_iter().collect();
        if !updates.is_empty() {
            if let Err(e) = self.catalog().batch_increment_node_counts(&updates) {
                // Log error but don't fail the operation
                tracing::warn!("Failed to batch update node counts: {}", e);
            }
        }

        // PERFORMANCE OPTIMIZATION: Use async flush for better throughput
        // The transaction commit above ensures data integrity
        // Async flush triggers write without blocking on OS confirmation
        // Memory barrier below ensures visibility across threads
        self.store_mut().flush_async()?;

        // Update label index with created nodes
        // Scan all nodes from the store that were created (iterate based on node IDs, not variables)
        let start_node_id = if created_nodes.is_empty() {
            // If no variables were tracked, we need to find the new nodes
            // For now, just iterate over ALL nodes in the recent range
            // This is a workaround - ideally we'd track all created IDs, not just those with variables
            // For standalone CREATE without variables, we need a different approach
            // Let's assume created nodes are at the end of the node_count range
            let node_count = self.store().node_count();
            // Get the expected number of nodes created (pattern elements count)
            let expected_created = pattern
                .elements
                .iter()
                .filter(|e| matches!(e, parser::PatternElement::Node(_)))
                .count();
            if node_count as usize >= expected_created {
                node_count - expected_created as u64
            } else {
                0
            }
        } else {
            // Use the tracked nodes
            *created_nodes.values().min().unwrap_or(&0)
        };

        let end_node_id = self.store().node_count();

        for node_id in start_node_id..end_node_id {
            // Read the node to get its labels
            if let Ok(node_record) = self.store().read_node(node_id) {
                if node_record.is_deleted() {
                    continue;
                }
                let mut label_ids = Vec::new();
                for bit in 0..64 {
                    if (node_record.label_bits & (1u64 << bit)) != 0 {
                        label_ids.push(bit as u32);
                    }
                }
                if !label_ids.is_empty() {
                    self.label_index_mut().add_node(node_id, &label_ids)?;
                }
            }
        }

        Ok(())
    }

    /// Convert expression to JSON value
    pub(in crate::executor) fn expression_to_json_value(
        &self,
        expr: &parser::Expression,
    ) -> Result<Value> {
        match expr {
            parser::Expression::Literal(lit) => match lit {
                parser::Literal::String(s) => Ok(Value::String(s.clone())),
                parser::Literal::Integer(i) => Ok(Value::Number((*i).into())),
                parser::Literal::Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        Ok(Value::Number(num))
                    } else {
                        Err(Error::CypherExecution(format!("Invalid float: {}", f)))
                    }
                }
                parser::Literal::Boolean(b) => Ok(Value::Bool(*b)),
                parser::Literal::Null => Ok(Value::Null),
                parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            parser::Expression::Variable(_) => Err(Error::CypherExecution(
                "Variables not supported in CREATE properties".to_string(),
            )),
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Check constraints before creating a node
    pub(in crate::executor) fn check_constraints(
        &self,
        label_ids: &[u32],
        properties: &serde_json::Value,
    ) -> Result<()> {
        let constraint_manager = self.catalog().constraint_manager().read();

        // Check constraints for each label
        for &label_id in label_ids {
            let constraints = constraint_manager.get_constraints_for_label(label_id)?;

            for constraint in constraints {
                // Get property name
                let property_name = self
                    .catalog()
                    .get_key_name(constraint.property_key_id)?
                    .ok_or_else(|| Error::Internal("Property key not found".to_string()))?;

                let property_value = properties.as_object().and_then(|m| m.get(&property_name));

                match constraint.constraint_type {
                    crate::catalog::constraints::ConstraintType::Exists => {
                        // Property must exist (not null)
                        if property_value.is_none()
                            || property_value == Some(&serde_json::Value::Null)
                        {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));
                            return Err(Error::ConstraintViolation(format!(
                                "EXISTS constraint violated: property '{}' must exist on nodes with label '{}'",
                                property_name, label_name
                            )));
                        }
                    }
                    crate::catalog::constraints::ConstraintType::Unique => {
                        // Property value must be unique across all nodes with this label
                        if let Some(value) = property_value {
                            let label_name = self
                                .catalog()
                                .get_label_name(label_id)?
                                .unwrap_or_else(|| format!("ID{}", label_id));

                            // Get all nodes with this label
                            let bitmap = self.label_index().get_nodes_with_labels(&[label_id])?;

                            for node_id in bitmap.iter() {
                                let node_id_u64 = node_id as u64;

                                let node_props = self.store().load_node_properties(node_id_u64)?;
                                if let Some(serde_json::Value::Object(props_map)) = node_props {
                                    if let Some(existing_value) = props_map.get(&property_name) {
                                        if existing_value == value {
                                            return Err(Error::ConstraintViolation(format!(
                                                "UNIQUE constraint violated: property '{}' value already exists on another node with label '{}'",
                                                property_name, label_name
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to string representation
    pub(in crate::executor) fn expression_to_string(
        &self,
        expr: &parser::Expression,
    ) -> Result<String> {
        match expr {
            parser::Expression::Variable(name) => Ok(name.clone()),
            parser::Expression::PropertyAccess { variable, property } => {
                Ok(format!("{}.{}", variable, property))
            }
            parser::Expression::Literal(literal) => match literal {
                // Use single quotes for strings in filter predicates to match Cypher parser expectations
                parser::Literal::String(s) => Ok(format!("'{}'", s)),
                parser::Literal::Integer(i) => Ok(i.to_string()),
                parser::Literal::Float(f) => Ok(f.to_string()),
                parser::Literal::Boolean(b) => Ok(b.to_string()),
                parser::Literal::Null => Ok("NULL".to_string()),
                parser::Literal::Point(p) => Ok(p.to_string()),
            },
            parser::Expression::BinaryOp { left, op, right } => {
                let left_str = self.expression_to_string(left)?;
                let right_str = self.expression_to_string(right)?;
                let op_str = match op {
                    parser::BinaryOperator::Equal => "=",
                    parser::BinaryOperator::NotEqual => "!=",
                    parser::BinaryOperator::LessThan => "<",
                    parser::BinaryOperator::LessThanOrEqual => "<=",
                    parser::BinaryOperator::GreaterThan => ">",
                    parser::BinaryOperator::GreaterThanOrEqual => ">=",
                    parser::BinaryOperator::And => "AND",
                    parser::BinaryOperator::Or => "OR",
                    parser::BinaryOperator::Add => "+",
                    parser::BinaryOperator::Subtract => "-",
                    parser::BinaryOperator::Multiply => "*",
                    parser::BinaryOperator::Divide => "/",
                    parser::BinaryOperator::In => "IN",
                    _ => "?",
                };
                Ok(format!("{} {} {}", left_str, op_str, right_str))
            }
            parser::Expression::Parameter(name) => Ok(format!("${}", name)),
            _ => Ok("?".to_string()),
        }
    }
    #[tracing::instrument(skip_all, level = "debug")]
    pub(in crate::executor) fn execute_create_with_context(
        &self,
        context: &mut ExecutionContext,
        pattern: &parser::Pattern,
    ) -> Result<()> {
        // Note: TransactionManager is now accessed via self.transaction_manager() (shared)
        use serde_json::Value as JsonValue;

        // CRITICAL FIX: Always try to use context.variables first for MATCH...CREATE
        // The variables contain the full node objects with _nexus_id, while result_set.rows
        // may contain only projected values (strings) without _nexus_id.
        // Only fall back to result_set.rows if variables are empty.

        tracing::trace!(
            "execute_create_with_context: variables={:?}, result_set.rows={}",
            context.variables.keys().collect::<Vec<_>>(),
            context.result_set.rows.len()
        );

        let current_rows = if !context.variables.is_empty() {
            // PERFORMANCE OPTIMIZATION: Fast-path for simple single-value variables
            // This avoids the expensive materialize_rows_from_variables() for common cases
            // like MATCH (p:Person {name: 'X'}), (c:Company {name: 'Y'}) CREATE ...
            let all_single_values = context
                .variables
                .values()
                .all(|v| !matches!(v, JsonValue::Array(_)));

            if all_single_values {
                // Fast path: directly create a single row from variables
                let mut row = std::collections::HashMap::with_capacity(context.variables.len());
                let mut has_node_ids = false;
                for (var, value) in &context.variables {
                    if let JsonValue::Object(obj) = value {
                        if obj.contains_key("_nexus_id") {
                            has_node_ids = true;
                        }
                    }
                    row.insert(var.clone(), value.clone());
                }
                if has_node_ids {
                    vec![row]
                } else if !context.result_set.rows.is_empty() {
                    // Fallback to result_set if no node IDs
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    vec![row]
                }
            } else {
                // Slow path: use full materialization for array variables
                let materialized = self.materialize_rows_from_variables(context);

                // Verify materialized rows have node objects with _nexus_id
                let has_node_ids = materialized.iter().any(|row| {
                    row.values().any(|v| {
                        if let JsonValue::Object(obj) = v {
                            obj.contains_key("_nexus_id")
                        } else {
                            false
                        }
                    })
                });

                if has_node_ids {
                    materialized
                } else if !context.result_set.rows.is_empty() {
                    let columns = context.result_set.columns.clone();
                    context
                        .result_set
                        .rows
                        .iter()
                        .map(|row| self.row_to_map(row, &columns))
                        .collect()
                } else {
                    materialized
                }
            }
        } else if !context.result_set.rows.is_empty() {
            // No variables - use result_set.rows
            let columns = context.result_set.columns.clone();
            let rows: Vec<_> = context
                .result_set
                .rows
                .iter()
                .map(|row| self.row_to_map(row, &columns))
                .collect();
            tracing::trace!(
                "execute_create_with_context: no variables, using {} rows from result_set.rows",
                rows.len()
            );
            rows
        } else {
            // No variables and no rows
            tracing::trace!("execute_create_with_context: no variables and no rows");
            Vec::new()
        };

        // If no rows from MATCH, nothing to create
        if current_rows.is_empty() {
            return Ok(());
        }

        // DEBUG: Print row contents to see if they contain _nexus_id
        for (idx, row) in current_rows.iter().enumerate() {}

        // PERFORMANCE OPTIMIZATION: Reuse shared transaction manager instead of creating new
        // This saves ~1-2ms per operation by avoiding TransactionManager::new() overhead
        let mut tx_mgr = self.transaction_manager().lock();
        let mut tx = tx_mgr.begin_write()?;

        // Track (node_id, label_ids) for every node we actually create so the
        // label-bitmap index can be updated in a single pass after the
        // transaction commits (MATCH queries depend on this index; without
        // the update UNWIND + CREATE creates nodes the planner can't find).
        let mut created_nodes_with_labels: Vec<(u64, Vec<u32>)> = Vec::new();

        // For each row in the MATCH result, create the pattern
        // PERFORMANCE OPTIMIZATION: Pre-calculate expected capacity for node_ids
        let expected_vars = pattern
            .elements
            .iter()
            .filter(|e| matches!(e, parser::PatternElement::Node(n) if n.variable.is_some()))
            .count();

        for row in current_rows.iter() {
            // Pre-allocate HashMap with expected capacity
            let mut node_ids: std::collections::HashMap<String, u64> =
                std::collections::HashMap::with_capacity(expected_vars);

            // First, resolve existing node variables from the row
            for (var_name, var_value) in row {
                if let JsonValue::Object(obj) = var_value {
                    if let Some(JsonValue::Number(id)) = obj.get("_nexus_id") {
                        if let Some(node_id) = id.as_u64() {
                            node_ids.insert(var_name.clone(), node_id);
                        }
                    }
                }
            }

            // DEBUG: Print node_ids after extraction

            // CRITICAL FIX: If no node IDs were resolved from the row and the pattern requires
            // existing nodes from MATCH, skip this row (Filter removed all valid rows)
            // This prevents CREATE from executing when Filter filtered out all rows
            if node_ids.is_empty() {
                // Check if pattern requires existing nodes (has variables that should come from MATCH)
                let pattern_requires_existing_nodes = pattern.elements.iter().any(|elem| {
                    match elem {
                        parser::PatternElement::Node(node) => {
                            if let Some(var) = &node.variable {
                                // If node has no properties or labels, it's likely from MATCH
                                // If it has properties/labels, it's a new node to create
                                node.properties.is_none() && node.labels.is_empty()
                            } else {
                                false
                            }
                        }
                        parser::PatternElement::Relationship(_) => false,
                    }
                });

                if pattern_requires_existing_nodes {
                    continue; // Skip this row - Filter removed all valid matches
                }
            }

            // Now process the pattern elements to create new nodes and relationships
            let mut last_node_var: Option<String> = None;

            for (idx, element) in pattern.elements.iter().enumerate() {
                match element {
                    parser::PatternElement::Node(node) => {
                        if let Some(var) = &node.variable {
                            if !node_ids.contains_key(var) {
                                // Create new node (not from MATCH)
                                let label_ids: Vec<u32> = node
                                    .labels
                                    .iter()
                                    .filter_map(|l| self.catalog().get_or_create_label(l).ok())
                                    .collect();

                                let mut label_bits = 0u64;
                                for label_id in &label_ids {
                                    if *label_id < 64 {
                                        label_bits |= 1u64 << label_id;
                                    }
                                }

                                // Extract properties
                                let properties = if let Some(props_map) = &node.properties {
                                    JsonValue::Object(
                                        props_map
                                            .properties
                                            .iter()
                                            .filter_map(|(k, v)| {
                                                self.expression_to_json_value(v)
                                                    .ok()
                                                    .map(|val| (k.clone(), val))
                                            })
                                            .collect(),
                                    )
                                } else {
                                    JsonValue::Object(serde_json::Map::new())
                                };

                                // Create the node
                                let node_id = self
                                    .store_mut()
                                    .create_node_with_label_bits(&mut tx, label_bits, properties)?;
                                if !label_ids.is_empty() {
                                    created_nodes_with_labels.push((node_id, label_ids.clone()));
                                }
                                node_ids.insert(var.clone(), node_id);
                            }

                            // Track this node as the last one for relationship creation
                            last_node_var = Some(var.clone());
                        }
                    }
                    parser::PatternElement::Relationship(rel) => {
                        // Create relationship between last_node and next_node
                        if let Some(rel_type) = rel.types.first() {
                            let type_id = self.catalog().get_or_create_type(rel_type)?;

                            // Extract relationship properties
                            let properties = if let Some(props_map) = &rel.properties {
                                JsonValue::Object(
                                    props_map
                                        .properties
                                        .iter()
                                        .filter_map(|(k, v)| {
                                            self.expression_to_json_value(v)
                                                .ok()
                                                .map(|val| (k.clone(), val))
                                        })
                                        .collect(),
                                )
                            } else {
                                JsonValue::Object(serde_json::Map::new())
                            };

                            // Source is the last_node_var, target will be the next node in pattern
                            if let Some(source_var) = &last_node_var {
                                if let Some(source_id) = node_ids.get(source_var) {
                                    // Find target node (next element after this relationship)
                                    if idx + 1 < pattern.elements.len() {
                                        if let parser::PatternElement::Node(target_node) =
                                            &pattern.elements[idx + 1]
                                        {
                                            if let Some(target_var) = &target_node.variable {
                                                if let Some(target_id) = node_ids.get(target_var) {
                                                    // PERFORMANCE OPTIMIZATION: Skip row-level locking when lock-free mode is enabled
                                                    // The transaction manager mutex already provides serialization
                                                    // Row locks are only needed for concurrent writers
                                                    let _locks =
                                                        if !self.config.enable_lock_free_structures
                                                        {
                                                            Some(self.acquire_relationship_locks(
                                                                *source_id, *target_id,
                                                            )?)
                                                        } else {
                                                            None
                                                        };

                                                    // Create the relationship
                                                    let rel_id =
                                                        self.store_mut().create_relationship(
                                                            &mut tx, *source_id, *target_id,
                                                            type_id, properties,
                                                        )?;
                                                    tracing::trace!(
                                                        "execute_create_with_context: relationship created successfully, rel_id={}",
                                                        rel_id
                                                    );

                                                    // CRITICAL FIX: Populate relationship variable if specified
                                                    // This ensures that queries like CREATE (a)-[r:KNOWS]->(b) RETURN r work correctly
                                                    if let Some(rel_var) = &rel.variable {
                                                        if !rel_var.is_empty() {
                                                            let rel_info = RelationshipInfo {
                                                                id: rel_id,
                                                                source_id: *source_id,
                                                                target_id: *target_id,
                                                                type_id,
                                                            };
                                                            if let Ok(rel_value) = self
                                                                .read_relationship_as_value(
                                                                    &rel_info,
                                                                )
                                                            {
                                                                // Store relationship in context for RETURN clause
                                                                context.variables.insert(
                                                                    rel_var.clone(),
                                                                    rel_value,
                                                                );
                                                            }
                                                        }
                                                    }

                                                    // Locks are released when guards are dropped

                                                    // Relationship created successfully
                                                } else {
                                                    tracing::warn!(
                                                        "execute_create_with_context: Target node not found: var={}, available node_ids: {:?}",
                                                        target_var,
                                                        node_ids.keys().collect::<Vec<_>>()
                                                    );
                                                }
                                            } else {
                                                tracing::warn!(
                                                    "execute_create_with_context: Target node has no variable"
                                                );
                                            }
                                        } else {
                                            tracing::warn!(
                                                "execute_create_with_context: Next element is not a Node"
                                            );
                                        }
                                    } else {
                                        tracing::warn!(
                                            "execute_create_with_context: No next element after relationship"
                                        );
                                    }
                                } else {
                                    tracing::warn!(
                                        "execute_create_with_context: Source node not found: var={}, available node_ids: {:?}",
                                        source_var,
                                        node_ids.keys().collect::<Vec<_>>()
                                    );
                                }
                            } else {
                                tracing::warn!(
                                    "execute_create_with_context: No last_node_var (no source node before relationship)"
                                );
                            }
                        }
                    }
                }
            }
        }

        // Commit transaction
        tx_mgr.commit(&mut tx)?;
        drop(tx_mgr);

        // Register the created nodes in the label-bitmap index so subsequent
        // MATCH queries can find them. The engine's `create_node` path does
        // this automatically, but the Cypher CREATE path goes through the
        // storage layer directly and must maintain the index itself.
        if !created_nodes_with_labels.is_empty() {
            for (node_id, label_ids) in &created_nodes_with_labels {
                if let Err(e) = self.label_index_mut().add_node(*node_id, label_ids) {
                    tracing::warn!(
                        node_id = *node_id,
                        error = %e,
                        "execute_create_with_context: failed to update label index",
                    );
                }
            }
        }

        // PERFORMANCE OPTIMIZATION: Use async flush instead of sync flush
        // The sync flush was costing ~15-20ms per relationship creation
        // Async flush triggers the write but doesn't wait for OS confirmation
        // Data integrity is still maintained by the transaction commit above
        // For critical durability, callers can explicitly call flush() after the query
        self.store_mut().flush_async()?;

        // Memory barrier to ensure writes are visible to subsequent reads
        // Using Acquire/Release is sufficient here since we're in single-writer context
        std::sync::atomic::fence(std::sync::atomic::Ordering::Release);

        // CRITICAL FIX: Populate result_set with created entities for CREATE without RETURN
        // Instead of clearing everything, we populate result_set with the variables we have
        // This ensures that CREATE without RETURN returns the created entities
        // If RETURN clause follows, Project operator will overwrite this
        let mut columns: Vec<String> = context.variables.keys().cloned().collect();
        columns.sort(); // Ensure consistent column order

        if !columns.is_empty() {
            let mut row_values = Vec::new();
            for col in &columns {
                if let Some(value) = context.variables.get(col) {
                    // CRITICAL FIX: Unwrap arrays to get the actual node object
                    // Variables from MATCH are arrays, but we need single objects
                    let unwrapped = match value {
                        JsonValue::Array(arr) if arr.len() == 1 => arr[0].clone(),
                        _ => value.clone(),
                    };
                    row_values.push(unwrapped);
                } else {
                    row_values.push(JsonValue::Null);
                }
            }
            context.result_set.columns = columns;
            context.result_set.rows = vec![Row { values: row_values }];
        } else {
            // No variables created - clear result_set
            context.result_set.rows.clear();
            context.result_set.columns.clear();
        }

        tracing::trace!(
            "After CREATE: result_set.columns={:?}, result_set.rows.len()={}, variables.len()={}",
            context.result_set.columns,
            context.result_set.rows.len(),
            context.variables.len()
        );

        Ok(())
    }
}
