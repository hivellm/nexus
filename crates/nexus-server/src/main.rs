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

// Activate jemalloc as the global allocator when the `memory-profiling`
// feature is enabled. Combined with `MALLOC_CONF=prof:true,...`, this lets
// ops dump pprof heap profiles from the running process on demand (see
// `api::debug`).
#[cfg(all(feature = "memory-profiling", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request},
    middleware::Next,
    routing::{any, delete, get, post, put},
};
use clap::Parser;
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use tokio::sync::RwLock as TokioRwLock;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Server imports
use tokio::net::TcpListener;

use axum::middleware as axum_middleware;
use nexus_core::auth::middleware::AuthMiddleware;
use nexus_server::{
    NexusServer, api, config,
    middleware::{RateLimiter, create_auth_middleware, mcp_auth_middleware_handler},
};

/// Nexus Server CLI arguments
#[derive(Parser, Debug)]
#[command(name = "nexus-server")]
#[command(about = "Nexus Graph Database HTTP Server", long_about = None)]
struct Args {
    /// Enable verbose logging (prints debug information to stdout/stderr)
    #[arg(long, short = 'v')]
    verbose: bool,
}

fn main() -> anyhow::Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // Configure Tokio runtime for high concurrency
    // Use CPU count * 2 for worker threads, minimum 8, maximum 32
    let worker_threads = (thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        * 2)
    .clamp(8, 32);

    // Use CPU count * 4 for blocking threads, minimum 32, maximum 128
    let blocking_threads = (thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        * 4)
    .clamp(32, 128);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(blocking_threads)
        .thread_name("nexus-worker")
        .thread_stack_size(2 * 1024 * 1024) // 2MB stack
        .enable_all()
        .build()?;

    // Initialize tracing early (before async_main) to capture runtime logs.
    //
    // Default filter fragment pinned to our crates — anything else that
    // hooks into `tracing` (notably `hnsw_rs`, whose `info!` firehose was
    // flooding production logs with `Hnsw max_nb_connection 16 …
    // entering PointIndexation drop` per index access) stays at `warn`
    // until the operator explicitly asks for more.
    let verbose = args.verbose;
    let filter = if verbose {
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "nexus_server=debug,nexus_core=debug,tower_http=debug,hnsw_rs=warn".into()
        })
    } else {
        // Only show errors and warnings when not verbose.
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            "nexus_server=error,nexus_core=warn,tower_http=error,hnsw_rs=warn".into()
        })
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    if verbose {
        tracing::info!(
            "Configured Tokio runtime: {} worker threads, {} blocking threads",
            worker_threads,
            blocking_threads
        );
    }

    rt.block_on(async_main(worker_threads))
}

