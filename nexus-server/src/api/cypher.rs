//! Cypher query execution endpoint

use crate::NexusServer;
use axum::extract::{Json, State};
use nexus_core::auth::Permission;
use nexus_core::executor::parser::PropertyMap;
use nexus_core::executor::{Executor, Query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Global executor instance
static EXECUTOR: std::sync::OnceLock<Arc<RwLock<Executor>>> = std::sync::OnceLock::new();

/// Global engine instance for CREATE operations
static ENGINE: std::sync::OnceLock<Arc<RwLock<nexus_core::Engine>>> = std::sync::OnceLock::new();

/// Initialize the executor (deprecated - use init_engine_with_executor instead)
pub fn init_executor() -> anyhow::Result<Arc<RwLock<Executor>>> {
    let executor = Executor::default();
    let executor_arc = Arc::new(RwLock::new(executor));
    EXECUTOR
        .set(executor_arc.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(executor_arc)
}

/// Initialize the engine
pub fn init_engine(engine: Arc<RwLock<nexus_core::Engine>>) -> anyhow::Result<()> {
    ENGINE
        .set(engine.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set engine"))?;
    Ok(())
}

/// Initialize both engine and executor with shared storage
pub fn init_engine_with_executor(engine: Arc<RwLock<nexus_core::Engine>>) -> anyhow::Result<()> {
    // Set the engine
    ENGINE
        .set(engine.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set engine"))?;

    // Create a wrapper for the executor that's inside the engine
    // We'll use a pattern where we access the engine's executor via the engine itself
    // For now, we'll still use a dummy executor for non-CREATE queries
    // The real solution is to make CREATE and MATCH both use the engine
    let executor = Executor::default();
    let executor_arc = Arc::new(RwLock::new(executor));
    EXECUTOR
        .set(executor_arc)
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;

    Ok(())
}

/// Get the executor instance
pub fn get_executor() -> Arc<RwLock<Executor>> {
    EXECUTOR.get().expect("Executor not initialized").clone()
}

/// Helper function to convert Expression to JSON Value
fn expression_to_json_value(expr: &nexus_core::executor::parser::Expression) -> serde_json::Value {
    match expr {
        nexus_core::executor::parser::Expression::Literal(lit) => match lit {
            nexus_core::executor::parser::Literal::String(s) => {
                serde_json::Value::String(s.clone())
            }
            nexus_core::executor::parser::Literal::Integer(i) => {
                serde_json::Value::Number((*i).into())
            }
            nexus_core::executor::parser::Literal::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            nexus_core::executor::parser::Literal::Boolean(b) => serde_json::Value::Bool(*b),
            nexus_core::executor::parser::Literal::Null => serde_json::Value::Null,
        },
        nexus_core::executor::parser::Expression::PropertyAccess {
            variable: _,
            property: _,
        } => {
            eprintln!("⚠️  expression_to_json_value: Property expression not supported in CREATE");
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Variable(_) => {
            eprintln!("⚠️  expression_to_json_value: Variable expression not supported in CREATE");
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Parameter(_) => {
            eprintln!("⚠️  expression_to_json_value: Parameter expression not supported in CREATE");
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Map(map) => {
            // This is a nested map expression - convert it
            let mut result = serde_json::Map::new();
            for (key, expr) in map {
                result.insert(key.clone(), expression_to_json_value(expr));
            }
            serde_json::Value::Object(result)
        }
        _ => {
            eprintln!(
                "⚠️  expression_to_json_value: Unsupported expression type: {:?}",
                expr
            );
            serde_json::Value::Null
        }
    }
}

fn property_map_to_json(property_map: &Option<PropertyMap>) -> serde_json::Value {
    let mut props = serde_json::Map::new();

    if let Some(prop_map) = property_map {
        for (key, expr) in &prop_map.properties {
            let value = expression_to_json_value(expr);
            props.insert(key.clone(), value);
        }
    }

    serde_json::Value::Object(props)
}

fn ensure_node_from_pattern(
    engine: &mut nexus_core::Engine,
    node_pattern: &nexus_core::executor::parser::NodePattern,
    variable_context: &mut HashMap<String, Vec<u64>>,
) -> Result<Vec<u64>, String> {
    if let Some(var_name) = &node_pattern.variable {
        if let Some(existing) = variable_context.get(var_name) {
            if !existing.is_empty() {
                return Ok(existing.clone());
            }
        }
    }

    let properties = property_map_to_json(&node_pattern.properties);

    match engine.create_node(node_pattern.labels.clone(), properties) {
        Ok(node_id) => {
            if let Some(var_name) = &node_pattern.variable {
                variable_context
                    .entry(var_name.clone())
                    .or_default()
                    .push(node_id);
            }
            Ok(vec![node_id])
        }
        Err(e) => Err(format!("Failed to create node: {}", e)),
    }
}

fn create_relationship_from_pattern(
    engine: &mut nexus_core::Engine,
    rel_pattern: &nexus_core::executor::parser::RelationshipPattern,
    source_ids: &[u64],
    target_ids: &[u64],
) -> Result<(), String> {
    if source_ids.is_empty() || target_ids.is_empty() {
        return Ok(());
    }

    let rel_type = rel_pattern
        .types
        .first()
        .cloned()
        .unwrap_or_else(|| "RELATIONSHIP".to_string());

    let properties = property_map_to_json(&rel_pattern.properties);

    let mut create_edge = |from: u64, to: u64| match engine.create_relationship(
        from,
        to,
        rel_type.clone(),
        properties.clone(),
    ) {
        Ok(_rel_id) => Ok(()),
        Err(e) => Err(format!("Failed to create relationship: {}", e)),
    };

    match rel_pattern.direction {
        nexus_core::executor::parser::RelationshipDirection::Outgoing => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(from, to)?;
                }
            }
        }
        nexus_core::executor::parser::RelationshipDirection::Incoming => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(to, from)?;
                }
            }
        }
        nexus_core::executor::parser::RelationshipDirection::Both => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(from, to)?;
                    create_edge(to, from)?;
                }
            }
        }
    }

    Ok(())
}

