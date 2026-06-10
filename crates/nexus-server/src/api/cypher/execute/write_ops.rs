//! Write-path handler for `CREATE` and `MERGE` queries (including nested
//! `SET`, `DELETE`, `REMOVE`, and `FOREACH` clauses).  Called from
//! `execute_cypher` when `is_create_query || is_merge_query` is true.

use super::super::*;

/// Execute a `CREATE` or `MERGE` query through the engine write path.
///
/// Receives the already-parsed AST, original request, start time, and
/// pre-extracted actor-info tuple (used for audit logging). Returns the
/// serialised `CypherResponse` JSON ready to be returned to the caller.
pub(super) async fn execute_create_or_merge(
    server: Arc<NexusServer>,
    request: &CypherRequest,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
    actor_info: (Option<String>, Option<String>, Option<String>),
    is_merge_query: bool,
) -> Json<CypherResponse> {
    let get_actor_info =
        || -> (Option<String>, Option<String>, Option<String>) { actor_info.clone() };

    // Use Engine for CREATE operations
    {
        // Execute all clauses sequentially using Engine
        let mut engine = server.engine.write().await;

        // Create a context map to store variable bindings between clauses
        // For now, we'll use a simple map: variable_name -> node_id
        let mut variable_context: HashMap<String, Vec<u64>> = HashMap::new();

        for clause in &ast.clauses {
            if let nexus_core::executor::parser::Clause::Create(create_clause) = clause {
                let elements = &create_clause.pattern.elements;
                let mut index = 0;

                while index < elements.len() {
                    match &elements[index] {
                        nexus_core::executor::parser::PatternElement::QuantifiedGroup(_) => {
                            let execution_time = start_time.elapsed().as_millis() as u64;
                            let err = "ERR_QPP_NOT_IN_CREATE: quantified \
                                       path patterns are read-only; use a \
                                       MATCH clause instead"
                                .to_string();
                            tracing::error!("{}", err);
                            return Json(CypherResponse {
                                columns: vec![],
                                rows: vec![],
                                execution_time_ms: execution_time,
                                error: Some(err),
                                notifications: Vec::new(),
                            });
                        }
                        nexus_core::executor::parser::PatternElement::Node(node_pattern) => {
                            // phase9_external-node-ids §4.4 — resolve `_id`
                            // (literal string or parameter) and the
                            // ON CONFLICT clause from the parsed CreateClause
                            // and route through create_node_with_external_id
                            // when set; otherwise plain create_node.
                            let resolved_ext_id = match create_clause.external_id_expr.as_ref() {
                                Some(expr) => {
                                    match resolve_external_id_for_server(expr, &request.params) {
                                        Ok(ext) => Some(ext),
                                        Err(err) => {
                                            let execution_time =
                                                start_time.elapsed().as_millis() as u64;
                                            tracing::error!("{}", err);
                                            return Json(CypherResponse {
                                                columns: vec![],
                                                rows: vec![],
                                                execution_time_ms: execution_time,
                                                error: Some(err),
                                                notifications: Vec::new(),
                                            });
                                        }
                                    }
                                }
                                None => None,
                            };
                            let resolved_policy =
                                ast_conflict_policy_to_storage(create_clause.conflict_policy);
                            let mut current_nodes = match ensure_node_from_pattern_with_ext_id(
                                &mut engine,
                                node_pattern,
                                &mut variable_context,
                                resolved_ext_id,
                                resolved_policy,
                            ) {
                                Ok(nodes) => nodes,
                                Err(err) => {
                                    let execution_time = start_time.elapsed().as_millis() as u64;
                                    tracing::error!("{}", err);

                                    // Log failed write operation
                                    let (user_id, username, api_key_id) = get_actor_info();
                                    let _ = server
                                        .audit_logger
                                        .log_write_operation(
                                            nexus_core::auth::WriteOperationParams {
                                                actor_user_id: user_id,
                                                actor_username: username,
                                                api_key_id,
                                                operation_type: "CREATE".to_string(),
                                                entity_type: "NODE".to_string(),
                                                entity_id: None,
                                                cypher_query: Some(request.query.clone()),
                                                result: nexus_core::auth::AuditResult::Failure {
                                                    error: err.clone(),
                                                },
                                            },
                                        )
                                        .await;

                                    return Json(CypherResponse {
                                        columns: vec![],
                                        rows: vec![],
                                        execution_time_ms: execution_time,
                                        error: Some(err),
                                        notifications: Vec::new(),
                                    });
                                }
                            };

                            index += 1;

                            while index < elements.len() {
                                match &elements[index] {
                                    nexus_core::executor::parser::PatternElement::Relationship(rel_pattern) => {
                                        if index + 1 >= elements.len() {
                                            let execution_time =
                                                start_time.elapsed().as_millis() as u64;
                                            let err = "Relationship pattern missing target node".to_string();
                                            tracing::error!("{}", err);
                                            return Json(CypherResponse {
                                                columns: vec![],
                                                rows: vec![],
                                                execution_time_ms: execution_time,
                                                error: Some(err),
                                                notifications: Vec::new(),
                                            });
                                        }

                                        let target_node = match &elements[index + 1] {
                                            nexus_core::executor::parser::PatternElement::Node(node) => node,
                                            _ => {
                                                let execution_time = start_time
                                                    .elapsed()
                                                    .as_millis() as u64;
                                                let err = "Relationship pattern must be followed by a node".to_string();
                                                tracing::error!("{}", err);
                                                return Json(CypherResponse {
                                                    columns: vec![],
                                                    rows: vec![],
                                                    execution_time_ms: execution_time,
                                                    error: Some(err),
                                                    notifications: Vec::new(),
                                                });
                                            }
                                        };

                                        let target_nodes = match ensure_node_from_pattern(
                                            &mut engine,
                                            target_node,
                                            &mut variable_context,
                                        ) {
                                            Ok(nodes) => nodes,
                                            Err(err) => {
                                                let execution_time =
                                                    start_time.elapsed().as_millis() as u64;
                                                tracing::error!("{}", err);
                                                return Json(CypherResponse {
                                                    columns: vec![],
                                                    rows: vec![],
                                                    execution_time_ms: execution_time,
                                                    error: Some(err),
                                                    notifications: Vec::new(),
                                                });
                                            }
                                        };

                                        if let Err(err) = create_relationship_from_pattern(
                                            &mut engine,
                                            rel_pattern,
                                            &current_nodes,
                                            &target_nodes,
                                        ) {
                                            let execution_time =
                                                start_time.elapsed().as_millis() as u64;
                                            tracing::error!("{}", err);
                                            return Json(CypherResponse {
                                                columns: vec![],
                                                rows: vec![],
                                                execution_time_ms: execution_time,
                                                error: Some(err),
                                                notifications: Vec::new(),
                                            });
                                        }

                                        current_nodes = target_nodes;
                                        index += 2;
                                    }
                                    nexus_core::executor::parser::PatternElement::Node(_) => {
                                        break;
                                    }
                                    nexus_core::executor::parser::PatternElement::QuantifiedGroup(_) => {
                                        break;
                                    }
                                }
                            }
                        }
                        nexus_core::executor::parser::PatternElement::Relationship(_) => {
                            tracing::warn!(
                                "CREATE clause encountered relationship without leading node; skipping"
                            );
                            index += 1;
                        }
                    }
                }

                // Log successful CREATE operation
                let (user_id, username, api_key_id) = get_actor_info();
                let _ = server
                    .audit_logger
                    .log_write_operation(nexus_core::auth::WriteOperationParams {
                        actor_user_id: user_id,
                        actor_username: username,
                        api_key_id,
                        operation_type: "CREATE".to_string(),
                        entity_type: "PATTERN".to_string(), // Could be NODE or RELATIONSHIP, using PATTERN as generic
                        entity_id: None,
                        cypher_query: Some(request.query.clone()),
                        result: nexus_core::auth::AuditResult::Success,
                    })
                    .await;
            } else if let nexus_core::executor::parser::Clause::Merge(merge_clause) = clause {
                // Extract pattern and try to find existing node, or create new one
                for element in &merge_clause.pattern.elements {
                    if let nexus_core::executor::parser::PatternElement::Node(node_pattern) =
                        element
                    {
                        let labels = node_pattern.labels.clone();

                        // Convert properties
                        let mut props = serde_json::Map::new();
                        if let Some(prop_map) = &node_pattern.properties {
                            for (key, expr) in &prop_map.properties {
                                // Convert expression to JSON value
                                let value = match expr {
                                    nexus_core::executor::parser::Expression::Literal(lit) => {
                                        match lit {
                                            nexus_core::executor::parser::Literal::String(s) => {
                                                serde_json::Value::String(s.clone())
                                            }
                                            nexus_core::executor::parser::Literal::Integer(i) => {
                                                serde_json::Value::Number((*i).into())
                                            }
                                            nexus_core::executor::parser::Literal::Float(f) => {
                                                serde_json::Number::from_f64(*f)
                                                    .map(serde_json::Value::Number)
                                                    .unwrap_or(serde_json::Value::Null)
                                            }
                                            nexus_core::executor::parser::Literal::Boolean(b) => {
                                                serde_json::Value::Bool(*b)
                                            }
                                            nexus_core::executor::parser::Literal::Null => {
                                                serde_json::Value::Null
                                            }
                                            nexus_core::executor::parser::Literal::Point(p) => {
                                                p.to_json_value()
                                            }
                                        }
                                    }
                                    _ => serde_json::Value::Null,
                                };
                                props.insert(key.clone(), value);
                            }
                        }

                        let properties = serde_json::Value::Object(props.clone());

                        // MERGE: Try to find existing node, or create new one
                        // First, try to find an existing node with matching labels
                        let mut found_node = false;
                        if let Some(first_label) = labels.first() {
                            // Get label ID
                            if let Ok(label_id) = engine.catalog.get_or_create_label(first_label) {
                                // Get all nodes with this label from label_index
                                if let Ok(node_ids) = engine.indexes.label_index.get_nodes(label_id)
                                {
                                    // Iterate through nodes and check if properties match
                                    for node_id in node_ids {
                                        if let Ok(Some(existing_props)) =
                                            engine.storage.load_node_properties(node_id as u64)
                                        {
                                            // Check if all properties from MERGE match existing properties
                                            let props_obj = properties.as_object().unwrap();
                                            let mut all_match = true;

                                            for (key, value) in props_obj {
                                                if let Some(existing_value) =
                                                    existing_props.get(key)
                                                {
                                                    if existing_value != value {
                                                        all_match = false;
                                                        break;
                                                    }
                                                } else {
                                                    all_match = false;
                                                    break;
                                                }
                                            }

                                            if all_match && !props_obj.is_empty() {
                                                // Found matching node, don't create
                                                let existing_node_id = node_id as u64;
                                                tracing::info!(
                                                    "MERGE: Found existing node {} with matching properties",
                                                    existing_node_id
                                                );

                                                // Store node_id in variable context if variable exists
                                                if let Some(var_name) = &node_pattern.variable {
                                                    variable_context
                                                        .entry(var_name.clone())
                                                        .or_default()
                                                        .push(existing_node_id);
                                                }

                                                found_node = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // If no matching node found, create new one
                        if !found_node {
                            tracing::debug!(
                                "🔍 MERGE creating node with {} properties: {:?}",
                                props.len(),
                                props.keys().collect::<Vec<_>>()
                            );
                            match engine
                                .create_node(labels, serde_json::Value::Object(props.clone()))
                            {
                                Ok(node_id) => {
                                    tracing::info!(
                                        "MERGE: Created new node {} via Engine",
                                        node_id
                                    );

                                    // Store node_id in variable context if variable exists
                                    if let Some(var_name) = &node_pattern.variable {
                                        variable_context
                                            .entry(var_name.clone())
                                            .or_default()
                                            .push(node_id);
                                    }

                                    // Execute ON CREATE clause if provided
                                    if let Some(on_create_set) = &merge_clause.on_create {
                                        if let Some(_var_name) = &node_pattern.variable {
                                            tracing::info!(
                                                "Executing ON CREATE clause for node {}",
                                                node_id
                                            );
                                            // Execute SET operations from ON CREATE
                                            for item in &on_create_set.items {
                                                match item {
                                                    nexus_core::executor::parser::SetItem::Property { target: _, property, value } => {
                                                        let mut properties = match engine.storage.load_node_properties(node_id) {
                                                            Ok(Some(props)) => props.as_object().unwrap().clone(),
                                                            _ => serde_json::Map::new(),
                                                        };
                                                        let json_value = expression_to_json_value(value);
                                                        properties.insert(property.clone(), json_value);

                                                        if let Ok(Some(node_record)) = engine.get_node(node_id) {
                                                            let labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                            let _ = engine.update_node(node_id, labels, serde_json::Value::Object(properties));
                                                        }
                                                    }
                                                    nexus_core::executor::parser::SetItem::Label { target: _, label } => {
                                                        if let Ok(Some(node_record)) = engine.get_node(node_id) {
                                                            let mut labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                            if !labels.contains(label) {
                                                                labels.push(label.clone());
                                                            }
                                                            let properties = match engine.storage.load_node_properties(node_id) {
                                                                Ok(Some(props)) => props,
                                                                _ => serde_json::Value::Object(serde_json::Map::new()),
                                                            };
                                                            let _ = engine.update_node(node_id, labels, properties);
                                                        }
                                                    }
                                                    // phase6_opencypher-quickwins §6 — SET lhs += mapExpr.
                                                    // Merge the literal map into the target's property bag.
                                                    // NULL-valued entries remove the key; absent keys are preserved.
                                                    nexus_core::executor::parser::SetItem::MapMerge { target: _, map } => {
                                                        let rhs = expression_to_json_value(map);
                                                        if let serde_json::Value::Object(rhs_map) = rhs {
                                                            let mut properties = match engine.storage.load_node_properties(node_id) {
                                                                Ok(Some(props)) => props.as_object().cloned().unwrap_or_default(),
                                                                _ => serde_json::Map::new(),
                                                            };
                                                            for (k, v) in rhs_map.into_iter() {
                                                                if matches!(v, serde_json::Value::Null) {
                                                                    properties.remove(&k);
                                                                } else {
                                                                    properties.insert(k, v);
                                                                }
                                                            }
                                                            if let Ok(Some(node_record)) = engine.get_node(node_id) {
                                                                let labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                                let _ = engine.update_node(node_id, labels, serde_json::Value::Object(properties));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    let execution_time = start_time.elapsed().as_millis() as u64;
                                    tracing::error!("Failed to merge node: {}", e);
                                    return Json(CypherResponse {
                                        columns: vec![],
                                        rows: vec![],
                                        execution_time_ms: execution_time,
                                        error: Some(format!("Failed to merge node: {}", e)),
                                        notifications: Vec::new(),
                                    });
                                }
                            }
                        } else {
                            // Node found, execute ON MATCH clause if provided
                            if let Some(on_match_set) = &merge_clause.on_match {
                                if let Some(var_name) = &node_pattern.variable {
                                    // Get the node_id we found earlier from variable context
                                    if let Some(node_ids) = variable_context.get(var_name) {
                                        for node_id in node_ids {
                                            tracing::info!(
                                                "Executing ON MATCH clause for node {}",
                                                node_id
                                            );
                                            // Execute SET operations from ON MATCH
                                            for item in &on_match_set.items {
                                                match item {
                                                    nexus_core::executor::parser::SetItem::Property { target: _, property, value } => {
                                                        let mut properties = match engine.storage.load_node_properties(*node_id) {
                                                            Ok(Some(props)) => props.as_object().unwrap().clone(),
                                                            _ => serde_json::Map::new(),
                                                        };
                                                        let json_value = expression_to_json_value(value);
                                                        properties.insert(property.clone(), json_value);

                                                        if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                                            let labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                            let _ = engine.update_node(*node_id, labels, serde_json::Value::Object(properties));
                                                        }
                                                    }
                                                    nexus_core::executor::parser::SetItem::Label { target: _, label } => {
                                                        if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                                            let mut labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                            if !labels.contains(label) {
                                                                labels.push(label.clone());
                                                            }
                                                            let properties = match engine.storage.load_node_properties(*node_id) {
                                                                Ok(Some(props)) => props,
                                                                _ => serde_json::Value::Object(serde_json::Map::new()),
                                                            };
                                                            let _ = engine.update_node(*node_id, labels, properties);
                                                        }
                                                    }
                                                    // phase6_opencypher-quickwins §6 — SET lhs += mapExpr in ON MATCH.
                                                    nexus_core::executor::parser::SetItem::MapMerge { target: _, map } => {
                                                        let rhs = expression_to_json_value(map);
                                                        if let serde_json::Value::Object(rhs_map) = rhs {
                                                            let mut properties = match engine.storage.load_node_properties(*node_id) {
                                                                Ok(Some(props)) => props.as_object().cloned().unwrap_or_default(),
                                                                _ => serde_json::Map::new(),
                                                            };
                                                            for (k, v) in rhs_map.into_iter() {
                                                                if matches!(v, serde_json::Value::Null) {
                                                                    properties.remove(&k);
                                                                } else {
                                                                    properties.insert(k, v);
                                                                }
                                                            }
                                                            if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                                                let labels = engine.catalog.get_labels_from_bitmap(node_record.label_bits).unwrap_or_default();
                                                                let _ = engine.update_node(*node_id, labels, serde_json::Value::Object(properties));
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Handle SET clause
            else if let nexus_core::executor::parser::Clause::Set(set_clause) = clause {
                tracing::info!("SET clause detected: {} items", set_clause.items.len());
                for item in &set_clause.items {
                    match item {
                        nexus_core::executor::parser::SetItem::Property {
                            target,
                            property,
                            value,
                        } => {
                            // Look up nodes from variable context
                            if let Some(node_ids) = variable_context.get(target) {
                                for node_id in node_ids {
                                    // Load existing properties
                                    let mut properties =
                                        match engine.storage.load_node_properties(*node_id) {
                                            Ok(Some(props)) => props.as_object().unwrap().clone(),
                                            _ => serde_json::Map::new(),
                                        };

                                    // Convert expression to JSON value
                                    let json_value = expression_to_json_value(value);

                                    // Update or add the property
                                    properties.insert(property.clone(), json_value);

                                    // Load existing labels
                                    if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                        let labels = engine
                                            .catalog
                                            .get_labels_from_bitmap(node_record.label_bits)
                                            .unwrap_or_default();

                                        // Update the node with new properties
                                        if let Err(e) = engine.update_node(
                                            *node_id,
                                            labels,
                                            serde_json::Value::Object(properties),
                                        ) {
                                            tracing::error!(
                                                "Failed to update node {}: {}",
                                                node_id,
                                                e
                                            );

                                            // Log failed SET operation
                                            let (user_id, username, api_key_id) = get_actor_info();
                                            if let Err(audit_err) = server
                                                .audit_logger
                                                .log_write_operation(
                                                    nexus_core::auth::WriteOperationParams {
                                                        actor_user_id: user_id,
                                                        actor_username: username,
                                                        api_key_id,
                                                        operation_type: "SET".to_string(),
                                                        entity_type: "PROPERTY".to_string(),
                                                        entity_id: Some(node_id.to_string()),
                                                        cypher_query: Some(request.query.clone()),
                                                        result:
                                                            nexus_core::auth::AuditResult::Failure {
                                                                error: format!(
                                                                    "Failed to update node {}: {}",
                                                                    node_id, e
                                                                ),
                                                            },
                                                    },
                                                )
                                                .await
                                            {
                                                nexus_core::auth::record_audit_log_failure(
                                                    "set_property_failure",
                                                    &audit_err,
                                                );
                                            }
                                        } else {
                                            tracing::info!(
                                                "SET {}.{} on node {}",
                                                target,
                                                property,
                                                node_id
                                            );

                                            // Log successful SET operation
                                            let (user_id, username, api_key_id) = get_actor_info();
                                            if let Err(audit_err) = server
                                                .audit_logger
                                                .log_write_operation(
                                                    nexus_core::auth::WriteOperationParams {
                                                        actor_user_id: user_id,
                                                        actor_username: username,
                                                        api_key_id,
                                                        operation_type: "SET".to_string(),
                                                        entity_type: "PROPERTY".to_string(),
                                                        entity_id: Some(node_id.to_string()),
                                                        cypher_query: Some(request.query.clone()),
                                                        result:
                                                            nexus_core::auth::AuditResult::Success,
                                                    },
                                                )
                                                .await
                                            {
                                                nexus_core::auth::record_audit_log_failure(
                                                    "set_property_success",
                                                    &audit_err,
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("Variable {} not found in context", target);
                            }
                        }
                        nexus_core::executor::parser::SetItem::Label { target, label } => {
                            // Look up nodes from variable context
                            if let Some(node_ids) = variable_context.get(target) {
                                for node_id in node_ids {
                                    // Load existing node to get current labels
                                    if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                        let mut labels = engine
                                            .catalog
                                            .get_labels_from_bitmap(node_record.label_bits)
                                            .unwrap_or_default();

                                        // Add new label if not already present
                                        if !labels.contains(label) {
                                            labels.push(label.clone());
                                        }

                                        // Load properties
                                        let properties = match engine
                                            .storage
                                            .load_node_properties(*node_id)
                                        {
                                            Ok(Some(props)) => props,
                                            _ => serde_json::Value::Object(serde_json::Map::new()),
                                        };

                                        // Update the node with new labels
                                        if let Err(e) =
                                            engine.update_node(*node_id, labels, properties)
                                        {
                                            tracing::error!(
                                                "Failed to update node {} with label {}: {}",
                                                node_id,
                                                label,
                                                e
                                            );

                                            // Log failed SET operation
                                            let (user_id, username, api_key_id) = get_actor_info();
                                            if let Err(audit_err) = server
                                                .audit_logger
                                                .log_write_operation(
                                                    nexus_core::auth::WriteOperationParams {
                                                        actor_user_id: user_id,
                                                        actor_username: username,
                                                        api_key_id,
                                                        operation_type: "SET".to_string(),
                                                        entity_type: "LABEL".to_string(),
                                                        entity_id: Some(node_id.to_string()),
                                                        cypher_query: Some(request.query.clone()),
                                                        result: nexus_core::auth::AuditResult::Failure {
                                                            error: format!("Failed to update node {} with label {}: {}", node_id, label, e),
                                                        },
                                                    },
                                                )
                                                .await
                                            {
                                                nexus_core::auth::record_audit_log_failure(
                                                    "set_label_failure",
                                                    &audit_err,
                                                );
                                            }
                                        } else {
                                            tracing::info!(
                                                "SET {}:{} on node {}",
                                                target,
                                                label,
                                                node_id
                                            );

                                            // Log successful SET operation
                                            let (user_id, username, api_key_id) = get_actor_info();
                                            if let Err(audit_err) = server
                                                .audit_logger
                                                .log_write_operation(
                                                    nexus_core::auth::WriteOperationParams {
                                                        actor_user_id: user_id,
                                                        actor_username: username,
                                                        api_key_id,
                                                        operation_type: "SET".to_string(),
                                                        entity_type: "LABEL".to_string(),
                                                        entity_id: Some(node_id.to_string()),
                                                        cypher_query: Some(request.query.clone()),
                                                        result:
                                                            nexus_core::auth::AuditResult::Success,
                                                    },
                                                )
                                                .await
                                            {
                                                nexus_core::auth::record_audit_log_failure(
                                                    "set_label_success",
                                                    &audit_err,
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("Variable {} not found in context", target);
                            }
                        }
                        // phase6_opencypher-quickwins §6 — top-level
                        // `SET lhs += mapExpr` on the /cypher path.
                        nexus_core::executor::parser::SetItem::MapMerge { target, map } => {
                            if let Some(node_ids) = variable_context.get(target) {
                                for node_id in node_ids {
                                    let rhs = expression_to_json_value(map);
                                    if let serde_json::Value::Object(rhs_map) = rhs {
                                        let mut properties =
                                            match engine.storage.load_node_properties(*node_id) {
                                                Ok(Some(props)) => {
                                                    props.as_object().cloned().unwrap_or_default()
                                                }
                                                _ => serde_json::Map::new(),
                                            };
                                        for (k, v) in rhs_map.into_iter() {
                                            if matches!(v, serde_json::Value::Null) {
                                                properties.remove(&k);
                                            } else {
                                                properties.insert(k, v);
                                            }
                                        }
                                        if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                            let labels = engine
                                                .catalog
                                                .get_labels_from_bitmap(node_record.label_bits)
                                                .unwrap_or_default();
                                            let _ = engine.update_node(
                                                *node_id,
                                                labels,
                                                serde_json::Value::Object(properties),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Handle DELETE clause
            else if let nexus_core::executor::parser::Clause::Delete(delete_clause) = clause {
                tracing::info!(
                    "DELETE clause detected: {} items, detach={}",
                    delete_clause.items.len(),
                    delete_clause.detach
                );
                for item in &delete_clause.items {
                    // Look up nodes from variable context
                    if let Some(node_ids) = variable_context.get(item) {
                        for node_id in node_ids {
                            if delete_clause.detach {
                                // DETACH DELETE: Remove all relationships before deleting
                                let mut deleted_rels = 0;
                                let total_rels = engine.storage.relationship_count();

                                // Scan all relationships
                                for rel_id in 0..total_rels {
                                    if let Ok(Some(rel_record)) = engine.get_relationship(rel_id) {
                                        // Check if this relationship is connected to the node we're deleting
                                        if rel_record.src_id == *node_id
                                            || rel_record.dst_id == *node_id
                                        {
                                            // Delete the relationship by marking it as deleted
                                            // Use storage's delete_rel method which handles transaction internally
                                            engine.storage.delete_rel(rel_id).unwrap();
                                            deleted_rels += 1;
                                        }
                                    }
                                }
                                tracing::info!(
                                    "DETACH DELETE: Removed {} relationships from node {}",
                                    deleted_rels,
                                    node_id
                                );
                            }

                            // Delete the node
                            match engine.delete_node(*node_id) {
                                Ok(deleted) => {
                                    if deleted {
                                        tracing::info!("DELETE node {}", node_id);

                                        // Log successful DELETE operation
                                        let (user_id, username, api_key_id) = get_actor_info();
                                        let _ = server
                                            .audit_logger
                                            .log_write_operation(
                                                nexus_core::auth::WriteOperationParams {
                                                    actor_user_id: user_id,
                                                    actor_username: username,
                                                    api_key_id,
                                                    operation_type: "DELETE".to_string(),
                                                    entity_type: "NODE".to_string(),
                                                    entity_id: Some(node_id.to_string()),
                                                    cypher_query: Some(request.query.clone()),
                                                    result: nexus_core::auth::AuditResult::Success,
                                                },
                                            )
                                            .await;
                                    } else {
                                        tracing::warn!("Node {} not found for deletion", node_id);

                                        // Log failed DELETE operation (node not found)
                                        let (user_id, username, api_key_id) = get_actor_info();
                                        let _ = server
                                            .audit_logger
                                            .log_write_operation(
                                                nexus_core::auth::WriteOperationParams {
                                                    actor_user_id: user_id,
                                                    actor_username: username,
                                                    api_key_id,
                                                    operation_type: "DELETE".to_string(),
                                                    entity_type: "NODE".to_string(),
                                                    entity_id: Some(node_id.to_string()),
                                                    cypher_query: Some(request.query.clone()),
                                                    result:
                                                        nexus_core::auth::AuditResult::Failure {
                                                            error: format!(
                                                                "Node {} not found",
                                                                node_id
                                                            ),
                                                        },
                                                },
                                            )
                                            .await;
                                    }
                                }
                                Err(e) => {
                                    tracing::error!("Failed to delete node {}: {}", node_id, e);

                                    // Log failed DELETE operation
                                    let (user_id, username, api_key_id) = get_actor_info();
                                    let _ = server
                                        .audit_logger
                                        .log_write_operation(
                                            nexus_core::auth::WriteOperationParams {
                                                actor_user_id: user_id,
                                                actor_username: username,
                                                api_key_id,
                                                operation_type: "DELETE".to_string(),
                                                entity_type: "NODE".to_string(),
                                                entity_id: Some(node_id.to_string()),
                                                cypher_query: Some(request.query.clone()),
                                                result: nexus_core::auth::AuditResult::Failure {
                                                    error: format!(
                                                        "Failed to delete node {}: {}",
                                                        node_id, e
                                                    ),
                                                },
                                            },
                                        )
                                        .await;
                                }
                            }
                        }
                    } else {
                        tracing::warn!("Variable {} not found in context", item);
                    }
                }
            }
            // Handle REMOVE clause
            else if let nexus_core::executor::parser::Clause::Remove(remove_clause) = clause {
                tracing::info!(
                    "REMOVE clause detected: {} items",
                    remove_clause.items.len()
                );
                for item in &remove_clause.items {
                    match item {
                        nexus_core::executor::parser::RemoveItem::Property { target, property } => {
                            // Look up nodes from variable context
                            if let Some(node_ids) = variable_context.get(target) {
                                for node_id in node_ids {
                                    // Load existing properties
                                    if let Ok(Some(mut properties)) =
                                        engine.storage.load_node_properties(*node_id)
                                    {
                                        let props = properties.as_object_mut().unwrap();
                                        props.remove(property);

                                        // Load existing labels
                                        if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                            let labels = engine
                                                .catalog
                                                .get_labels_from_bitmap(node_record.label_bits)
                                                .unwrap_or_default();

                                            // Update the node with removed property
                                            if let Err(e) =
                                                engine.update_node(*node_id, labels, properties)
                                            {
                                                tracing::error!(
                                                    "Failed to remove property {} from node {}: {}",
                                                    property,
                                                    node_id,
                                                    e
                                                );
                                            } else {
                                                tracing::info!(
                                                    "REMOVE {}.{} from node {}",
                                                    target,
                                                    property,
                                                    node_id
                                                );
                                            }
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("Variable {} not found in context", target);
                            }
                        }
                        nexus_core::executor::parser::RemoveItem::Label { target, label } => {
                            // Look up nodes from variable context
                            if let Some(node_ids) = variable_context.get(target) {
                                for node_id in node_ids {
                                    // Load existing node to get current labels
                                    if let Ok(Some(node_record)) = engine.get_node(*node_id) {
                                        let mut labels = engine
                                            .catalog
                                            .get_labels_from_bitmap(node_record.label_bits)
                                            .unwrap_or_default();

                                        // Remove the label if present
                                        labels.retain(|l| l != label);

                                        // Load properties
                                        let properties = match engine
                                            .storage
                                            .load_node_properties(*node_id)
                                        {
                                            Ok(Some(props)) => props,
                                            _ => serde_json::Value::Object(serde_json::Map::new()),
                                        };

                                        // Update the node with removed label
                                        if let Err(e) =
                                            engine.update_node(*node_id, labels, properties)
                                        {
                                            tracing::error!(
                                                "Failed to remove label {} from node {}: {}",
                                                label,
                                                node_id,
                                                e
                                            );
                                        } else {
                                            tracing::info!(
                                                "REMOVE {}:{} from node {}",
                                                target,
                                                label,
                                                node_id
                                            );
                                        }
                                    }
                                }
                            } else {
                                tracing::warn!("Variable {} not found in context", target);
                            }
                        }
                    }
                }
            }
        }

        let execution_time = start_time.elapsed().as_millis() as u64;
        let clause_type = if is_merge_query { "MERGE" } else { "CREATE" };
        tracing::info!(
            "{} query executed successfully in {}ms",
            clause_type,
            execution_time
        );

        // Check if query has RETURN clause
        if let Some(return_clause) = ast.clauses.iter().find_map(|c| {
            if let nexus_core::executor::parser::Clause::Return(ret) = c {
                Some(ret)
            } else {
                None
            }
        }) {
            tracing::info!("Processing RETURN clause for CREATE/MERGE");

            // Build result from variable_context and RETURN projection
            let mut columns = Vec::new();
            let mut row_values = Vec::new();

            for item in &return_clause.items {
                // Column name: prefer explicit alias, then the variable name
                // for a bare `RETURN t`, then fall back to "result".
                let col_name = item.alias.clone().unwrap_or_else(|| {
                    if let nexus_core::executor::parser::Expression::Variable(v) = &item.expression
                    {
                        v.clone()
                    } else {
                        "result".to_string()
                    }
                });
                columns.push(col_name);

                // Evaluate the expression using the variable context
                let value = match &item.expression {
                    nexus_core::executor::parser::Expression::Variable(var_name) => {
                        // Bare `RETURN t` — serialize the whole node object using
                        // the same shape that the executor's `node_to_result_value`
                        // produces: {…props, _nexus_id: id, _nexus_labels: […]}.
                        if let Some(node_ids) = variable_context.get(var_name) {
                            if let Some(node_id) = node_ids.first() {
                                let node_id = *node_id;
                                // Match the executor's `read_node_as_value`
                                // shape exactly: {…properties, _nexus_id: id}
                                // so CREATE…RETURN n equals MATCH…RETURN n.
                                let mut map = match engine.storage.load_node_properties(node_id) {
                                    Ok(Some(serde_json::Value::Object(m))) => m,
                                    _ => serde_json::Map::new(),
                                };
                                map.insert(
                                    "_nexus_id".to_string(),
                                    serde_json::Value::Number(node_id.into()),
                                );
                                serde_json::Value::Object(map)
                            } else {
                                serde_json::Value::Null
                            }
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    nexus_core::executor::parser::Expression::PropertyAccess {
                        variable,
                        property,
                    } => {
                        if let Some(node_ids) = variable_context.get(variable) {
                            if let Some(node_id) = node_ids.first() {
                                // phase9_external-node-ids §4.7 — `n._id`
                                // is sourced from the catalog reverse map,
                                // not from the regular property store.
                                if property == "_id" {
                                    match engine.catalog.read_txn() {
                                        Ok(txn) => {
                                            match engine
                                                .catalog
                                                .external_id_index()
                                                .get_external(&txn, *node_id)
                                            {
                                                Ok(Some(ext)) => {
                                                    serde_json::Value::String(ext.to_string())
                                                }
                                                _ => serde_json::Value::Null,
                                            }
                                        }
                                        Err(_) => serde_json::Value::Null,
                                    }
                                } else {
                                    // Load node properties
                                    match engine.storage.load_node_properties(*node_id) {
                                        Ok(Some(props)) => props
                                            .get(property)
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                        _ => serde_json::Value::Null,
                                    }
                                }
                            } else {
                                serde_json::Value::Null
                            }
                        } else {
                            serde_json::Value::Null
                        }
                    }
                    nexus_core::executor::parser::Expression::Literal(lit) => match lit {
                        nexus_core::executor::parser::Literal::String(s) => {
                            serde_json::Value::String(s.clone())
                        }
                        nexus_core::executor::parser::Literal::Integer(i) => {
                            serde_json::Value::Number((*i).into())
                        }
                        nexus_core::executor::parser::Literal::Float(f) => {
                            serde_json::Number::from_f64(*f)
                                .map(serde_json::Value::Number)
                                .unwrap_or(serde_json::Value::Null)
                        }
                        nexus_core::executor::parser::Literal::Boolean(b) => {
                            serde_json::Value::Bool(*b)
                        }
                        nexus_core::executor::parser::Literal::Null => serde_json::Value::Null,
                        nexus_core::executor::parser::Literal::Point(p) => p.to_json_value(),
                    },
                    _ => serde_json::Value::Null,
                };

                row_values.push(value);
            }

            return Json(CypherResponse {
                columns,
                rows: vec![serde_json::Value::Array(row_values)],
                execution_time_ms: execution_time,
                error: None,
                notifications: Vec::new(),
            });
        }

        Json(CypherResponse {
            columns: vec![],
            rows: vec![],
            execution_time_ms: execution_time,
            error: None,
            notifications: Vec::new(),
        })
    }
}
