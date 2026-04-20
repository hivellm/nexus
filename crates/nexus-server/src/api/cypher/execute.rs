//! `execute_cypher` — the main Cypher query HTTP handler. Drives the
//! planner, runs the executor, handles write paths (CREATE / MERGE /
//! SET / REMOVE / DELETE / FOREACH), tracks metrics, and builds the
//! JSON response.

use super::*;

pub async fn execute_cypher(
    State(server): State<Arc<NexusServer>>,
    auth_context: Option<Extension<Option<AuthContext>>>,
    Json(request): Json<CypherRequest>,
) -> Json<CypherResponse> {
    tracing::debug!("[CYPHER-API] Received query: {}", request.query);
    let auth_context = auth_context.and_then(|e| e.0);
    let start_time = std::time::Instant::now();
    let query_for_tracking = request.query.clone();

    // Register connection and query for tracking
    // Note: ConnectInfo requires special router setup, using fallback for now
    let client_address = "unknown".to_string(); // Will be improved when ConnectInfo is enabled
    let connection_id = register_connection_and_query_fallback(
        &server,
        &query_for_tracking,
        &client_address,
        &auth_context,
    );
    let query_id = connection_id.clone(); // Use connection_id as query_id for simplicity

    tracing::info!("Executing Cypher query: {}", request.query);

    // Extract actor info from auth context for audit logging
    let actor_info = auth_context
        .as_ref()
        .map(|ctx| {
            let api_key_id = Some(ctx.api_key.id.clone());
            let user_id = ctx.api_key.user_id.clone();
            let username = None; // Username not available in ApiKey
            (user_id, username, api_key_id)
        })
        .unwrap_or((None, None, None));
    let get_actor_info =
        || -> (Option<String>, Option<String>, Option<String>) { actor_info.clone() };

    // Parse query first to check for admin commands
    use nexus_core::executor::parser::CypherParser;
    let mut parser = CypherParser::new(request.query.clone());
    let ast = match parser.parse() {
        Ok(ast) => ast,
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;
            tracing::error!("Parse error: {}", e);
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: execution_time,
                error: Some(format!("Parse error: {}", e)),
            });
        }
    };

    // Check for database management commands
    let has_db_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::CreateDatabase(_)
                | nexus_core::executor::parser::Clause::DropDatabase(_)
                | nexus_core::executor::parser::Clause::ShowDatabases
                | nexus_core::executor::parser::Clause::UseDatabase(_)
        )
    });

    if has_db_cmd {
        return execute_database_commands(server, &ast, start_time).await;
    }

    // Check for user management commands
    let has_user_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowUsers
                | nexus_core::executor::parser::Clause::ShowUser(_)
                | nexus_core::executor::parser::Clause::CreateUser(_)
                | nexus_core::executor::parser::Clause::DropUser(_)
                | nexus_core::executor::parser::Clause::Grant(_)
                | nexus_core::executor::parser::Clause::Revoke(_)
        )
    });

    // Check for API key management commands
    let has_api_key_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::CreateApiKey(_)
                | nexus_core::executor::parser::Clause::ShowApiKeys(_)
                | nexus_core::executor::parser::Clause::RevokeApiKey(_)
                | nexus_core::executor::parser::Clause::DeleteApiKey(_)
        )
    });

    if has_api_key_cmd {
        return execute_api_key_commands(server, &ast, start_time).await;
    }

    if has_user_cmd {
        return execute_user_commands(server, &ast, start_time).await;
    }

    // Check for query management commands (SHOW QUERIES, TERMINATE QUERY)
    let has_query_mgmt_cmd = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowQueries
                | nexus_core::executor::parser::Clause::TerminateQuery(_)
        )
    });

    if has_query_mgmt_cmd {
        return execute_query_management_commands(server.clone(), &ast, start_time).await;
    }

    // Check for SHOW CONSTRAINTS or SHOW FUNCTIONS commands
    let has_show_constraints_or_functions = ast.clauses.iter().any(|c| {
        matches!(
            c,
            nexus_core::executor::parser::Clause::ShowConstraints
                | nexus_core::executor::parser::Clause::ShowFunctions
                | nexus_core::executor::parser::Clause::CreateConstraint(_)
                | nexus_core::executor::parser::Clause::DropConstraint(_)
                | nexus_core::executor::parser::Clause::CreateFunction(_)
                | nexus_core::executor::parser::Clause::DropFunction(_)
        )
    });

    if has_show_constraints_or_functions {
        // Use Engine for these commands
        {
            let mut engine = server.engine.write().await;
            match engine.execute_cypher(&request.query) {
                Ok(result) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    let rows: Vec<serde_json::Value> = result
                        .rows
                        .into_iter()
                        .map(|row| serde_json::Value::Array(row.values))
                        .collect();
                    return Json(CypherResponse {
                        columns: result.columns,
                        rows,
                        execution_time_ms: execution_time,
                        error: None,
                    });
                }
                Err(e) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("Execution error: {}", e)),
                    });
                }
            }
        }
    }

    // Check if this is a CREATE, MERGE, SET, DELETE, REMOVE, or MATCH query
    let query_upper = request.query.trim().to_uppercase();
    let is_create_query = query_upper.starts_with("CREATE");
    let is_merge_query = query_upper.starts_with("MERGE");
    let _is_set_query = query_upper.starts_with("SET");
    let _is_delete_query = query_upper.starts_with("DELETE");
    let _is_remove_query = query_upper.starts_with("REMOVE");
    // MATCH queries can start with MATCH or have MATCH after UNWIND/WITH/OPTIONAL clauses
    // We need to detect MATCH anywhere in the query to route it through the Engine
    let is_match_query = query_upper.starts_with("MATCH")
        || query_upper.contains(" MATCH ")
        || query_upper.contains(" MATCH(")
        || query_upper.starts_with("OPTIONAL MATCH")
        || query_upper.contains(" OPTIONAL MATCH");

    if is_create_query || is_merge_query {
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
                            nexus_core::executor::parser::PatternElement::Node(node_pattern) => {
                                let mut current_nodes = match ensure_node_from_pattern(
                                    &mut engine,
                                    node_pattern,
                                    &mut variable_context,
                                ) {
                                    Ok(nodes) => nodes,
                                    Err(err) => {
                                        let execution_time =
                                            start_time.elapsed().as_millis() as u64;
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
                                                    result:
                                                        nexus_core::auth::AuditResult::Failure {
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
                                                });
                                            }

                                            current_nodes = target_nodes;
                                            index += 2;
                                        }
                                        nexus_core::executor::parser::PatternElement::Node(_) => {
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
                                                nexus_core::executor::parser::Literal::String(
                                                    s,
                                                ) => serde_json::Value::String(s.clone()),
                                                nexus_core::executor::parser::Literal::Integer(
                                                    i,
                                                ) => serde_json::Value::Number((*i).into()),
                                                nexus_core::executor::parser::Literal::Float(f) => {
                                                    serde_json::Number::from_f64(*f)
                                                        .map(serde_json::Value::Number)
                                                        .unwrap_or(serde_json::Value::Null)
                                                }
                                                nexus_core::executor::parser::Literal::Boolean(
                                                    b,
                                                ) => serde_json::Value::Bool(*b),
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
                                if let Ok(label_id) =
                                    engine.catalog.get_or_create_label(first_label)
                                {
                                    // Get all nodes with this label from label_index
                                    if let Ok(node_ids) =
                                        engine.indexes.label_index.get_nodes(label_id)
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
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let execution_time =
                                            start_time.elapsed().as_millis() as u64;
                                        tracing::error!("Failed to merge node: {}", e);
                                        return Json(CypherResponse {
                                            columns: vec![],
                                            rows: vec![],
                                            execution_time_ms: execution_time,
                                            error: Some(format!("Failed to merge node: {}", e)),
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
                                        let mut properties = match engine
                                            .storage
                                            .load_node_properties(*node_id)
                                        {
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
                                                let (user_id, username, api_key_id) =
                                                    get_actor_info();
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
                                                            result: nexus_core::auth::AuditResult::Failure {
                                                                error: format!("Failed to update node {}: {}", node_id, e),
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
                                                let (user_id, username, api_key_id) =
                                                    get_actor_info();
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
                                                            result: nexus_core::auth::AuditResult::Success,
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
                                            let properties =
                                                match engine.storage.load_node_properties(*node_id)
                                                {
                                                    Ok(Some(props)) => props,
                                                    _ => serde_json::Value::Object(
                                                        serde_json::Map::new(),
                                                    ),
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
                                                let (user_id, username, api_key_id) =
                                                    get_actor_info();
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
                                                let (user_id, username, api_key_id) =
                                                    get_actor_info();
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
                                                            result: nexus_core::auth::AuditResult::Success,
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
                                        if let Ok(Some(rel_record)) =
                                            engine.get_relationship(rel_id)
                                        {
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
                                                        result:
                                                            nexus_core::auth::AuditResult::Success,
                                                    },
                                                )
                                                .await;
                                        } else {
                                            tracing::warn!(
                                                "Node {} not found for deletion",
                                                node_id
                                            );

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
                                                    result:
                                                        nexus_core::auth::AuditResult::Failure {
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
                            nexus_core::executor::parser::RemoveItem::Property {
                                target,
                                property,
                            } => {
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
                                            if let Ok(Some(node_record)) = engine.get_node(*node_id)
                                            {
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
                                            let properties =
                                                match engine.storage.load_node_properties(*node_id)
                                                {
                                                    Ok(Some(props)) => props,
                                                    _ => serde_json::Value::Object(
                                                        serde_json::Map::new(),
                                                    ),
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
                    columns.push(item.alias.clone().unwrap_or_else(|| "result".to_string()));

                    // Evaluate the expression using the variable context
                    let value = match &item.expression {
                        nexus_core::executor::parser::Expression::PropertyAccess {
                            variable,
                            property,
                        } => {
                            if let Some(node_ids) = variable_context.get(variable) {
                                if let Some(node_id) = node_ids.first() {
                                    // Load node properties
                                    match engine.storage.load_node_properties(*node_id) {
                                        Ok(Some(props)) => props
                                            .get(property)
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                        _ => serde_json::Value::Null,
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
                });
            }

            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: execution_time,
                error: None,
            });
        }
    }

    // For MATCH queries, use the engine's executor to access the shared storage
    if is_match_query {
        {
            // Use the engine's execute_cypher method which uses its internal executor
            let mut engine_guard = server.engine.write().await;
            match engine_guard.execute_cypher(&request.query) {
                Ok(result_set) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    tracing::info!(
                        "MATCH query executed successfully in {}ms, {} rows returned",
                        execution_time,
                        result_set.rows.len()
                    );

                    return Json(CypherResponse {
                        columns: result_set.columns,
                        rows: result_set
                            .rows
                            .into_iter()
                            .map(|row| serde_json::Value::Array(row.values))
                            .collect(),
                        execution_time_ms: execution_time,
                        error: None,
                    });
                }
                Err(e) => {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    tracing::error!("MATCH query execution failed: {}", e);

                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(e.to_string()),
                    });
                }
            }
        }
    }

    // Get executor instance for other queries
    // Executor is Clone and contains only Arc internally, so we can clone directly
    // without any locks - this enables true parallel execution
    let executor = server.executor.clone();

    // Create query
    let query = Query {
        cypher: request.query.clone(),
        params: request.params,
    };

    // Check cache status before execution
    let (cache_hits, cache_misses) = check_query_cache_status(&server, &request.query);

    // Track memory usage during query execution
    let initial_memory =
        nexus_core::performance::memory_tracking::QueryMemoryTracker::get_current_memory_usage()
            .ok();

    // Execute query - clone executor for concurrent execution
    // This removes the global lock bottleneck - each query gets its own executor clone
    // that shares the underlying data structures (catalog, store, indexes) via Arc
    // Use spawn_blocking to execute in a separate thread pool for true parallelism
    // No lock needed - Executor is Clone and Arc is thread-safe
    let executor_clone = executor.clone();
    let query_clone = query.clone();

    // Debug: Log thread info before spawning
    let thread_id_before = std::thread::current().id();
    tracing::debug!("Spawning blocking task from thread {:?}", thread_id_before);

    // Execute in blocking thread pool for true parallel execution
    // This allows multiple queries to run concurrently across CPU cores
    // Tokio's blocking thread pool automatically scales with CPU count
    let execution_result = match tokio::task::spawn_blocking(move || {
        let thread_id_after = std::thread::current().id();
        tracing::debug!("Executing in blocking thread {:?}", thread_id_after);

        let result = executor_clone.execute(&query_clone);
        tracing::debug!(
            "Query executed successfully in blocking thread {:?}",
            thread_id_after
        );
        result
    })
    .await
    {
        Ok(result) => result,
        Err(e) => {
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some(format!("Task execution error: {}", e)),
            });
        }
    };

    // Get memory delta after execution
    let memory_usage = initial_memory.and_then(|initial| {
        nexus_core::performance::memory_tracking::QueryMemoryTracker::get_current_memory_usage()
            .ok()
            .map(|final_memory| final_memory.saturating_sub(initial))
            .filter(|&delta| delta > 1024) // Only include if > 1KB
    });

    match execution_result {
        Ok(result_set) => {
            let execution_time = start_time.elapsed();
            let execution_time_ms = execution_time.as_millis() as u64;
            let rows_count = result_set.rows.len();

            tracing::info!(
                "Query executed successfully in {}ms, {} rows returned{}",
                execution_time_ms,
                rows_count,
                memory_usage
                    .map(|m| format!(", {} bytes memory", m))
                    .unwrap_or_default()
            );

            // Record successful query execution with cache and memory metrics
            record_query_execution_with_metrics(
                &server,
                &query_for_tracking,
                execution_time,
                true,
                None,
                rows_count,
                memory_usage,
                Some(cache_hits),
                Some(cache_misses),
            );

            // Record Prometheus metrics
            let cache_hit = cache_hits > 0;
            record_prometheus_metrics(&server, execution_time_ms, true, cache_hit);

            // Mark query as completed
            mark_query_completed(&server, &query_id);

            Json(CypherResponse {
                columns: result_set.columns,
                rows: result_set
                    .rows
                    .into_iter()
                    .map(|row| serde_json::Value::Array(row.values))
                    .collect(),
                execution_time_ms,
                error: None,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed();
            let execution_time_ms = execution_time.as_millis() as u64;
            let error_msg = e.to_string();

            tracing::error!("Query execution failed: {}", error_msg);

            // Get memory delta even for failed queries (already calculated above)

            // Record failed query execution with cache and memory metrics
            record_query_execution_with_metrics(
                &server,
                &query_for_tracking,
                execution_time,
                false,
                Some(error_msg.clone()),
                0,
                memory_usage,
                Some(cache_hits),
                Some(cache_misses),
            );

            // Record Prometheus metrics
            let cache_hit = cache_hits > 0;
            record_prometheus_metrics(&server, execution_time_ms, false, cache_hit);

            // Mark query as completed
            mark_query_completed(&server, &query_id);

            Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms,
                error: Some(error_msg),
            })
        }
    }
}
