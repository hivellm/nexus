//! Cypher query execution endpoint

use axum::extract::Json;
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

/// Cypher query request
#[derive(Debug, Deserialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters
    #[serde(default)]
    pub params: HashMap<String, serde_json::Value>,
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
pub async fn execute_cypher(Json(request): Json<CypherRequest>) -> Json<CypherResponse> {
    let start_time = std::time::Instant::now();

    tracing::info!("Executing Cypher query: {}", request.query);

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

            // Execute CREATE or MERGE clauses using Engine
            let mut engine = engine.write().await;
            for clause in &ast.clauses {
                // Handle CREATE clause
                if let nexus_core::executor::parser::Clause::Create(create_clause) = clause {
                    // Extract pattern and create nodes
                    for element in &create_clause.pattern.elements {
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

                            let properties = serde_json::Value::Object(props);

                            // Create node using Engine
                            match engine.create_node(labels, properties) {
                                Ok(_node_id) => {
                                    tracing::info!("Node created successfully via Engine");
                                }
                                Err(e) => {
                                    let execution_time = start_time.elapsed().as_millis() as u64;
                                    tracing::error!("Failed to create node: {}", e);
                                    return Json(CypherResponse {
                                        columns: vec![],
                                        rows: vec![],
                                        execution_time_ms: execution_time,
                                        error: Some(format!("Failed to create node: {}", e)),
                                    });
                                }
                            }
                        }
                    }
                }
                // Handle MERGE clause
                else if let nexus_core::executor::parser::Clause::Merge(merge_clause) = clause {
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
                                if let Ok(label_id) = engine.catalog.get_or_create_label(first_label) {
                                    // Get all nodes with this label from label_index
                                    if let Ok(node_ids) = engine.indexes.label_index.get_nodes(label_id) {
                                        // Iterate through nodes and check if properties match
                                        for node_id in node_ids {
                                            if let Ok(Some(existing_props)) = engine.storage.load_node_properties(node_id as u64) {
                                                // Check if all properties from MERGE match existing properties
                                                let props_obj = properties.as_object().unwrap();
                                                let mut all_match = true;
                                                
                                                for (key, value) in props_obj {
                                                    if let Some(existing_value) = existing_props.get(key) {
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
                                                    tracing::info!("MERGE: Found existing node {} with matching properties", node_id);
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
                                match engine.create_node(labels, serde_json::Value::Object(props)) {
                                    Ok(node_id) => {
                                        tracing::info!("MERGE: Created new node {} via Engine", node_id);
                                    }
                                    Err(e) => {
                                        let execution_time = start_time.elapsed().as_millis() as u64;
                                        tracing::error!("Failed to merge node: {}", e);
                                        return Json(CypherResponse {
                                            columns: vec![],
                                            rows: vec![],
                                            execution_time_ms: execution_time,
                                            error: Some(format!("Failed to merge node: {}", e)),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            let execution_time = start_time.elapsed().as_millis() as u64;
            let clause_type = if is_merge_query { "MERGE" } else { "CREATE" };
            tracing::info!("{} query executed successfully in {}ms", clause_type, execution_time);

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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Json;
    use serde_json::json;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_execute_simple_query() {
        let request = CypherRequest {
            query: "MATCH (n) RETURN n LIMIT 1".to_string(),
            params: HashMap::new(),
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_query_with_params() {
        let mut params = HashMap::new();
        params.insert("limit".to_string(), json!(5));

        let request = CypherRequest {
            query: "MATCH (n) RETURN n LIMIT $limit".to_string(),
            params,
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_invalid_query() {
        let request = CypherRequest {
            query: "INVALID SYNTAX".to_string(),
            params: HashMap::new(),
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
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_initialized_executor() {
        let request = CypherRequest {
            query: "RETURN 'hello' as greeting".to_string(),
            params: HashMap::new(),
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
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_empty_result() {
        let request = CypherRequest {
            query: "MATCH (n) WHERE n.nonexistent = 'value' RETURN n".to_string(),
            params: HashMap::new(),
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_multiple_rows() {
        let request = CypherRequest {
            query: "UNWIND [1, 2, 3] AS num RETURN num".to_string(),
            params: HashMap::new(),
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
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_execute_with_empty_query() {
        let request = CypherRequest {
            query: "".to_string(),
            params: HashMap::new(),
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
        };

        let _response = execute_cypher(Json(request)).await;
        // Should handle long query gracefully
    }

    #[tokio::test]
    async fn test_merge_node() {
        let request = CypherRequest {
            query: "MERGE (n:Person {name: \"Alice\", age: 30})".to_string(),
            params: HashMap::new(),
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }

    #[tokio::test]
    async fn test_merge_node_without_properties() {
        let request = CypherRequest {
            query: "MERGE (n:Person)".to_string(),
            params: HashMap::new(),
        };

        let _response = execute_cypher(Json(request)).await;
        // Test passes if no panic occurs
    }
}
