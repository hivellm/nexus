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
    middleware::Next,
    routing::{any, delete, get, post, put},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use axum::middleware as axum_middleware;
use nexus_core::auth::middleware::AuthMiddleware;
use nexus_server::{
    NexusServer, api, config,
    middleware::{RateLimiter, create_auth_middleware, mcp_auth_middleware_handler},
};

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

    // Load configuration (from env vars and/or config/auth.toml)
    let config = config::Config::from_env();

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

    // The Engine already contains Catalog, LabelIndex, KnnIndex etc.
    // For the new data endpoints, we'll use the Engine's components directly via engine_arc.
    // No need to create separate instances - they should all come from Engine.

    // Initialize engine for all API modules that need it
    api::data::init_engine(engine_arc.clone())?;
    api::stats::init_engine(engine_arc.clone())?;
    // Initialize cypher engine
    api::cypher::init_engine(engine_arc.clone())?;
    // Initialize performance monitoring
    api::performance::init_performance_monitoring(1000, 1000, 100, 10)?; // 1000ms threshold, 1000 max slow queries, 100 plan cache size, 10MB memory

    // Initialize MCP tool performance monitoring
    api::mcp_performance::init_mcp_performance_monitoring(
        500,  // 500ms threshold for slow tools
        1000, // Max 1000 slow tool records
        3600, // 1 hour cache TTL
        100,  // Max 100 cache entries
    )?;

    // Initialize DatabaseManager for multi-database support
    let database_manager = nexus_core::database::DatabaseManager::new(data_dir.clone().into())?;
    let database_manager_arc = Arc::new(RwLock::new(database_manager));

    // Initialize RBAC for user management
    let mut rbac = nexus_core::auth::RoleBasedAccessControl::new();

    // Create root user if enabled in config
    if config.root_user.enabled {
        // Hash password with SHA512
        let password_hash = nexus_core::auth::hash_password(&config.root_user.password);

        if let Err(e) = rbac.create_root_user(config.root_user.username.clone(), password_hash) {
            warn!("Failed to create root user: {}", e);
        } else {
            info!(
                "Root user '{}' created successfully",
                config.root_user.username
            );
        }
    }

    let rbac_arc = Arc::new(RwLock::new(rbac));

    // Initialize AuthManager for API key management with LMDB persistence
    let auth_config = nexus_core::auth::AuthConfig::default();
    let auth_manager = if auth_config.enabled {
        // Use persistent storage when authentication is enabled
        let auth_storage_path = std::path::Path::new(&data_dir).join("auth");
        std::fs::create_dir_all(&auth_storage_path)?;
        Arc::new(
            nexus_core::auth::AuthManager::with_storage(auth_config, auth_storage_path)
                .map_err(|e| anyhow::anyhow!("Failed to initialize auth storage: {}", e))?,
        )
    } else {
        // Use in-memory storage when authentication is disabled
        Arc::new(nexus_core::auth::AuthManager::new(auth_config))
    };

    // Initialize JWT manager
    let jwt_config = nexus_core::auth::JwtConfig::from_env();
    let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

    // Initialize audit logger
    let audit_config = nexus_core::auth::AuditConfig {
        enabled: true,
        log_dir: std::path::PathBuf::from(&data_dir).join("audit"),
        retention_days: 90,
        compress_logs: true,
    };
    let audit_logger = Arc::new(
        nexus_core::auth::AuditLogger::new(audit_config)
            .map_err(|e| anyhow::anyhow!("Failed to initialize audit logger: {}", e))?,
    );

    // Create Nexus server state
    let nexus_server = Arc::new(NexusServer::new(
        api::cypher::get_executor(),
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager.clone(),
        jwt_manager.clone(),
        audit_logger.clone(),
        config.root_user.clone(),
    ));

    // Start expired API keys cleanup job (runs every hour)
    // Only start if authentication is enabled
    if auth_manager.config().enabled {
        NexusServer::start_expired_keys_cleanup_job(auth_manager.clone(), 3600); // 1 hour = 3600 seconds
        info!("Started expired API keys cleanup job (runs every hour)");
    }

    // Validate MCP API key if provided
    if let Some(mcp_api_key) = config::Config::mcp_api_key() {
        if auth_manager.config().enabled {
            match auth_manager.verify_api_key(&mcp_api_key) {
                Ok(Some(_)) => {
                    info!("MCP API key validated successfully");
                }
                Ok(None) | Err(_) => {
                    warn!(
                        "MCP API key from NEXUS_MCP_API_KEY environment variable is invalid or not found"
                    );
                }
            }
        } else {
            warn!(
                "NEXUS_MCP_API_KEY is set but authentication is disabled. MCP authentication will not be enforced."
            );
        }
    }

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

    // Initialize authentication middleware if enabled
    // For now, we'll enable it based on config.auth.enabled
    // In the future, this can be made more granular per route
    let auth_middleware_state = if config.auth.enabled {
        Some(create_auth_middleware(nexus_server.clone(), true))
    } else {
        None
    };

    // Build main router
    let mut app = Router::new()
        .route("/", get(api::health::health_check))
        .route("/health", get(api::health::health_check))
        .route("/metrics", get(api::health::metrics))
        .route("/cypher", post(api::cypher::execute_cypher))
        // Always insert None auth context for endpoints when auth is disabled
        .layer({
            let auth_enabled = config.auth.enabled;
            axum::middleware::from_fn(move |mut request: axum::extract::Request, next: axum::middleware::Next| async move {
                if !auth_enabled {
                    request.extensions_mut().insert(axum::extract::Extension(None::<nexus_core::auth::middleware::AuthContext>));
                }
                next.run(request).await
            })
        })
        // Authentication endpoints
        .route("/auth/users", post(api::auth::create_user))
        .route(
            "/auth/users",
            get({
                let server = nexus_server.clone();
                move || api::auth::list_users(axum::extract::State(server))
            }),
        )
        .route(
            "/auth/users/{username}",
            get({
                let server = nexus_server.clone();
                move |path| api::auth::get_user(axum::extract::State(server), path)
            }),
        )
        .route("/auth/users/{username}", delete(api::auth::delete_user))
        .route(
            "/auth/users/{username}/permissions",
            post(api::auth::grant_permissions),
        )
        .route(
            "/auth/users/{username}/permissions",
            get({
                let server = nexus_server.clone();
                move |path| api::auth::get_user_permissions(axum::extract::State(server), path)
            }),
        )
        .route(
            "/auth/users/{username}/permissions/{permission}",
            delete(api::auth::revoke_permission),
        )
        // API key management endpoints
        .route("/auth/keys", post(api::auth::create_api_key))
        .route(
            "/auth/keys",
            get({
                let server = nexus_server.clone();
                move |query| api::auth::list_api_keys(axum::extract::State(server), query)
            }),
        )
        .route(
            "/auth/keys/{key_id}",
            get({
                let server = nexus_server.clone();
                move |path| api::auth::get_api_key(axum::extract::State(server), path)
            }),
        )
        .route("/auth/keys/{key_id}", delete(api::auth::delete_api_key))
        .route(
            "/auth/keys/{key_id}/revoke",
            post(api::auth::revoke_api_key),
        )
        .route("/knn_traverse", post(api::knn::knn_traverse))
        .route(
            "/ingest",
            post(
                move |state: axum::extract::State<std::sync::Arc<NexusServer>>, request| {
                    api::ingest::ingest_data(state, request)
                },
            ),
        )
        .route(
            "/export",
            get(
                move |state: axum::extract::State<std::sync::Arc<NexusServer>>, query| {
                    api::export::export_data(state, query)
                },
            ),
        )
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
        // Performance monitoring endpoints
        .route(
            "/performance/statistics",
            get(api::performance::get_query_statistics),
        )
        .route(
            "/performance/slow-queries",
            get(api::performance::get_slow_queries),
        )
        .route(
            "/performance/slow-queries/analysis",
            get(api::performance::analyze_slow_queries),
        )
        .route(
            "/performance/plan-cache",
            get(api::performance::get_plan_cache_statistics),
        )
        .route(
            "/performance/plan-cache/clear",
            post(api::performance::clear_plan_cache),
        )
        // MCP tool performance monitoring endpoints
        .route(
            "/mcp/performance/statistics",
            get(api::mcp_performance::get_mcp_tool_statistics),
        )
        .route(
            "/mcp/performance/tools/{tool_name}",
            get(api::mcp_performance::get_tool_statistics),
        )
        .route(
            "/mcp/performance/slow-tools",
            get(api::mcp_performance::get_slow_tool_calls),
        )
        .route(
            "/mcp/performance/cache",
            get(api::mcp_performance::get_cache_statistics),
        )
        .route(
            "/mcp/performance/cache/clear",
            post(api::mcp_performance::clear_cache),
        )
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
        // UMICP endpoint for graph correlation
        .route(
            "/umicp/graph",
            post(api::graph_correlation_umicp::handle_umicp_request),
        )
        .route(
            "/openapi.json",
            get(|| async { axum::Json(api::openapi::generate_openapi_spec()) }),
        )
        // MCP StreamableHTTP endpoint
        .nest("/mcp", mcp_router)
        // Add state to router (must be after all routes)
        .with_state(nexus_server.clone());

    // Apply authentication middleware if enabled
    if let Some(auth_middleware) = auth_middleware_state {
        app = app.layer(axum_middleware::from_fn_with_state(
            auth_middleware,
            |state: axum::extract::State<AuthMiddleware>, request: Request, next: Next| async move {
                nexus_server::middleware::auth::auth_middleware_handler(state, request, next).await
            },
        ));
    }

    // Apply tracing layer
    let app = app.layer(TraceLayer::new_for_http());

    // Start server
    let listener = tokio::net::TcpListener::bind(&config.addr).await?;
    info!("Nexus Server listening on {}", config.addr);

    axum::serve(listener, app).await?;

    Ok(())
}

