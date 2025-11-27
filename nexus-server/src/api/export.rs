//! Data export endpoint

use crate::NexusServer;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

/// Export request parameters
#[derive(Debug, Deserialize)]
pub struct ExportRequest {
    /// Export format: "json" or "csv" (default: "json")
    #[serde(default = "default_format")]
    pub format: String,
    /// Cypher query to select data to export (default: "MATCH (n) RETURN n")
    #[serde(default = "default_query")]
    pub query: String,
    /// Whether to stream the response (default: false)
    #[serde(default)]
    pub stream: bool,
}

fn default_format() -> String {
    "json".to_string()
}

fn default_query() -> String {
    "MATCH (n) RETURN n".to_string()
}

/// Export response (for non-streaming)
#[derive(Debug, Serialize)]
pub struct ExportResponse {
    /// Number of records exported
    pub records_exported: usize,
    /// Export format used
    pub format: String,
    /// Export data (for JSON format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// Export data from the database
pub async fn export_data(
    State(server): State<std::sync::Arc<NexusServer>>,
    Query(params): Query<ExportRequest>,
) -> Result<Response, (StatusCode, String)> {
    tracing::info!(
        "Export request: format={}, query={}, stream={}",
        params.format,
        params.query,
        params.stream
    );

    // Execute query to get data
    let mut engine = server.engine.write().await;
    let result = engine.execute_cypher(&params.query).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Query execution failed: {}", e),
        )
    })?;
    drop(engine);

    match params.format.to_lowercase().as_str() {
        "json" => export_json(result, params.stream).await,
        "csv" => export_csv(result, params.stream).await,
        _ => Err((
            StatusCode::BAD_REQUEST,
            format!(
                "Unsupported format: {}. Supported formats: json, csv",
                params.format
            ),
        )),
    }
}

/// Export data as JSON
async fn export_json(
    result: nexus_core::executor::ResultSet,
    _stream: bool,
) -> Result<Response, (StatusCode, String)> {
    if _stream {
        // Streaming JSON export
        let mut json_data = Vec::new();
        json_data.push(b'[');

        for (i, row) in result.rows.iter().enumerate() {
            if i > 0 {
                json_data.extend_from_slice(b",");
            }

            let row_json = serde_json::to_vec(&row.values).map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("JSON serialization failed: {}", e),
                )
            })?;
            json_data.extend_from_slice(&row_json);
        }

        json_data.push(b']');

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header(
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"export.json\"",
            )
            .body(Body::from(json_data))
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to build response: {}", e),
                )
            })?
            .into_response())
    } else {
        // Non-streaming JSON export
        let data: Vec<serde_json::Value> = result
            .rows
            .iter()
            .map(|row| {
                if row.values.len() == 1 {
                    row.values[0].clone()
                } else {
                    serde_json::Value::Array(row.values.clone())
                }
            })
            .collect();

        let response = ExportResponse {
            records_exported: data.len(),
            format: "json".to_string(),
            data: Some(serde_json::Value::Array(data)),
        };

        Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_string(&response).map_err(
                |e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("JSON serialization failed: {}", e),
                    )
                },
            )?))
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to build response: {}", e),
                )
            })?
            .into_response())
    }
}

