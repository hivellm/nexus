//! MATCH/DELETE/CREATE execution helpers and expression evaluators
//! for the engine write path. Extracted from `engine/mod.rs`.

use super::Engine;
use crate::{Error, Result, executor, session, transaction};
use serde_json::Value;

impl Engine {
    /// Execute MATCH ... DELETE query
    /// Returns the number of nodes deleted
    pub(super) fn execute_match_delete_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<u64> {
        // First, execute the MATCH part to get the matching nodes
        let mut match_query_clauses = Vec::new();
        let mut delete_clause_opt = None;

        for clause in &ast.clauses {
            match clause {
                executor::parser::Clause::Match(_) | executor::parser::Clause::Where(_) => {
                    match_query_clauses.push(clause.clone());
                }
                executor::parser::Clause::Delete(delete_clause) => {
                    delete_clause_opt = Some(delete_clause.clone());
                    break; // Stop at DELETE
                }
                _ => {
                    match_query_clauses.push(clause.clone());
                }
            }
        }

        // Execute MATCH to get results
        let match_query = executor::parser::CypherQuery {
            clauses: match_query_clauses,
            params: ast.params.clone(),
            graph_scope: ast.graph_scope.clone(),
        };

        // Collect all node variables from MATCH and CREATE clauses.
        // phase6 §8 — also consider CREATE-bound variables so patterns
        // like `CREATE (n:BenchCycle) WITH n DELETE n` resolve (the
        // outer caller now admits queries with CREATE-or-MATCH + DELETE).
        let mut node_variables = Vec::new();
        for clause in &match_query.clauses {
            let pattern_opt = match clause {
                executor::parser::Clause::Match(mc) => Some(&mc.pattern),
                executor::parser::Clause::Create(cc) => Some(&cc.pattern),
                _ => None,
            };
            if let Some(pattern) = pattern_opt {
                for element in &pattern.elements {
                    if let executor::parser::PatternElement::Node(node) = element {
                        if let Some(var) = &node.variable {
                            if !node_variables.contains(var) {
                                node_variables.push(var.clone());
                            }
                        }
                    }
                }
            }
        }

        // Build a synthetic RETURN clause that projects every
        // matched node variable, then attach it to the MATCH-only
        // AST and hand the whole thing to the executor as a
        // preparsed override. Going through an AST override avoids
        // ever re-serialising the scoped label strings (e.g.
        // `ns:alice:Person`) into Cypher and re-parsing them — that
        // round-trip would split on `:` into three separate labels
        // and break cluster-mode isolation on `MATCH … DELETE`.
        //
        // Pre-cluster-mode deployments (`mode = None` in
        // `execute_cypher_with_context`) end up here too, with
        // unscoped labels, and the override path handles them
        // identically — one code path, two modes.
        let return_items: Vec<executor::parser::ReturnItem> = node_variables
            .iter()
            .map(|var| executor::parser::ReturnItem {
                expression: executor::parser::Expression::Variable(var.clone()),
                alias: Some(var.clone()),
            })
            .collect();
        let mut match_query_with_return = match_query.clone();
        match_query_with_return
            .clauses
            .push(executor::parser::Clause::Return(
                executor::parser::ReturnClause {
                    items: return_items,
                    distinct: false,
                },
            ));

        // RAII guard clears the override on every return path so a
        // leftover override cannot leak into an unrelated caller.
        struct OverrideGuard {
            executor: executor::Executor,
        }
        impl Drop for OverrideGuard {
            fn drop(&mut self) {
                self.executor.install_preparsed_ast_override(None);
            }
        }
        self.executor
            .install_preparsed_ast_override(Some(match_query_with_return));
        let _override_guard = OverrideGuard {
            executor: self.executor.clone(),
        };

        let query_obj = executor::Query {
            cypher: String::new(),
            params: ast.params.clone(),
        };

        let match_results = self.executor.execute(&query_obj)?;

        // Count deleted nodes
        let mut deleted_count = 0u64;

        // For each row in MATCH result, delete the nodes
        if let Some(delete_clause) = delete_clause_opt {
            let detach = delete_clause.detach;

            for row in &match_results.rows {
                // Extract node IDs from the row
                for (idx, column) in match_results.columns.iter().enumerate() {
                    // Check if this variable is in the DELETE clause items
                    if delete_clause.items.contains(column) && idx < row.values.len() {
                        if let serde_json::Value::Object(obj) = &row.values[idx] {
                            if let Some(serde_json::Value::Number(id)) = obj.get("_nexus_id") {
                                if let Some(node_id) = id.as_u64() {
                                    if detach {
                                        // Delete all relationships connected to this node first
                                        self.delete_node_relationships(node_id)?;
                                        self.delete_node(node_id)?;
                                    } else {
                                        let node_record = self.storage.read_node(node_id)?;
                                        if node_record.first_rel_ptr != 0 {
                                            return Err(Error::CypherExecution(
                                                "Cannot DELETE node with existing relationships; use DETACH DELETE"
                                                    .to_string(),
                                            ));
                                        }
                                        self.delete_node(node_id)?;
                                    }
                                    deleted_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(deleted_count)
    }

    /// Execute MATCH ... CREATE query
    pub(super) fn execute_match_create_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
        query_str_opt: Option<&str>,
    ) -> Result<executor::ResultSet> {
        // FIXED: Don't split the query - let the executor handle MATCH...CREATE as a single operation
        // The executor's CREATE operator (execute_create_with_context) will correctly handle
        // creating relationships using the MATCH results

        let cypher = if let Some(qs) = query_str_opt {
            qs.to_string()
        } else {
            self.query_to_string(ast)
        };

        let query_obj = executor::Query {
            cypher,
            params: ast.params.clone(),
        };

        // Execute and return result
        self.executor.execute(&query_obj)
    }

    /// Create from pattern with existing node context
    pub(super) fn create_from_pattern_with_context(
        &mut self,
        pattern: &executor::parser::Pattern,
        node_vars: &std::collections::HashMap<String, u64>,
    ) -> Result<()> {
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.create_from_pattern_with_context_and_transaction(pattern, node_vars, &mut tx_ref, None)
    }

    /// Create from pattern with existing node context and optional transaction
    pub(super) fn create_from_pattern_with_context_and_transaction(
        &mut self,
        pattern: &executor::parser::Pattern,
        node_vars: &std::collections::HashMap<String, u64>,
        session_tx: &mut Option<&mut transaction::Transaction>,
        mut created_nodes_tracker: Option<&mut Vec<u64>>,
    ) -> Result<()> {
        let mut current_node_id: Option<u64> = None;

        // Use indexed iteration to access next element for relationships
        for (i, element) in pattern.elements.iter().enumerate() {
            match element {
                executor::parser::PatternElement::Node(node) => {
                    if let Some(var) = &node.variable {
                        // Check if this variable exists in the MATCH context
                        if let Some(&existing_id) = node_vars.get(var) {
                            current_node_id = Some(existing_id);
                        } else {
                            // Create new node
                            let properties = if let Some(props_map) = &node.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            let node_id = if let Some(ref mut tracker) = created_nodes_tracker {
                                self.create_node_with_transaction(
                                    node.labels.clone(),
                                    properties,
                                    session_tx,
                                    Some(tracker),
                                )?
                            } else {
                                self.create_node_with_transaction(
                                    node.labels.clone(),
                                    properties,
                                    session_tx,
                                    None,
                                )?
                            };
                            current_node_id = Some(node_id);
                        }
                    }
                }
                executor::parser::PatternElement::Relationship(rel) => {
                    // Get source node (set by previous node element)
                    let source_id = current_node_id.ok_or_else(|| {
                        Error::CypherExecution("Relationship must follow a node".to_string())
                    })?;

                    // Get target node (next element after relationship)
                    if i + 1 < pattern.elements.len() {
                        if let executor::parser::PatternElement::Node(target_node) =
                            &pattern.elements[i + 1]
                        {
                            // Target node MUST have a variable and MUST exist in MATCH context
                            let target_id = if let Some(var) = &target_node.variable {
                                // Check if target exists in MATCH context
                                if let Some(&existing_id) = node_vars.get(var) {
                                    current_node_id = Some(existing_id);
                                    existing_id
                                } else {
                                    // This shouldn't happen for MATCH ... CREATE
                                    // All nodes should be matched first
                                    return Err(Error::CypherExecution(format!(
                                        "Node variable '{}' not found in MATCH context",
                                        var
                                    )));
                                }
                            } else {
                                return Err(Error::CypherExecution(
                                    "Target node must have a variable".to_string(),
                                ));
                            };

                            // Create relationship
                            let rel_type = rel.types.first().ok_or_else(|| {
                                Error::CypherExecution("Relationship must have a type".to_string())
                            })?;

                            let rel_properties = if let Some(props_map) = &rel.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            self.create_relationship_with_transaction(
                                source_id,
                                target_id,
                                rel_type.clone(),
                                rel_properties,
                                session_tx,
                            )?;
                        } else {
                            return Err(Error::CypherExecution(
                                "Relationship must be followed by a node".to_string(),
                            ));
                        }
                    } else {
                        return Err(Error::CypherExecution(
                            "Pattern must end with a node".to_string(),
                        ));
                    }
                }
                executor::parser::PatternElement::QuantifiedGroup(_) => {
                    return Err(Error::CypherExecution(
                        "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                         are read-only; use a MATCH clause instead"
                            .to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    /// Execute CREATE query via Engine to ensure proper persistence
    pub(super) fn execute_create_query(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<()> {
        // Get session and check if it has an active transaction
        let session_id = "default";

        // Get session once and check if it has an active transaction
        let mut session = self.session_manager.get_session(&session_id.to_string());

        if let Some(ref mut sess) = session {
            if sess.has_active_transaction() {
                // Extract transaction from session
                if let Some(mut tx) = sess.active_transaction.take() {
                    // Execute CREATE operations with this transaction
                    let mut tx_ref: Option<&mut transaction::Transaction> = Some(&mut tx);
                    let result =
                        self.execute_create_query_with_transaction(ast, &mut tx_ref, Some(sess));

                    // Put transaction back in session and update session with tracked nodes
                    sess.active_transaction = Some(tx);
                    self.session_manager.update_session(sess.clone());

                    return result;
                }
            }
        }

        // No active transaction, execute normally (will create own transactions)
        let mut tx_ref: Option<&mut transaction::Transaction> = None;
        self.execute_create_query_with_transaction(ast, &mut tx_ref, None)
    }

    /// Execute CREATE query with optional transaction
    pub(super) fn execute_create_query_with_transaction(
        &mut self,
        ast: &executor::parser::CypherQuery,
        session_tx: &mut Option<&mut transaction::Transaction>,
        mut session: Option<&mut session::Session>,
    ) -> Result<()> {
        use std::collections::HashMap;

        // Map of variable names to created node IDs
        let mut created_nodes: HashMap<String, u64> = HashMap::new();

        for clause in &ast.clauses {
            if let executor::parser::Clause::Create(create_clause) = clause {
                let mut last_node_id: Option<u64> = None;

                // Process pattern elements
                for (i, element) in create_clause.pattern.elements.iter().enumerate() {
                    match element {
                        executor::parser::PatternElement::Node(node) => {
                            // Extract properties
                            let properties = if let Some(props_map) = &node.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create node using Engine API with session transaction if available
                            let node_id = self.create_node_with_transaction(
                                node.labels.clone(),
                                properties,
                                session_tx,
                                session.as_mut().map(|s| &mut s.created_nodes),
                            )?;

                            // Store node ID if variable exists
                            if let Some(var) = &node.variable {
                                created_nodes.insert(var.clone(), node_id);
                            }

                            last_node_id = Some(node_id);
                        }
                        executor::parser::PatternElement::Relationship(rel) => {
                            // Get source node
                            let source_id = last_node_id.ok_or_else(|| {
                                Error::CypherExecution(
                                    "Relationship must follow a node".to_string(),
                                )
                            })?;

                            // Get target node (next element)
                            let target_id = if i + 1 < create_clause.pattern.elements.len() {
                                if let executor::parser::PatternElement::Node(target_node) =
                                    &create_clause.pattern.elements[i + 1]
                                {
                                    // Extract target properties
                                    let target_properties =
                                        if let Some(props_map) = &target_node.properties {
                                            let mut json_props = serde_json::Map::new();
                                            for (key, value_expr) in &props_map.properties {
                                                let json_value =
                                                    self.expression_to_json_value(value_expr)?;
                                                json_props.insert(key.clone(), json_value);
                                            }
                                            serde_json::Value::Object(json_props)
                                        } else {
                                            serde_json::Value::Null
                                        };

                                    // Create target node with session transaction if available
                                    let tid = self.create_node_with_transaction(
                                        target_node.labels.clone(),
                                        target_properties,
                                        session_tx,
                                        session.as_mut().map(|s| &mut s.created_nodes),
                                    )?;

                                    // Store target node ID
                                    if let Some(var) = &target_node.variable {
                                        created_nodes.insert(var.clone(), tid);
                                    }

                                    last_node_id = Some(tid);
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

                            // Extract relationship properties
                            let rel_properties = if let Some(props_map) = &rel.properties {
                                let mut json_props = serde_json::Map::new();
                                for (key, value_expr) in &props_map.properties {
                                    let json_value = self.expression_to_json_value(value_expr)?;
                                    json_props.insert(key.clone(), json_value);
                                }
                                serde_json::Value::Object(json_props)
                            } else {
                                serde_json::Value::Null
                            };

                            // Create relationship using Engine API with session transaction if available
                            self.create_relationship_with_transaction(
                                source_id,
                                target_id,
                                rel_type.to_string(),
                                rel_properties,
                                session_tx,
                            )?;
                        }
                        executor::parser::PatternElement::QuantifiedGroup(_) => {
                            return Err(Error::CypherExecution(
                                "ERR_QPP_NOT_IN_CREATE: quantified path patterns \
                                 are read-only; use a MATCH clause instead"
                                    .to_string(),
                            ));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Convert expression to JSON value (helper for CREATE)
    pub(super) fn expression_to_json_value(
        &self,
        expr: &executor::parser::Expression,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::String(s) => Ok(serde_json::Value::String(s.clone())),
                executor::parser::Literal::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
                executor::parser::Literal::Float(f) => {
                    if let Some(num) = serde_json::Number::from_f64(*f) {
                        Ok(serde_json::Value::Number(num))
                    } else {
                        Err(Error::CypherExecution(format!("Invalid float: {}", f)))
                    }
                }
                executor::parser::Literal::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
                executor::parser::Literal::Null => Ok(serde_json::Value::Null),
                executor::parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            // UNWIND-write row bindings (issue #13): `row` / `row.id` resolve
            // against the per-iteration binding installed by the UNWIND-write
            // loop. Outside such a loop `unwind_bindings` is empty and these
            // fall through to the error below — preserving the legacy
            // "literals only" contract for ordinary CREATE/MERGE properties.
            executor::parser::Expression::Variable(name) => {
                self.unwind_bindings.get(name).cloned().ok_or_else(|| {
                    Error::CypherExecution(format!(
                        "Unbound variable `{name}` in write property value"
                    ))
                })
            }
            executor::parser::Expression::PropertyAccess { variable, property } => {
                match self.unwind_bindings.get(variable) {
                    Some(serde_json::Value::Object(m)) => {
                        Ok(m.get(property).cloned().unwrap_or(serde_json::Value::Null))
                    }
                    _ => Err(Error::CypherExecution(format!(
                        "Cannot resolve `{variable}.{property}` in write property value"
                    ))),
                }
            }
            _ => Err(Error::CypherExecution(
                "Complex expressions not supported in CREATE properties".to_string(),
            )),
        }
    }

    /// Evaluate expression for SET clause with node context
    pub(super) fn evaluate_set_expression(
        &self,
        expr: &executor::parser::Expression,
        target_var: &str,
        node_props: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::String(s) => Ok(serde_json::Value::String(s.clone())),
                executor::parser::Literal::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
                executor::parser::Literal::Float(f) => serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| Error::CypherExecution(format!("Invalid float: {}", f))),
                executor::parser::Literal::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
                executor::parser::Literal::Null => Ok(serde_json::Value::Null),
                executor::parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            executor::parser::Expression::PropertyAccess { variable, property } => {
                if variable == target_var {
                    Ok(node_props
                        .get(property)
                        .cloned()
                        .unwrap_or(serde_json::Value::Null))
                } else if let Some(serde_json::Value::Object(m)) =
                    self.unwind_bindings.get(variable)
                {
                    // UNWIND-write row binding (issue #13): `SET n.x = row.y`.
                    Ok(m.get(property).cloned().unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            // UNWIND-write row binding (issue #13): bare `row` on the SET RHS.
            executor::parser::Expression::Variable(name) => Ok(self
                .unwind_bindings
                .get(name)
                .cloned()
                .unwrap_or(serde_json::Value::Null)),
            executor::parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_set_expression(left, target_var, node_props)?;
                let right_val = self.evaluate_set_expression(right, target_var, node_props)?;
                match op {
                    executor::parser::BinaryOperator::Add => {
                        self.json_add_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Subtract => {
                        self.json_subtract_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Multiply => {
                        self.json_multiply_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Divide => {
                        self.json_divide_values(&left_val, &right_val)
                    }
                    executor::parser::BinaryOperator::Modulo => {
                        self.json_modulo_values(&left_val, &right_val)
                    }
                    _ => Err(Error::CypherExecution(format!(
                        "Unsupported binary operator in SET: {:?}",
                        op
                    ))),
                }
            }
            executor::parser::Expression::UnaryOp { op, operand } => {
                let val = self.evaluate_set_expression(operand, target_var, node_props)?;
                match op {
                    executor::parser::UnaryOperator::Minus => {
                        if let Some(n) = val.as_i64() {
                            Ok(serde_json::Value::Number((-n).into()))
                        } else if let Some(n) = val.as_f64() {
                            serde_json::Number::from_f64(-n)
                                .map(serde_json::Value::Number)
                                .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    executor::parser::UnaryOperator::Not => val
                        .as_bool()
                        .map(|b| serde_json::Value::Bool(!b))
                        .ok_or_else(|| Error::CypherExecution("Invalid bool".to_string())),
                    _ => Ok(serde_json::Value::Null),
                }
            }
            // phase6_opencypher-quickwins §6 — Map literal in SET RHS.
            // Needed for `SET n += {city: 'Berlin'}`; the merge operator
            // evaluates the whole map first, then consults it key-by-key.
            executor::parser::Expression::Map(entries) => {
                let mut out = serde_json::Map::with_capacity(entries.len());
                for (k, v) in entries.iter() {
                    let val = self.evaluate_set_expression(v, target_var, node_props)?;
                    out.insert(k.clone(), val);
                }
                Ok(serde_json::Value::Object(out))
            }
            // Parameter placeholders surface as NULL in this narrow
            // evaluator — parameter-binding lives on the executor side.
            // Treating them as NULL keeps `SET n += $missing` safely a
            // no-op when the parameter is absent.
            executor::parser::Expression::Parameter(_) => Ok(serde_json::Value::Null),
            _ => Err(Error::CypherExecution(
                "Unsupported expression type in SET clause".to_string(),
            )),
        }
    }

    pub(super) fn json_add_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li + ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf + rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            (serde_json::Value::String(l), serde_json::Value::String(r)) => {
                Ok(serde_json::Value::String(format!("{}{}", l, r)))
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    pub(super) fn json_subtract_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li - ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf - rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    pub(super) fn json_multiply_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li * ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf * rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    pub(super) fn json_divide_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf / rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    pub(super) fn json_modulo_values(
        &self,
        left: &serde_json::Value,
        right: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    if ri == 0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        Ok(serde_json::Value::Number((li % ri).into()))
                    }
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf % rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    /// Engine-side CREATE entry-point used when the executor path
    /// cannot be trusted with a dynamic-label pattern
    /// (phase6_opencypher-advanced-types §2). Walks the CREATE
    /// pattern, resolves each node's `:$param` sentinels via
    /// `resolve_dynamic_labels`, and funnels through the engine's
    /// own `create_node` — which re-runs the resolver (fast-path
    /// on static-only inputs) and performs the catalog write.
    ///
    /// Relationships in the pattern are ignored at this entry point
    /// for now; today the dynamic-label feature is scoped to node
    /// labels only, and the CREATE patterns the tests exercise are
    /// node-only.
    pub(super) fn execute_create_via_engine(
        &mut self,
        ast: &executor::parser::CypherQuery,
    ) -> Result<()> {
        for clause in &ast.clauses {
            if let executor::parser::Clause::Create(cc) = clause {
                for element in &cc.pattern.elements {
                    if let executor::parser::PatternElement::Node(node) = element {
                        let resolved = self.resolve_dynamic_labels(&node.labels)?;
                        let mut props = serde_json::Map::new();
                        if let Some(pm) = &node.properties {
                            for (k, expr) in &pm.properties {
                                let v = self.expression_to_json_value(expr)?;
                                props.insert(k.clone(), v);
                            }
                        }
                        self.create_node(resolved, serde_json::Value::Object(props))?;
                    }
                }
            }
        }
        self.refresh_executor()?;
        Ok(())
    }

    /// Convert an expression to its string representation (for query building)
    pub(super) fn expression_to_string(&self, expr: &executor::parser::Expression) -> String {
        match expr {
            executor::parser::Expression::Variable(v) => v.clone(),
            executor::parser::Expression::PropertyAccess { variable, property } => {
                format!("{}.{}", variable, property)
            }
            executor::parser::Expression::FunctionCall { name, args } => {
                let args_str = args
                    .iter()
                    .map(|arg| self.expression_to_string(arg))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, args_str)
            }
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::Integer(n) => n.to_string(),
                executor::parser::Literal::Float(f) => f.to_string(),
                executor::parser::Literal::String(s) => format!("'{}'", s),
                executor::parser::Literal::Boolean(b) => b.to_string(),
                executor::parser::Literal::Null => "null".to_string(),
                _ => "?".to_string(),
            },
            // For other complex expressions, just return a placeholder
            // The full executor will handle them properly
            _ => "?".to_string(),
        }
    }
}