async fn async_main(_worker_threads: usize) -> anyhow::Result<()> {
    // Tracing already initialized in main()

    // Load configuration (YAML file -> env vars -> defaults, env wins).
    let config = config::Config::from_env();

    // Initialize Engine (contains all core components)
    // `config.data_dir` already merges NEXUS_DATA_DIR / YAML / default, so
    // we use it directly instead of re-reading the env var here.
    let data_dir = config.data_dir.clone();
    std::fs::create_dir_all(&data_dir)?;
    let engine = nexus_core::Engine::with_data_dir_and_config(&data_dir, config.engine.clone())?;
    info!(
        "Using persistent data directory: {} (page_cache_capacity={})",
        data_dir, config.engine.page_cache_capacity
    );
    let engine_arc = Arc::new(TokioRwLock::new(engine));

    // Build the shared executor (with query cache enabled) that every
    // handler reads via State<Arc<NexusServer>>.
    let executor = Arc::new(api::cypher::build_executor()?);

    // schema / stats / knn handlers all read server state via
    // State<Arc<NexusServer>> now (see phase2b); no init_* dance needed.
    // Performance + MCP tool monitoring are constructed inside
    // NexusServer::new (phase2c) with the same defaults the previous
    // init_* pair used.

    // Initialize DatabaseManager for multi-database support
    let database_manager = nexus_core::database::DatabaseManager::new(data_dir.clone().into())?;
    let database_manager_arc = Arc::new(RwLock::new(database_manager));

    // Wire the DatabaseManager into the executor so multi-database
    // Cypher commands (USE / CREATE DATABASE / ...) can reach it.
    executor
        .set_database_manager(database_manager_arc.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set database manager on executor"))?;
    info!("Multi-database support enabled with DatabaseManager");

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

    let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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
        executor.clone(),
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

    // Optional RESP3 listener (see docs/specs/resp3-nexus-commands.md).
    // Spawned before we start serving HTTP so `redis-cli -p 15476 PING`
    // is already reachable as soon as the HTTP listener opens.
    if config.resp3.enabled {
        match nexus_server::protocol::resp3::spawn_resp3_listener(
            nexus_server.clone(),
            config.resp3.addr,
            config.resp3.require_auth,
        )
        .await
        {
            Ok(_handle) => {
                info!(
                    "Nexus RESP3 listener bound on {} (auth_required={})",
                    config.resp3.addr, config.resp3.require_auth
                );
            }
            Err(e) => {
                warn!(
                    "Failed to bind RESP3 listener on {}: {}. HTTP/MCP surfaces continue unaffected.",
                    config.resp3.addr, e
                );
            }
        }
    }

    // Native binary RPC listener (see docs/specs/rpc-wire-format.md).
    // Enabled by default — first-party SDKs prefer this transport for its
    // multiplexed MessagePack framing, but HTTP and RESP3 keep running
    // regardless so existing clients and tooling stay working.
    if config.rpc.enabled {
        match nexus_server::protocol::rpc::spawn_rpc_listener(
            nexus_server.clone(),
            config.rpc.addr,
            config.rpc.clone(),
            config.rpc.require_auth,
        )
        .await
        {
            Ok(()) => {
                info!(
                    "Nexus RPC listener bound on {} (auth_required={}, max_frame_bytes={})",
                    config.rpc.addr, config.rpc.require_auth, config.rpc.max_frame_bytes
                );
            }
            Err(e) => {
                warn!(
                    "Failed to bind RPC listener on {}: {}. HTTP/RESP3 surfaces continue unaffected.",
                    config.rpc.addr, e
                );
            }
        }
    }

    // Hoisted above `create_mcp_router` so both the MCP and main
    // routers see the same cluster flag. Legacy auth stays wired
    // up through `auth.enabled`; cluster mode piggy-backs on it.
    let cluster_enabled = config.cluster.enabled;

    // Create MCP router with StreamableHTTP transport
    let mcp_router = create_mcp_router(nexus_server.clone(), cluster_enabled).await?;

    // Health + Prometheus now read `server.start_time` and
    // `server.metrics` via State<Arc<NexusServer>> (phase2e); the
    // `api::health::init` + `api::prometheus::init` bootstrap pair
    // that used to live here is gone.

    // The comparison graphs + correlation manager + UMICP handler are
    // owned by NexusServer::new (phase2d), so the `init_graphs` /
    // `init_manager` scaffolding that used to live here is gone.

    // Initialize rate limiter (for future use)
    let _rate_limiter = RateLimiter::new();

    // Initialize authentication middleware if enabled
    // For now, we'll enable it based on config.auth.enabled
    // In the future, this can be made more granular per route
    // Cluster mode implies authentication — no "cluster without auth"
    // deployment shape exists. The `||` here keeps the legacy
    // auth.enabled = true / cluster.enabled = false deployments wired
    // up exactly as before. `cluster_enabled` was already hoisted
    // above `create_mcp_router`; reuse it.
    let auth_middleware_state = if config.auth.enabled || cluster_enabled {
        Some(create_auth_middleware(
            nexus_server.clone(),
            true,
            cluster_enabled,
        ))
    } else {
        None
    };

    // Build main router
    let mut app = Router::new()
        .route("/", get(api::health::health_check))
        .route("/health", get(api::health::health_check))
        .route("/metrics", get(api::health::metrics))
        .route("/prometheus", get(api::prometheus::prometheus_metrics))
        // Memory profiling endpoints. They respond 503 if the crate was
        // built without `--features memory-profiling`, so the routes are
        // always wired — no conditional routing required.
        .route("/debug/memory", get(api::debug::memory_stats))
        .route("/debug/heap/dump", post(api::debug::heap_dump))
        .route("/test", get(|| async { "Test endpoint working" }))
        .route("/cypher-debug", post(|body: String| async move {
            tracing::debug!("Raw body received on /cypher-debug: {}", body);
            Json(serde_json::json!({"message": "Debug endpoint received", "body": body}))
        }))
        .route("/cypher", post(api::cypher::execute_cypher))
        .route("/test-handler", get(|| async {
            tracing::debug!("Handler called!");
            "Handler called successfully"
        }))
        // GraphQL endpoints
        .route("/graphql", post({
            let server = nexus_server.clone();
            let graphql_schema = api::graphql::create_schema(server);
            move |req| {
                let schema = graphql_schema.clone();
                api::graphql::graphql_handler(axum::extract::State(schema), req)
            }
        }))
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
        .route("/schema/indexes", get({
            let server = nexus_server.clone();
            move || {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::list_indexes(axum::extract::State(state))
            }
        }))
        .route("/schema/indexes", post({
            let server = nexus_server.clone();
            move |req: axum::extract::Json<api::indexes::CreateIndexRequest>| {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::create_index(axum::extract::State(state), req)
            }
        }))
        .route("/schema/indexes/{name}", delete({
            let server = nexus_server.clone();
            move |path: axum::extract::Path<String>| {
                let state = api::indexes::IndexState {
                    engine: server.engine.clone(),
                };
                api::indexes::delete_index(axum::extract::State(state), path)
            }
        }))
        // Property keys endpoint
        .route("/property_keys", get({
            let server = nexus_server.clone();
            move || {
                let state = api::property_keys::PropertyKeysState {
                    engine: server.engine.clone(),
                };
                api::property_keys::list_property_keys(axum::extract::State(state))
            }
        }))
        // Logs endpoint
        .route("/logs", get(api::logs::get_logs))
        // Query history endpoint (placeholder - will be implemented when query history is added to server)
        .route("/query-history", get(|| async {
            axum::Json(serde_json::json!({
                "queries": [],
                "total": 0
            }))
        }))
        // Config endpoint
        .route("/config", get(api::config::get_config))
        // Data management endpoints
        .route("/data/nodes", get(api::data::get_node_by_id))
        .route("/data/nodes", post(api::data::create_node))
        .route("/data/nodes", put(api::data::update_node))
        .route("/data/nodes", delete(api::data::delete_node))
        .route("/data/relationships", post(api::data::create_rel))
        // Statistics endpoint
        .route("/stats", get(api::stats::get_stats))
        // Cluster-mode per-tenant stats. Returns 404
        // CLUSTER_MODE_DISABLED on standalone deployments, 404
        // TENANT_UNKNOWN for tenants that haven't been seen yet,
        // and 401 NO_TENANT_CONTEXT if the request didn't carry
        // a tenant binding (shouldn't happen in cluster mode but
        // guards the case anyway).
        .route("/cluster/stats/self", get(api::cluster_stats::tenant_stats))
        // Database management endpoints
        .route(
            "/databases",
            get({
                let server = nexus_server.clone();
                move || {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::list_databases(axum::extract::State(api::database::DatabaseState { manager })).await
                    }
                }
            }),
        )
        .route(
            "/databases",
            post({
                let server = nexus_server.clone();
                move |request| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::create_database(axum::extract::State(api::database::DatabaseState { manager }), request).await
                    }
                }
            }),
        )
        .route(
            "/databases/{name}",
            get({
                let server = nexus_server.clone();
                move |path| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::get_database(axum::extract::State(api::database::DatabaseState { manager }), path).await
                    }
                }
            }),
        )
        .route(
            "/databases/{name}",
            delete({
                let server = nexus_server.clone();
                move |path| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::drop_database(axum::extract::State(api::database::DatabaseState { manager }), path).await
                    }
                }
            }),
        )
        // Session database endpoints
        .route(
            "/session/database",
            get({
                let server = nexus_server.clone();
                move || {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::get_session_database(axum::extract::State(api::database::DatabaseState { manager })).await
                    }
                }
            }),
        )
        .route(
            "/session/database",
            put({
                let server = nexus_server.clone();
                move |request| {
                    let manager = server.database_manager.clone();
                    async move {
                        api::database::switch_session_database(axum::extract::State(api::database::DatabaseState { manager }), request).await
                    }
                }
            }),
        )
        .route("/cache/stats", get(api::cypher::get_cache_stats))
        .route("/cache/clear", post(api::cypher::clear_cache))
        .route("/cache/clean", post(api::cypher::clean_cache))
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
        // Replication endpoints
        .route("/replication/status", get(api::replication::get_status))
        .route("/replication/master/stats", get(api::replication::get_master_stats))
        .route("/replication/replica/stats", get(api::replication::get_replica_stats))
        .route("/replication/replicas", get(api::replication::list_replicas))
        .route("/replication/promote", post(api::replication::promote_to_master))
        .route("/replication/snapshot", post(api::replication::create_snapshot))
        .route("/replication/snapshot", get(api::replication::get_last_snapshot))
        .route("/replication/stop", post(api::replication::stop_replication))
        // V2 sharded-cluster management (Phase 5). Endpoints return
        // 503 when sharding is disabled on this node — see
        // `api::cluster`.
        .route("/cluster/status", get(api::cluster::get_status))
        .route("/cluster/add_node", post(api::cluster::add_node))
        .route("/cluster/remove_node", post(api::cluster::remove_node))
        .route("/cluster/rebalance", post(api::cluster::rebalance))
        .route("/cluster/shards/{id}", get(api::cluster::get_shard))
        // Add state to router (must be after all routes)
        .with_state(nexus_server.clone());

    // Cluster-mode quota gate. Layered BEFORE the auth middleware in
    // this block so it ends up INSIDE the auth layer at runtime (axum
    // composes layers outermost-first, last `.layer()` call → closest
    // to the handler). The quota middleware is a pass-through in
    // standalone mode — the check is guarded on presence of a
    // `UserContext` in request extensions, which only the cluster-mode
    // auth path inserts.
    //
    // The SAME provider is also installed on `NexusServer` (and,
    // through it, on the inner `Engine`) so the rate-limit middleware
    // on the HTTP side and the storage-quota gate on the engine-write
    // side consult ONE tenant's usage counters, not two out-of-sync
    // views.
    if cluster_enabled {
        let quota_provider: std::sync::Arc<dyn nexus_core::cluster::QuotaProvider> =
            nexus_core::cluster::LocalQuotaProvider::new(config.cluster.default_quotas.clone());
        nexus_server
            .set_cluster_quota_provider(Some(quota_provider.clone()))
            .await;
        let quota_state = nexus_core::cluster::QuotaMiddlewareState::new(quota_provider);
        app = app.layer(axum_middleware::from_fn_with_state(
            quota_state,
            nexus_core::cluster::quota_middleware_handler,
        ));
    }

    // Apply authentication middleware if enabled
    if let Some(auth_middleware) = auth_middleware_state {
        app = app.layer(axum_middleware::from_fn_with_state(
            auth_middleware,
            |state: axum::extract::State<AuthMiddleware>, request: Request, next: Next| async move {
                nexus_server::middleware::auth::auth_middleware_handler(state, request, next).await
            },
        ));
    }

    // Add GraphQL playground route in debug builds
    #[cfg(debug_assertions)]
    let app = app.route("/graphql/playground", get(api::graphql::graphql_playground));

    // Apply middleware layers
    let app = app
        // Cap request body size. Without this, Axum allows bodies up to its
        // internal default (2 MB) but we want the value to come from config
        // so ops can tune it per deployment. A single oversized POST must
        // not be able to exhaust the server allocator.
        .layer(DefaultBodyLimit::max(config.max_body_size_bytes))
        // Compression for responses (gzip, deflate, br)
        .layer(CompressionLayer::new())
        // CORS support
        .layer(CorsLayer::permissive())
        // Request/response tracing
        .layer(TraceLayer::new_for_http());

    // Start server with optimized configuration for high concurrency
    let listener = TcpListener::bind(&config.addr).await?;
    info!("Nexus Server listening on {}", config.addr);

    tracing::debug!("Starting optimized Axum server with high concurrency settings");

    // Start server
    axum::serve(listener, app).await?;

    Ok(())
}