/// Cypher query request
#[derive(Debug, Deserialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
    /// Database name (optional, defaults to "neo4j")
    #[serde(default)]
    pub database: Option<String>,
}

/// Cypher query response
#[derive(Debug, Serialize)]
pub struct CypherResponse {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<serde_json::Value>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execute Cypher query
pub async fn execute_cypher(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<CypherRequest>,
) -> Json<CypherResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!("Executing Cypher query: {}", request.query);

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

    if has_user_cmd {
        return execute_user_commands(server, &ast, start_time).await;
    }

    // Check if this is a CREATE, MERGE, SET, DELETE, REMOVE, or MATCH query
    let query_upper = request.query.trim().to_uppercase();
    let is_create_query = query_upper.starts_with("CREATE");
    let is_merge_query = query_upper.starts_with("MERGE");
    let _is_set_query = query_upper.starts_with("SET");
    let _is_delete_query = query_upper.starts_with("DELETE");
    let _is_remove_query = query_upper.starts_with("REMOVE");
    let is_match_query = query_upper.starts_with("MATCH");

    if is_create_query || is_merge_query {
        // Use Engine for CREATE operations
        if let Some(engine) = ENGINE.get() {
            // Execute all clauses sequentially using Engine
            let mut engine = engine.write().await;

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
                                eprintln!(
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
                                            } else {
                                                tracing::info!(
                                                    "SET {}.{} on node {}",
                                                    target,
                                                    property,
                                                    node_id
                                                );
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
                                            } else {
                                                tracing::info!(
                                                    "SET {}:{} on node {}",
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
                                                let mut tx = engine
                                                    .transaction_manager
                                                    .begin_write()
                                                    .unwrap();
                                                let mut deleted_record = rel_record;
                                                deleted_record.mark_deleted();
                                                engine
                                                    .storage
                                                    .write_rel(rel_id, &deleted_record)
                                                    .unwrap();
                                                engine.transaction_manager.commit(&mut tx).unwrap();
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
                                        } else {
                                            tracing::warn!(
                                                "Node {} not found for deletion",
                                                node_id
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to delete node {}: {}", node_id, e);
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
        if let Some(engine) = ENGINE.get() {
            // Use the engine's execute_cypher method which uses its internal executor
            let mut engine_guard = engine.write().await;
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
    let executor_guard = match EXECUTOR.get() {
        Some(executor) => executor,
        None => {
            tracing::error!("Executor not initialized");
            return Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: start_time.elapsed().as_millis() as u64,
                error: Some("Executor not initialized".to_string()),
            });
        }
    };

    // Create query
    let query = Query {
        cypher: request.query.clone(),
        params: request.params,
    };

    // Execute query
    let mut executor = executor_guard.write().await;
    match executor.execute(&query) {
        Ok(result_set) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::info!(
                "Query executed successfully in {}ms, {} rows returned",
                execution_time,
                result_set.rows.len()
            );

            Json(CypherResponse {
                columns: result_set.columns,
                rows: result_set
                    .rows
                    .into_iter()
                    .map(|row| serde_json::Value::Array(row.values))
                    .collect(),
                execution_time_ms: execution_time,
                error: None,
            })
        }
        Err(e) => {
            let execution_time = start_time.elapsed().as_millis() as u64;

            tracing::error!("Query execution failed: {}", e);

            Json(CypherResponse {
                columns: vec![],
                rows: vec![],
                execution_time_ms: execution_time,
                error: Some(e.to_string()),
            })
        }
    }
}

/// Execute database management commands (CREATE DATABASE, DROP DATABASE, SHOW DATABASES)
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_database_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::ShowDatabases => {
                columns = vec!["name".to_string(), "default".to_string()];
                let manager = server.database_manager.read().await;
                let databases = manager.list_databases();
                let default_db = manager.default_database_name();

                for db in databases {
                    rows.push(serde_json::json!([db.name, db.name == default_db]));
                }
            }
            nexus_core::executor::parser::Clause::CreateDatabase(create_db) => {
                columns = vec!["name".to_string(), "message".to_string()];
                let manager = server.database_manager.write().await;

                match manager.create_database(&create_db.name) {
                    Ok(_) => {
                        rows.push(serde_json::json!([
                            create_db.name.clone(),
                            format!("Database '{}' created successfully", create_db.name)
                        ]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to create database: {}", e)),
                        });
                    }
                }
            }
            nexus_core::executor::parser::Clause::DropDatabase(drop_db) => {
                columns = vec!["message".to_string()];
                let manager = server.database_manager.write().await;

                match manager.drop_database(&drop_db.name) {
                    Ok(_) => {
                        rows.push(serde_json::json!([format!(
                            "Database '{}' dropped successfully",
                            drop_db.name
                        )]));
                    }
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to drop database: {}", e)),
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}

/// Execute user management commands (SHOW USERS, CREATE USER, GRANT, REVOKE)
#[cfg_attr(test, allow(dead_code))]
pub(crate) async fn execute_user_commands(
    server: Arc<NexusServer>,
    ast: &nexus_core::executor::parser::CypherQuery,
    start_time: std::time::Instant,
) -> Json<CypherResponse> {
    let mut columns = Vec::new();
    let mut rows = Vec::new();
    let mut rbac = server.rbac.write().await;

    for clause in &ast.clauses {
        match clause {
            nexus_core::executor::parser::Clause::ShowUsers => {
                columns = vec![
                    "username".to_string(),
                    "roles".to_string(),
                    "is_active".to_string(),
                ];
                let users = rbac.list_users();

                for user in users {
                    rows.push(serde_json::json!([
                        user.username.clone(),
                        user.roles.clone(),
                        user.is_active
                    ]));
                }
            }
            nexus_core::executor::parser::Clause::ShowUser(show_user) => {
                columns = vec![
                    "username".to_string(),
                    "id".to_string(),
                    "email".to_string(),
                    "roles".to_string(),
                    "permissions".to_string(),
                    "is_active".to_string(),
                    "is_root".to_string(),
                ];

                let users_list = rbac.list_users();
                let user = users_list.iter().find(|u| u.username == show_user.username);

                if let Some(user) = user {
                    let permissions: Vec<String> = user
                        .additional_permissions
                        .permissions()
                        .iter()
                        .map(|p| p.to_string())
                        .collect();

                    rows.push(serde_json::json!([
                        user.username.clone(),
                        user.id.clone(),
                        user.email.clone().unwrap_or_default(),
                        user.roles.clone(),
                        permissions,
                        user.is_active,
                        user.is_root
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' not found", show_user.username)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::DropUser(drop_user) => {
                columns = vec!["username".to_string(), "message".to_string()];

                let users_list = rbac.list_users();
                let user_info = users_list
                    .iter()
                    .find(|u| u.username == drop_user.username)
                    .map(|u| (u.id.clone(), u.is_root));

                if let Some((user_id, is_root)) = user_info {
                    // Prevent deletion of root user
                    if is_root {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(
                                "Cannot delete root user. Use DISABLE instead.".to_string(),
                            ),
                        });
                    }

                    if let Some(_removed_user) = rbac.remove_user(&user_id) {
                        rows.push(serde_json::json!([
                            drop_user.username.clone(),
                            format!("User '{}' deleted successfully", drop_user.username)
                        ]));
                    } else {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(format!("Failed to delete user '{}'", drop_user.username)),
                        });
                    }
                } else if drop_user.if_exists {
                    rows.push(serde_json::json!([
                        drop_user.username.clone(),
                        format!("User '{}' does not exist (IF EXISTS)", drop_user.username)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' not found", drop_user.username)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::CreateUser(create_user) => {
                columns = vec!["username".to_string(), "message".to_string()];

                // Check if user already exists (by username)
                let users_list = rbac.list_users();
                let existing_user = users_list
                    .iter()
                    .find(|u| u.username == create_user.username);

                if existing_user.is_some() && !create_user.if_not_exists {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User '{}' already exists", create_user.username)),
                    });
                }

                if existing_user.is_none() {
                    let user_id = uuid::Uuid::new_v4().to_string();
                    let user = if let Some(password) = &create_user.password {
                        // Hash password with Argon2
                        use argon2::password_hash::{SaltString, rand_core::OsRng};
                        use argon2::{Argon2, PasswordHasher};

                        let argon2 = Argon2::default();
                        let salt = SaltString::generate(&mut OsRng);
                        match argon2.hash_password(password.as_bytes(), &salt) {
                            Ok(password_hash) => nexus_core::auth::User::with_password_hash(
                                user_id.clone(),
                                create_user.username.clone(),
                                password_hash.to_string(),
                            ),
                            Err(e) => {
                                let execution_time = start_time.elapsed().as_millis() as u64;
                                return Json(CypherResponse {
                                    columns: vec![],
                                    rows: vec![],
                                    execution_time_ms: execution_time,
                                    error: Some(format!("Failed to hash password: {}", e)),
                                });
                            }
                        }
                    } else {
                        nexus_core::auth::User::new(user_id.clone(), create_user.username.clone())
                    };
                    rbac.add_user(user);
                }

                rows.push(serde_json::json!([
                    create_user.username.clone(),
                    format!("User '{}' created successfully", create_user.username)
                ]));
            }
            nexus_core::executor::parser::Clause::Grant(grant) => {
                columns = vec![
                    "target".to_string(),
                    "permissions".to_string(),
                    "message".to_string(),
                ];

                // Parse permissions
                let permissions: Result<Vec<Permission>, _> = grant
                    .permissions
                    .iter()
                    .map(|p| match p.to_uppercase().as_str() {
                        "READ" => Ok(Permission::Read),
                        "WRITE" => Ok(Permission::Write),
                        "ADMIN" => Ok(Permission::Admin),
                        "SUPER" => Ok(Permission::Super),
                        _ => Err(format!("Unknown permission: {}", p)),
                    })
                    .collect();

                let permissions = match permissions {
                    Ok(p) => p,
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(e),
                        });
                    }
                };

                // Check if target is a user (by username or id) or role
                let users_list = rbac.list_users();
                let user_id = users_list
                    .iter()
                    .find(|u| u.username == grant.target || u.id == grant.target)
                    .map(|u| u.id.clone());

                if let Some(user_id) = user_id {
                    // Grant to user
                    if let Some(user_mut) = rbac.get_user_mut(&user_id) {
                        for perm in &permissions {
                            user_mut.add_permission(perm.clone());
                        }
                    }
                    rows.push(serde_json::json!([
                        grant.target.clone(),
                        grant.permissions.clone(),
                        format!("Granted permissions to user '{}'", grant.target)
                    ]));
                } else if let Some(role) = rbac.get_role_mut(&grant.target) {
                    // Grant to role
                    for perm in &permissions {
                        role.add_permission(perm.clone());
                    }
                    rows.push(serde_json::json!([
                        grant.target.clone(),
                        grant.permissions.clone(),
                        format!("Granted permissions to role '{}'", grant.target)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User or role '{}' not found", grant.target)),
                    });
                }
            }
            nexus_core::executor::parser::Clause::Revoke(revoke) => {
                columns = vec![
                    "target".to_string(),
                    "permissions".to_string(),
                    "message".to_string(),
                ];

                // Parse permissions
                let permissions: Result<Vec<Permission>, _> = revoke
                    .permissions
                    .iter()
                    .map(|p| match p.to_uppercase().as_str() {
                        "READ" => Ok(Permission::Read),
                        "WRITE" => Ok(Permission::Write),
                        "ADMIN" => Ok(Permission::Admin),
                        "SUPER" => Ok(Permission::Super),
                        _ => Err(format!("Unknown permission: {}", p)),
                    })
                    .collect();

                let permissions = match permissions {
                    Ok(p) => p,
                    Err(e) => {
                        let execution_time = start_time.elapsed().as_millis() as u64;
                        return Json(CypherResponse {
                            columns: vec![],
                            rows: vec![],
                            execution_time_ms: execution_time,
                            error: Some(e),
                        });
                    }
                };

                // Check if target is a user (by username or id) or role
                let users_list = rbac.list_users();
                let user_id = users_list
                    .iter()
                    .find(|u| u.username == revoke.target || u.id == revoke.target)
                    .map(|u| u.id.clone());

                if let Some(user_id) = user_id {
                    // Revoke from user
                    if let Some(user_mut) = rbac.get_user_mut(&user_id) {
                        for perm in &permissions {
                            user_mut.remove_permission(perm);
                        }
                    }
                    rows.push(serde_json::json!([
                        revoke.target.clone(),
                        revoke.permissions.clone(),
                        format!("Revoked permissions from user '{}'", revoke.target)
                    ]));
                } else if let Some(role) = rbac.get_role_mut(&revoke.target) {
                    // Revoke from role
                    for perm in &permissions {
                        role.remove_permission(perm);
                    }
                    rows.push(serde_json::json!([
                        revoke.target.clone(),
                        revoke.permissions.clone(),
                        format!("Revoked permissions from role '{}'", revoke.target)
                    ]));
                } else {
                    let execution_time = start_time.elapsed().as_millis() as u64;
                    return Json(CypherResponse {
                        columns: vec![],
                        rows: vec![],
                        execution_time_ms: execution_time,
                        error: Some(format!("User or role '{}' not found", revoke.target)),
                    });
                }
            }
            _ => {}
        }
    }

    let execution_time = start_time.elapsed().as_millis() as u64;
    Json(CypherResponse {
        columns,
        rows,
        execution_time_ms: execution_time,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    // Note: These tests need to be updated to use State<Arc<NexusServer>>
    // They are temporarily disabled until we can properly set up the test server
    /*
    #[tokio::test]
    async fn test_execute_simple_query() {
        use crate::NexusServer;
        use nexus_core::database::DatabaseManager;
        use nexus_core::auth::RoleBasedAccessControl;
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));
        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));
        let database_manager = DatabaseManager::new(temp_dir.path().join("databases")).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));
        let server = Arc::new(NexusServer::new(executor_arc, engine_arc, database_manager_arc, rbac_arc));

        let request = CypherRequest {
            query: "MATCH (n) RETURN n LIMIT 1".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(axum::extract::State(server), Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_query_with_params() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), json!(5));

        let request = CypherRequest {
            query: "MATCH (n) RETURN n LIMIT $limit".to_string(),
            params,
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_invalid_query() {
        let request = CypherRequest {
            query: "INVALID SYNTAX".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Should handle invalid syntax gracefully
    }

    #[tokio::test]
    async fn test_execute_without_executor() {
        // Don't initialize executor
        let request = CypherRequest {
            query: "MATCH (n) RETURN n".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let response = execute_cypher(Json(request)).await;
        assert!(response.error.is_some());
        assert_eq!(response.error.as_ref().unwrap(), "Executor not initialized");
    }

    #[tokio::test]
    async fn test_response_format() {
        let request = CypherRequest {
            query: "RETURN 1 as num, 'test' as str".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_initialized_executor() {
        let request = CypherRequest {
            query: "RETURN 'hello' as greeting".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs - executor may or may not be initialized
    }

    #[tokio::test]
    async fn test_execute_with_complex_params() {
        let mut params = HashMap::new();
        params.insert("name".to_string(), json!("Alice"));
        params.insert("age".to_string(), json!(30));
        params.insert("active".to_string(), json!(true));

        let request = CypherRequest {
            query: "RETURN $name as name, $age as age, $active as active".to_string(),
            params,
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_empty_result() {
        let request = CypherRequest {
            query: "MATCH (n) WHERE n.nonexistent = 'value' RETURN n".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_multiple_rows() {
        let request = CypherRequest {
            query: "UNWIND [1, 2, 3] AS num RETURN num".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_nested_params() {
        let mut params = HashMap::new();
        params.insert("list".to_string(), json!([1, 2, 3]));
        params.insert("obj".to_string(), json!({"key": "value"}));

        let request = CypherRequest {
            query: "RETURN $list as numbers, $obj as data".to_string(),
            params,
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_null_params() {
        let mut params = HashMap::new();
        params.insert("null_value".to_string(), json!(null));

        let request = CypherRequest {
            query: "RETURN $null_value as null_val".to_string(),
            params,
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_empty_query() {
        let request = CypherRequest {
            query: "".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Should handle empty query gracefully
    }

    #[tokio::test]
    async fn test_execute_with_very_long_query() {
        let long_query = "RETURN ".to_string() + &"x".repeat(1000);
        let request = CypherRequest {
            query: long_query,
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Should handle long query gracefully
    }

    #[tokio::test]
    async fn test_merge_node() {
        let request = CypherRequest {
            query: "MERGE (n:Person {name: \"Alice\", age: 30})".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_merge_node_without_properties() {
        let request = CypherRequest {
            query: "MERGE (n:Person)".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_set_property() {
        let request = CypherRequest {
            query: "CREATE (n:Person {name: \"Alice\"}) SET n.age = 30".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_set_label() {
        let request = CypherRequest {
            query: "CREATE (n:Person) SET n:Employee".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_delete_node() {
        let request = CypherRequest {
            query: "CREATE (n:Person {name: \"Bob\"}) DELETE n".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_detach_delete() {
        let request = CypherRequest {
            query: "CREATE (n:Person {name: \"Charlie\"}) DETACH DELETE n".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs (DETACH DELETE partially supported)
    }

    #[tokio::test]
    async fn test_remove_property() {
        let request = CypherRequest {
            query: "CREATE (n:Person {name: \"David\", age: 25}) REMOVE n.age".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_remove_label() {
        let request = CypherRequest {
            query: "CREATE (n:Person:Employee) REMOVE n:Employee".to_string(),
            params: HashMap::new(),
            database: None,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }
    */
}
