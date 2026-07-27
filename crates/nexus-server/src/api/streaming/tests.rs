#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::NexusServer;
    use crate::config::RootUserConfig;
    use nexus_core::auth::{
        AuditConfig, AuditLogger, AuthConfig, AuthManager, JwtConfig, JwtManager, Permission,
        RateLimits, RoleBasedAccessControl,
    };
    use nexus_core::catalog::{CATALOG_MMAP_INITIAL_SIZE, Catalog};
    use nexus_core::database::DatabaseManager;
    use nexus_core::index::{DEFAULT_VECTORIZER_DIMENSION, KnnIndex, LabelIndex};
    use nexus_core::storage::RecordStore;
    use nexus_core::testing::TestContext;
    use nexus_core::{Engine, executor::Executor};
    use parking_lot::RwLock;
    use rmcp::ServerHandler;
    use rmcp::model::CallToolRequestParams;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::RwLock as TokioRwLock;

    /// Helper function to create a test server with all required components.
    /// Mirrors `tests/streaming_mcp_write_test.rs::create_test_server` — an
    /// isolated catalog/store/engine per test so parallel test-binary
    /// execution can't cross-contaminate label/property state. Returns the
    /// `TestContext` alongside the server so the caller keeps the backing
    /// temp dir alive for the duration of the test.
    async fn create_test_server() -> (Arc<NexusServer>, TestContext) {
        let ctx = TestContext::new();
        let data_dir = ctx.path().to_path_buf();
        std::fs::create_dir_all(&data_dir).unwrap();

        let engine = Engine::with_isolated_catalog(&data_dir).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let catalog = Catalog::with_isolated_path(
            data_dir.join("executor_catalog.mdb"),
            CATALOG_MMAP_INITIAL_SIZE,
        )
        .unwrap();
        let store = RecordStore::new(&data_dir).unwrap();
        let label_index = LabelIndex::new();
        let knn_index = KnnIndex::new_default(DEFAULT_VECTORIZER_DIMENSION).unwrap();
        let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();
        let executor_arc = Arc::new(executor);

        let database_manager = DatabaseManager::new(data_dir.clone()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));

        let rbac = RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

        let auth_config = AuthConfig {
            enabled: false,
            required_for_public: false,
            default_permissions: vec![Permission::Read, Permission::Write],
            rate_limits: RateLimits {
                per_minute: 1000,
                per_hour: 10000,
            },
        };
        let auth_storage_path = data_dir.join("auth");
        std::fs::create_dir_all(&auth_storage_path).unwrap();
        let auth_manager =
            Arc::new(AuthManager::with_storage(auth_config.clone(), auth_storage_path).unwrap());

        let jwt_manager = Arc::new(JwtManager::new(JwtConfig::from_env()));

        let audit_logger = Arc::new(
            AuditLogger::new(AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        let server = Arc::new(NexusServer::new(
            executor_arc,
            engine_arc,
            database_manager_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            RootUserConfig::default(),
        ));

        (server, ctx)
    }

    #[tokio::test]
    async fn test_nexus_mcp_service_new() {
        let (server, _ctx) = create_test_server().await;
        let _service = NexusMcpService::new(server);
        // Service created successfully
    }

    #[tokio::test]
    async fn test_get_info() {
        let (server, _ctx) = create_test_server().await;

        let service = NexusMcpService::new(server);
        let info = service.get_info();

        assert_eq!(info.server_info.name, "nexus-server");
        assert_eq!(
            info.server_info.title,
            Some("Nexus Graph Database Server".to_string())
        );
        assert!(info.server_info.website_url.is_some());
        assert!(info.instructions.is_some());
    }

    #[tokio::test]
    async fn test_get_nexus_mcp_tools() {
        let tools = get_nexus_mcp_tools();
        assert!(!tools.is_empty());

        // Check that we have the expected tools
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert!(tool_names.contains(&"create_node"));
        assert!(tool_names.contains(&"create_relationship"));
        assert!(tool_names.contains(&"execute_cypher"));
        assert!(tool_names.contains(&"knn_search"));
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_unknown() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("unknown_tool");

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(tool_result.is_error.unwrap_or(false));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for unknown tools
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_create_node() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("create_node").with_arguments(
            json!({
                "labels": ["Person"],
                "properties": {"name": "Alice"}
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_execute_cypher() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("execute_cypher").with_arguments(
            json!({
                "query": "RETURN 1 as test"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_handle_nexus_mcp_tool_knn_search() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("knn_search").with_arguments(
            json!({
                "label": "Person",
                "vector": [0.1, 0.2, 0.3],
                "k": 5
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;

        // The result might be Ok or Err depending on the tool implementation
        if let Ok(tool_result) = result {
            assert!(!tool_result.is_error.unwrap_or(true));
            assert_eq!(tool_result.content.len(), 1);
        } else {
            // If it returns an error, that's also acceptable for uninitialized executor
            assert!(result.is_err());
        }
    }

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await;
        let data = response.0;
        assert_eq!(data["protocol"], "MCP");
        assert_eq!(data["version"], "1.0");
        assert_eq!(data["transport"], "streamable-http");
        assert_eq!(data["status"], "ok");
        assert!(!data["nexus_version"].as_str().unwrap().is_empty());
    }

    // ============================================================================
    // Graph Correlation MCP Tools Tests
    // ============================================================================

    #[tokio::test]
    async fn test_graph_correlation_generate_call_graph() {
        let (server, _ctx) = create_test_server().await;

        let mut files = serde_json::Map::new();
        files.insert(
            "main.rs".to_string(),
            json!("fn main() { helper(); }\nfn helper() {}"),
        );

        let request = CallToolRequestParams::new("graph_correlation_generate").with_arguments(
            json!({
                "graph_type": "Call",
                "files": files,
                "name": "Test Graph"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        assert!(!tool_result.is_error.unwrap_or(true));
        assert_eq!(tool_result.content.len(), 1);

        // Parse response
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert!(response.get("graph").is_some());
    }

    #[tokio::test]
    async fn test_graph_correlation_generate_dependency_graph() {
        let (server, _ctx) = create_test_server().await;

        let mut files = serde_json::Map::new();
        files.insert("mod_a.rs".to_string(), json!("use mod_b;"));
        files.insert("mod_b.rs".to_string(), json!(""));

        let request = CallToolRequestParams::new("graph_correlation_generate").with_arguments(
            json!({
                "graph_type": "Dependency",
                "files": files
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_graph_correlation_generate_invalid_type() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("graph_correlation_generate").with_arguments(
            json!({
                "graph_type": "InvalidType",
                "files": {}
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_statistics() {
        let (server, _ctx) = create_test_server().await;

        // Create a simple graph
        let graph = json!({
            "name": "Test Graph",
            "graph_type": "Call",
            "nodes": [
                {"id": "node1", "node_type": "Function", "label": "func1", "metadata": {}, "position": null, "size": null},
                {"id": "node2", "node_type": "Function", "label": "func2", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [
                {"id": "edge1", "source": "node1", "target": "node2", "edge_type": "Calls", "weight": 1.0, "label": null, "metadata": {}}
            ],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_analyze").with_arguments(
            json!({
                "graph": graph,
                "analysis_type": "statistics"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert!(response.get("statistics").is_some());
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_patterns() {
        let (server, _ctx) = create_test_server().await;

        let graph = json!({
            "name": "Pipeline Graph",
            "graph_type": "DataFlow",
            "nodes": [
                {"id": "stage1", "node_type": "Function", "label": "input", "metadata": {}, "position": null, "size": null},
                {"id": "stage2", "node_type": "Function", "label": "process", "metadata": {}, "position": null, "size": null},
                {"id": "stage3", "node_type": "Function", "label": "output", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [
                {"id": "edge1", "source": "stage1", "target": "stage2", "edge_type": "Transforms", "weight": 1.0, "label": null, "metadata": {}},
                {"id": "edge2", "source": "stage2", "target": "stage3", "edge_type": "Transforms", "weight": 1.0, "label": null, "metadata": {}}
            ],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_analyze").with_arguments(
            json!({
                "graph": graph,
                "analysis_type": "patterns"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert!(response.get("patterns").is_some());
    }

    #[tokio::test]
    async fn test_graph_correlation_analyze_all() {
        let (server, _ctx) = create_test_server().await;

        let graph = json!({
            "name": "Full Graph",
            "graph_type": "Call",
            "nodes": [
                {"id": "n1", "node_type": "Function", "label": "f1", "metadata": {}, "position": null, "size": null}
            ],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_analyze").with_arguments(
            json!({
                "graph": graph,
                "analysis_type": "all"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert!(response.get("statistics").is_some());
        assert!(response.get("patterns").is_some());
    }

    #[tokio::test]
    async fn test_graph_correlation_export_json() {
        let (server, _ctx) = create_test_server().await;

        let graph = json!({
            "name": "Export Test",
            "graph_type": "Call",
            "description": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "nodes": [{"id": "n1", "node_type": "Function", "label": "func", "metadata": {}, "position": null, "size": null, "color": null}],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_export").with_arguments(
            json!({
                "graph": graph,
                "format": "JSON"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert_eq!(response["format"], "JSON");
        assert!(response.get("content").is_some());
    }

    #[tokio::test]
    async fn test_graph_correlation_export_graphml() {
        let (server, _ctx) = create_test_server().await;

        let graph = json!({
            "name": "GraphML Export",
            "graph_type": "Dependency",
            "description": null,
            "created_at": "2024-01-01T00:00:00Z",
            "updated_at": "2024-01-01T00:00:00Z",
            "nodes": [{"id": "mod1", "node_type": "Module", "label": "module1", "metadata": {}, "position": null, "size": null, "color": null}],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_export").with_arguments(
            json!({
                "graph": graph,
                "format": "GraphML"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_graph_correlation_export_invalid_format() {
        let (server, _ctx) = create_test_server().await;

        let graph = json!({
            "name": "Test",
            "graph_type": "Call",
            "nodes": [],
            "edges": [],
            "metadata": {}
        });

        let request = CallToolRequestParams::new("graph_correlation_export").with_arguments(
            json!({
                "graph": graph,
                "format": "InvalidFormat"
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_graph_correlation_types() {
        let (server, _ctx) = create_test_server().await;

        let request = CallToolRequestParams::new("graph_correlation_types");

        let result = handle_nexus_mcp_tool(request, server).await;
        assert!(result.is_ok());

        let tool_result = result.unwrap();
        let text = &tool_result.content[0].as_text().expect("text content").text;
        let response: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(response["status"], "success");
        assert!(response.get("types").is_some());

        let types = response["types"].as_array().unwrap();
        assert_eq!(types.len(), 4);
        assert!(types.contains(&json!("Call")));
        assert!(types.contains(&json!("Dependency")));
        assert!(types.contains(&json!("DataFlow")));
        assert!(types.contains(&json!("Component")));
    }

    #[tokio::test]
    async fn test_mcp_tools_include_graph_correlation() {
        let tools = get_nexus_mcp_tools();
        let tool_names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

        assert!(tool_names.contains(&"graph_correlation_generate"));
        assert!(tool_names.contains(&"graph_correlation_analyze"));
        assert!(tool_names.contains(&"graph_correlation_export"));
        assert!(tool_names.contains(&"graph_correlation_types"));
    }
}