/// Export data as CSV
async fn export_csv(
    result: nexus_core::executor::ResultSet,
    _stream: bool,
) -> Result<Response, (StatusCode, String)> {
    let mut csv_data = Vec::new();

    // Write header
    let header = result.columns.join(",");
    csv_data.extend_from_slice(header.as_bytes());
    csv_data.push(b'\n');

    // Write rows
    for row in &result.rows {
        let csv_row: Vec<String> = row
            .values
            .iter()
            .map(|v| {
                match v {
                    serde_json::Value::String(s) => {
                        // Escape quotes and wrap in quotes if contains comma or quote
                        if s.contains(',') || s.contains('"') || s.contains('\n') {
                            format!("\"{}\"", s.replace('"', "\"\""))
                        } else {
                            s.clone()
                        }
                    }
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::Bool(b) => b.to_string(),
                    serde_json::Value::Null => String::new(),
                    _ => serde_json::to_string(v).unwrap_or_else(|_| String::new()),
                }
            })
            .collect();
        csv_data.extend_from_slice(csv_row.join(",").as_bytes());
        csv_data.push(b'\n');
    }

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=\"export.csv\"",
        )
        .body(Body::from(csv_data))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to build response: {}", e),
            )
        })?
        .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RootUserConfig;
    use axum::extract::{Query, State};
    use nexus_core::{
        Engine,
        auth::{
            AuditConfig, AuditLogger, AuthConfig, AuthManager, JwtConfig, JwtManager,
            RoleBasedAccessControl,
        },
        database::DatabaseManager,
        executor::Executor,
    };
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    /// Helper function to create a test server
    /// Returns server and temp_dir (to keep temp_dir alive)
    async fn create_test_server() -> (Arc<NexusServer>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let engine = Engine::with_data_dir(temp_dir.path()).unwrap();

        // Create executor using Engine's components
        // The Engine already has executor, but we need a separate one for server state
        // We'll create a new executor with the same components
        let executor = Executor::new(
            &engine.catalog,
            &engine.storage,
            &engine.indexes.label_index,
            &engine.indexes.knn_index,
        )
        .unwrap();
        let executor_arc = Arc::new(executor);

        let engine_arc = Arc::new(RwLock::new(engine));

        let database_manager = DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));

        let rbac = RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(auth_config));

        let jwt_config = JwtConfig::default();
        let jwt_manager = Arc::new(JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            AuditLogger::new(AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        (
            Arc::new(NexusServer::new(
                executor_arc,
                engine_arc,
                database_manager_arc,
                rbac_arc,
                auth_manager,
                jwt_manager,
                audit_logger,
                RootUserConfig::default(),
            )),
            temp_dir,
        )
    }

    #[test]
    fn test_default_format() {
        assert_eq!(default_format(), "json");
    }

    #[test]
    fn test_default_query() {
        assert_eq!(default_query(), "MATCH (n) RETURN n");
    }

    #[tokio::test]
    async fn test_export_json_empty() {
        let (server, _temp_dir) = create_test_server().await;
        let params = ExportRequest {
            format: "json".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_export_json_with_data() {
        let (server, _temp_dir) = create_test_server().await;

        // Create some test data
        let mut engine = server.engine.write().await;
        engine
            .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30}) RETURN n")
            .unwrap();
        engine
            .execute_cypher("CREATE (n:Person {name: 'Bob', age: 25}) RETURN n")
            .unwrap();
        drop(engine);

        let params = ExportRequest {
            format: "json".to_string(),
            query: "MATCH (n:Person) RETURN n.name as name, n.age as age ORDER BY n.age"
                .to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    #[ignore] // TODO: Fix temp dir race condition
    async fn test_export_csv_empty() {
        use crate::api::graph_correlation_mcp_tests::TestServer;
        let test_server = TestServer::new();
        let server = test_server.server();
        let params = ExportRequest {
            format: "csv".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_export_csv_with_data() {
        use crate::api::graph_correlation_mcp_tests::TestServer;
        let test_server = TestServer::new();
        let server = test_server.server();

        // Create some test data
        let mut engine = server.engine.write().await;
        engine
            .execute_cypher("CREATE (n:Person {name: 'Alice', age: 30}) RETURN n")
            .unwrap();
        engine
            .execute_cypher("CREATE (n:Person {name: 'Bob', age: 25}) RETURN n")
            .unwrap();
        drop(engine);

        let params = ExportRequest {
            format: "csv".to_string(),
            query: "MATCH (n:Person) RETURN n.name as name, n.age as age ORDER BY n.age"
                .to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_export_invalid_format() {
        let (server, _temp_dir) = create_test_server().await;
        let params = ExportRequest {
            format: "xml".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("Unsupported format"));
    }

    #[tokio::test]
    async fn test_export_invalid_query() {
        let (server, _temp_dir) = create_test_server().await;
        let params = ExportRequest {
            format: "json".to_string(),
            query: "INVALID QUERY SYNTAX".to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_export_default_format() {
        let (server, _temp_dir) = create_test_server().await;
        let params = ExportRequest {
            format: default_format(),
            query: default_query(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_export_custom_query() {
        let (server, _temp_dir) = create_test_server().await;

        // Create test data
        let mut engine = server.engine.write().await;
        engine
            .execute_cypher("CREATE (n:Company {name: 'TechCorp'}) RETURN n")
            .unwrap();
        drop(engine);

        let params = ExportRequest {
            format: "json".to_string(),
            query: "MATCH (n:Company) RETURN n.name as company_name".to_string(),
            stream: false,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_export_streaming_flag() {
        let (server, _temp_dir) = create_test_server().await;
        let params = ExportRequest {
            format: "json".to_string(),
            query: "MATCH (n) RETURN n".to_string(),
            stream: true,
        };

        let result = export_data(State(server), Query(params)).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
}