/// Create MCP router with StreamableHTTP transport
async fn create_mcp_router(
    nexus_server: Arc<NexusServer>,
    cluster_enabled: bool,
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
    if nexus_server.auth_manager.config().enabled || cluster_enabled {
        let auth_middleware = create_auth_middleware(
            nexus_server.clone(),
            true, // Require authentication for MCP
            cluster_enabled,
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
    use nexus_core::testing::TestContext;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_config_default() {
        use config::Config;
        let config = Config::default();
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[tokio::test]
    async fn test_nexus_server_creation() {
        let ctx = TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager =
            nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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
        // Executor is Arc<Executor>, no need to lock
        let _engine_guard = server_arc.engine.read().await;

        // If we get here, the locks were acquired successfully
    }

    #[tokio::test]
    async fn test_nexus_server_clone() {
        let ctx = TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager =
            nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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
        let ctx = TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager =
            nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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

        // Test that MCP router can be created (standalone mode; cluster off)
        let result = create_mcp_router(server, false).await;
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
        let ctx = TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager =
            nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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

        // Test that we can create the MCP router (standalone mode)
        let mcp_router_result = create_mcp_router(server.clone(), false).await;
        assert!(mcp_router_result.is_ok());

        let _mcp_router = mcp_router_result.unwrap();

        // Test that the router has routes
        // Note: axum::Router doesn't have a routes() method, so we just verify it was created
    }

    #[tokio::test]
    async fn test_nexus_server_fields() {
        let ctx = TestContext::new();
        let engine = nexus_core::Engine::with_data_dir(ctx.path()).unwrap();
        let engine_arc = Arc::new(TokioRwLock::new(engine));

        let executor = nexus_core::executor::Executor::default();
        let executor_arc = Arc::new(executor);

        let database_manager =
            nexus_core::database::DatabaseManager::new(ctx.path().into()).unwrap();
        let database_manager_arc = Arc::new(RwLock::new(database_manager));
        let rbac = nexus_core::auth::RoleBasedAccessControl::new();
        let rbac_arc = Arc::new(TokioRwLock::new(rbac));

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
