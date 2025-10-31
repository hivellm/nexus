//! Nexus Server - HTTP API for graph database
//!
//! Provides REST endpoints for:
//! - POST /cypher - Execute Cypher queries
//! - POST /knn_traverse - KNN-seeded graph traversal
//! - POST /ingest - Bulk data ingestion
//! - POST /schema/labels - Create labels
//! - GET /schema/labels - List labels
//! - POST /schema/rel_types - Create relationship types
//! - GET /schema/rel_types - List relationship types
//! - POST /data/nodes - Create nodes
//! - POST /data/relationships - Create relationships
//! - PUT /data/nodes - Update nodes
//! - DELETE /data/nodes - Delete nodes
//! - GET /stats - Database statistics
//! - POST /mcp - MCP StreamableHTTP endpoint

use axum::{
    Router,
    extract::Request,
    routing::{any, delete, get, post, put},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use nexus_server::{NexusServer, api, config::Config, middleware::RateLimiter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "nexus_server=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::default();

    // Initialize Engine (contains all core components)
    // Use persistent data directory instead of tempdir
    let data_dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    std::fs::create_dir_all(&data_dir)?;
    let engine = nexus_core::Engine::with_data_dir(&data_dir)?;
    info!("Using persistent data directory: {}", data_dir);
    let engine_arc = Arc::new(RwLock::new(engine));

    // Initialize executor
    let executor = api::cypher::init_executor()?;

    // Share executor with other modules
    api::knn::init_executor(executor.clone())?;
    api::ingest::init_executor(executor)?;

    // The Engine already contains Catalog, LabelIndex, KnnIndex etc.
    // For the new data endpoints, we'll use the Engine's components directly via engine_arc.
    // No need to create separate instances - they should all come from Engine.

    // Initialize engine for all API modules that need it
    api::data::init_engine(engine_arc.clone())?;
    api::stats::init_engine(engine_arc.clone())?;
    // Initialize cypher engine
    api::cypher::init_engine(engine_arc.clone())?;

    // Create Nexus server state (simplified - only engine and executor needed)
    let nexus_server = Arc::new(NexusServer {
        executor: api::cypher::get_executor(),
        engine: engine_arc,
    });

    info!("Starting Nexus Server on {}", config.addr);

    // Create MCP router with StreamableHTTP transport
    let mcp_router = create_mcp_router(nexus_server.clone()).await?;

    // Initialize health check system
    api::health::init();

    // Initialize comparison service with dummy graphs
    // In a real implementation, these would be actual graph instances
    let temp_dir_a = tempfile::tempdir()?;
    let temp_dir_b = tempfile::tempdir()?;

    let store_a = nexus_core::storage::RecordStore::new(temp_dir_a.path())?;
    let store_b = nexus_core::storage::RecordStore::new(temp_dir_b.path())?;

    let catalog_a = Arc::new(nexus_core::catalog::Catalog::new(
        temp_dir_a.path().join("catalog"),
    )?);
    let catalog_b = Arc::new(nexus_core::catalog::Catalog::new(
        temp_dir_b.path().join("catalog"),
    )?);

    let graph_a = Arc::new(std::sync::Mutex::new(nexus_core::Graph::new(
        store_a, catalog_a,
    )));
    let graph_b = Arc::new(std::sync::Mutex::new(nexus_core::Graph::new(
        store_b, catalog_b,
    )));

    api::comparison::init_graphs(graph_a, graph_b)?;

    // Initialize graph correlation manager
    let graph_manager = Arc::new(std::sync::Mutex::new(
        nexus_core::graph::correlation::GraphCorrelationManager::new(),
    ));
    api::graph_correlation::init_manager(graph_manager)?;

    // Initialize rate limiter (for future use)
    let _rate_limiter = RateLimiter::new();

    // Build main router
    let app = Router::new()
        .route("/", get(api::health::health_check))
        .route("/health", get(api::health::health_check))
        .route("/metrics", get(api::health::metrics))
        .route("/cypher", post(api::cypher::execute_cypher))
        .route("/knn_traverse", post(api::knn::knn_traverse))
        .route("/ingest", post(api::ingest::ingest_data))
        // Schema management endpoints
        .route("/schema/labels", post(api::schema::create_label))
        .route("/schema/labels", get(api::schema::list_labels))
        .route("/schema/rel_types", post(api::schema::create_rel_type))
        .route("/schema/rel_types", get(api::schema::list_rel_types))
        // Data management endpoints
        .route("/data/nodes", get(api::data::get_node_by_id))
        .route("/data/nodes", post(api::data::create_node))
        .route("/data/nodes", put(api::data::update_node))
        .route("/data/nodes", delete(api::data::delete_node))
        .route("/data/relationships", post(api::data::create_rel))
        // Statistics endpoint
        .route("/stats", get(api::stats::get_stats))
        // Graph comparison endpoints
        .route("/comparison/compare", post(api::comparison::compare_graphs))
        .route(
            "/comparison/similarity",
            post(api::comparison::calculate_similarity),
        )
        .route("/comparison/stats", post(api::comparison::get_graph_stats))
        .route("/comparison/health", get(api::comparison::health_check))
        .route(
            "/comparison/advanced",
            post(api::comparison::advanced_compare_graphs),
        )
        // Clustering endpoints
        .route(
            "/clustering/algorithms",
            get(api::clustering::get_algorithms),
        )
        .route(
            "/clustering/cluster",
            post({
                let server = nexus_server.clone();
                move |request| api::clustering::cluster_nodes(axum::extract::State(server), request)
            }),
        )
        .route(
            "/clustering/group-by-label",
            post({
                let server = nexus_server.clone();
                move |request| {
                    api::clustering::group_by_label(axum::extract::State(server), request)
                }
            }),
        )
        .route(
            "/clustering/group-by-property",
            post({
                let server = nexus_server.clone();
                move |request| {
                    api::clustering::group_by_property(axum::extract::State(server), request)
                }
            }),
        )
        // Graph correlation endpoints
        .route(
            "/graph-correlation/generate",
            post(api::graph_correlation::generate_graph),
        )
        .route(
            "/graph-correlation/types",
            get(api::graph_correlation::get_graph_types),
        )
        .route(
            "/graph-correlation/auto-generate",
            get(api::auto_generate::auto_generate_graphs),
        )
        .route(
            "/openapi.json",
            get(|| async { axum::Json(api::openapi::generate_openapi_spec()) }),
        )
        // MCP StreamableHTTP endpoint
        .nest("/mcp", mcp_router)
        .layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.addr).await?;
    info!("Nexus Server listening on {}", config.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create MCP router with StreamableHTTP transport
async fn create_mcp_router(nexus_server: Arc<NexusServer>) -> anyhow::Result<Router> {
    use hyper::service::Service;
    use hyper_util::service::TowerToHyperService;
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

    // Create MCP service handler
    let server = nexus_server.clone();

    // Create StreamableHTTP service
    let streamable_service = StreamableHttpService::new(
        move || Ok(api::streaming::NexusMcpService::new(server.clone())),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // Convert to axum service and create router
    let hyper_service = TowerToHyperService::new(streamable_service);

    // Create router with the MCP endpoint
    let router = Router::new().route(
        "/",
        any(move |req: Request| {
            let service = hyper_service.clone();
            async move {
                // Forward request to hyper service
                match service.call(req).await {
                    Ok(response) => Ok(response),
                    Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
                }
            }
        }),
    );

    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[tokio::test]
    async fn test_nexus_server_creation() {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let server = NexusServer::new(executor_arc.clone(), engine_arc.clone());

        // Test that the server can be created
        let server_arc = Arc::new(server);
        let _executor_guard = server_arc.executor.read().await;
        let _engine_guard = server_arc.engine.read().await;

        // If we get here, the locks were acquired successfully
    }

    #[test]
    fn test_nexus_server_clone() {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let server = NexusServer::new(executor_arc, engine_arc);
        let cloned = server.clone();

        // Test that clone works and references the same underlying data
        assert!(Arc::ptr_eq(&server.executor, &cloned.executor));
        assert!(Arc::ptr_eq(&server.engine, &cloned.engine));
    }

    #[tokio::test]
    async fn test_create_mcp_router() {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let server = Arc::new(NexusServer::new(executor_arc, engine_arc));

        // Test that MCP router can be created
        let result = create_mcp_router(server).await;
        assert!(result.is_ok());

        let _router = result.unwrap();
        // Router should be created successfully
        // Note: axum::Router doesn't have a routes() method, so we just verify it was created
    }

    #[test]
    fn test_config_parsing() {
        let config = Config::default();

        // Test that default config has expected values
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.data_dir, "./data");
    }

    #[test]
    fn test_config_from_env() {
        // Clear environment first to ensure clean state
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        // Test with environment variables
        unsafe {
            std::env::set_var("NEXUS_ADDR", "192.168.1.100:8080");
            std::env::set_var("NEXUS_DATA_DIR", "/custom/data");
        }

        let config = Config::from_env();
        assert_eq!(config.addr.port(), 8080);
        assert_eq!(
            config.addr.ip(),
            IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100))
        );
        assert_eq!(config.data_dir, "/custom/data");

        // Clean up
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }
    }

    #[test]
    #[ignore] // Flaky due to parallel test execution env var pollution
    fn test_config_from_env_defaults() {
        // Clear environment variables to test defaults
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        let config = Config::from_env();
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.data_dir, "./data");

        // Ensure cleanup even for defaults test
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }
    }

    #[test]
    #[should_panic(expected = "Invalid NEXUS_ADDR")]
    fn test_config_from_env_invalid_addr() {
        // Clear first
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        unsafe {
            std::env::set_var("NEXUS_ADDR", "invalid-address");
        }

        let _config = Config::from_env();

        // Note: cleanup won't run due to panic - test framework handles it
    }

    #[tokio::test]
    async fn test_router_creation() {
        // This test verifies that the router can be created without panicking
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let server = Arc::new(NexusServer::new(executor_arc, engine_arc));

        // Test that we can create the MCP router
        let mcp_router_result = create_mcp_router(server.clone()).await;
        assert!(mcp_router_result.is_ok());

        let _mcp_router = mcp_router_result.unwrap();

        // Test that the router has routes
        // Note: axum::Router doesn't have a routes() method, so we just verify it was created
    }

    #[test]
    fn test_nexus_server_fields() {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let server = NexusServer::new(executor_arc.clone(), engine_arc.clone());

        // Test that all fields are accessible
        assert!(Arc::ptr_eq(&server.executor, &executor_arc));
        assert!(Arc::ptr_eq(&server.engine, &engine_arc));
    }
}