/// Create MCP router with StreamableHTTP transport
async fn create_mcp_router(
    nexus_server: Arc<NexusServer>,
) -> anyhow::Result<Router<Arc<NexusServer>>> {
    use hyper::service::Service;
    use hyper_util::service::TowerToHyperService;
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

    // Create MCP service handler
    let server = nexus_server.clone();

    // Create StreamableHTTP service
    let streamable_service = StreamableHttpService::new(
        move || Ok(crate::api::streaming::NexusMcpService::new(server.clone())),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // Convert to axum service and create router
    let hyper_service = TowerToHyperService::new(streamable_service);

    // Create router with the MCP endpoint
    let mut router = Router::new()
        .route(
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
        )
        .with_state(nexus_server.clone());

    // Apply MCP authentication middleware if authentication is enabled
    if nexus_server.auth_manager.config().enabled {
        let auth_middleware = create_auth_middleware(
            nexus_server.clone(),
            true, // Require authentication for MCP
        );

        router = router.layer(axum_middleware::from_fn_with_state(
            auth_middleware,
            |state: axum::extract::State<nexus_core::auth::middleware::AuthMiddleware>,
             request: Request,
             next: Next| async move {
                mcp_auth_middleware_handler(state, request, next).await
            },
        ));
    }

    Ok(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::RootUserConfig;
    use std::net::{IpAddr, Ipv4Addr};
    use tempfile::TempDir;

    #[test]
    fn test_config_default() {
        use config::Config;
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

        let database_manager =
            nexus_core::database::DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = nexus_core::auth::AuthConfig::default();
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));

        let jwt_config = nexus_core::auth::JwtConfig::default();
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        let server = NexusServer::new(
            executor_arc.clone(),
            engine_arc.clone(),
            database_manager_arc,
            rbac_arc,
            auth_manager,
            jwt_manager,
            audit_logger,
            RootUserConfig::default(),
        );

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

        let database_manager =
            nexus_core::database::DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = nexus_core::auth::AuthConfig::default();
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));

        let jwt_config = nexus_core::auth::JwtConfig::default();
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        let server = NexusServer::new(
            executor_arc,
            engine_arc,
            database_manager_arc,
            rbac_arc,
            auth_manager.clone(),
            jwt_manager,
            audit_logger,
            RootUserConfig::default(),
        );
        let cloned = server.clone();

        // Test that clone works and references the same underlying data
        assert!(Arc::ptr_eq(&server.executor, &cloned.executor));
        assert!(Arc::ptr_eq(&server.engine, &cloned.engine));
        assert!(Arc::ptr_eq(
            &server.database_manager,
            &cloned.database_manager
        ));
        assert!(Arc::ptr_eq(&server.rbac, &cloned.rbac));
        assert!(Arc::ptr_eq(&server.auth_manager, &cloned.auth_manager));
    }

    #[tokio::test]
    async fn test_create_mcp_router() {
        let temp_dir = TempDir::new().unwrap();
        let engine = nexus_core::Engine::with_data_dir(temp_dir.path()).unwrap();
        let engine_arc = Arc::new(RwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(RwLock::new(executor));

        let database_manager =
            nexus_core::database::DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = nexus_core::auth::AuthConfig::default();
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));

        let jwt_config = nexus_core::auth::JwtConfig::default();
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
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

        // Test that MCP router can be created
        let result = create_mcp_router(server).await;
        assert!(result.is_ok());

        let _router = result.unwrap();
        // Router should be created successfully
        // Note: axum::Router doesn't have a routes() method, so we just verify it was created
    }

    #[test]
    fn test_config_parsing() {
        use config::Config;
        let config = Config::default();

        // Test that default config has expected values
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.data_dir, "./data");
    }

    #[test]
    #[ignore] // Flaky due to parallel test execution env var pollution
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

        use config::Config;
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

        use config::Config;
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

        use config::Config;
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

        let database_manager =
            nexus_core::database::DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = nexus_core::auth::AuthConfig::default();
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));

        let jwt_config = nexus_core::auth::JwtConfig::default();
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
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

        let database_manager =
            nexus_core::database::DatabaseManager::new(temp_dir.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(RwLock::new(rbac));

        let auth_config = nexus_core::auth::AuthConfig::default();
        let auth_manager = Arc::new(nexus_core::auth::AuthManager::new(auth_config));

        let jwt_config = nexus_core::auth::JwtConfig::default();
        let jwt_manager = Arc::new(nexus_core::auth::JwtManager::new(jwt_config));

        let audit_logger = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: std::path::PathBuf::from("./logs"),
                retention_days: 30,
                compress_logs: false,
            })
            .unwrap(),
        );

        let server = NexusServer::new(
            executor_arc.clone(),
            engine_arc.clone(),
            database_manager_arc.clone(),
            rbac_arc.clone(),
            auth_manager.clone(),
            jwt_manager,
            audit_logger,
            RootUserConfig::default(),
        );

        // Test that all fields are accessible
        assert!(Arc::ptr_eq(&server.executor, &executor_arc));
        assert!(Arc::ptr_eq(&server.engine, &engine_arc));
        assert!(Arc::ptr_eq(&server.database_manager, &database_manager_arc));
        assert!(Arc::ptr_eq(&server.rbac, &rbac_arc));
        assert!(Arc::ptr_eq(&server.auth_manager, &auth_manager));
    }
}
