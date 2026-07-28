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

// Global allocator selection.
//
// Default builds (production, including the `FROM scratch` musl image and
// Windows) use mimalloc — phase9 profiling proved the per-query heap
// allocation churn on the default system allocator, not lock contention,
// is what caps read/write throughput under high thread counts. mimalloc
// is cross-platform (glibc/musl/msvc) and contention-friendly.
//
// The `memory-profiling` feature instead swaps in jemalloc (non-msvc) so
// ops can dump pprof heap profiles on demand (see `api::debug`); on that
// build mimalloc is not the global allocator.
#[cfg(not(feature = "memory-profiling"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(feature = "memory-profiling", not(target_env = "msvc")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use axum::{
    extract::{DefaultBodyLimit, Request},
    middleware::Next,
    routing::get,
};
use clap::Parser;
use parking_lot::RwLock;
use std::sync::Arc;
use std::thread;
use tokio::sync::RwLock as TokioRwLock;
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing::{info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Server imports
use tokio::net::TcpListener;

use axum::middleware as axum_middleware;
use nexus_core::auth::middleware::AuthMiddleware;
use nexus_server::{
    NexusServer, api, config,
    middleware::{RateLimiter, create_auth_middleware},
};

mod mcp_router;
mod routes;

use mcp_router::create_mcp_router;

/// Nexus Server CLI arguments
#[derive(Parser, Debug)]
#[command(name = "nexus-server")]
#[command(about = "Nexus Graph Database HTTP Server", long_about = None)]
struct Args {
    /// Enable verbose logging (prints debug information to stdout/stderr)
    #[arg(long, short = 'v')]
    verbose: bool,

    /// Run a one-shot health probe against a locally running server and
    /// exit immediately (0 = healthy, 1 = unhealthy) instead of starting
    /// the server. Intended as a container `HEALTHCHECK` command for
    /// images that ship no shell (e.g. `FROM scratch`), where the binary
    /// itself must double as the probe. The target port is read from the
    /// `NEXUS_ADDR` environment variable (default 15474).
    #[arg(long)]
    healthcheck: bool,
}

/// Extracts the TCP port to probe for `--healthcheck` from a `NEXUS_ADDR`-style
/// `host:port` string.
///
/// Falls back to the default Nexus port (15474) when `addr` is `None` or does
/// not parse as a valid port. Takes the segment after the last `:` so
/// IPv6-style addresses like `[::]:15474` still resolve to the correct port.
fn healthcheck_port_from_env(addr: Option<&str>) -> u16 {
    const DEFAULT_PORT: u16 = 15474;
    addr.and_then(|s| s.rsplit(':').next())
        .and_then(|port_str| port_str.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Runs the `--healthcheck` probe and never returns: it always terminates the
/// process via `std::process::exit`.
///
/// Connects to `GET /health` on `127.0.0.1:<port>` (port resolved by
/// [`healthcheck_port_from_env`] from `NEXUS_ADDR`) using short connect/read/
/// write timeouts so a wedged server fails the probe quickly rather than
/// hanging the container runtime's healthcheck. Exits `0` when the response's
/// status line contains `200`; exits `1` on any connection, I/O, or non-200
/// failure.
fn run_healthcheck() -> ! {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let port = healthcheck_port_from_env(std::env::var("NEXUS_ADDR").ok().as_deref());
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();
    let timeout = Duration::from_secs(3);

    let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("healthcheck: failed to connect to {addr}: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = stream.set_read_timeout(Some(timeout)) {
        eprintln!("healthcheck: failed to set read timeout: {e}");
        std::process::exit(1);
    }
    if let Err(e) = stream.set_write_timeout(Some(timeout)) {
        eprintln!("healthcheck: failed to set write timeout: {e}");
        std::process::exit(1);
    }

    let request = b"GET /health HTTP/1.0\r\nHost: localhost\r\n\r\n";
    if let Err(e) = stream.write_all(request) {
        eprintln!("healthcheck: failed to send request: {e}");
        std::process::exit(1);
    }

    let mut buf = [0u8; 512];
    let bytes_read = match stream.read(&mut buf) {
        Ok(n) => n,
        Err(e) => {
            eprintln!("healthcheck: failed to read response: {e}");
            std::process::exit(1);
        }
    };

    let response = String::from_utf8_lossy(&buf[..bytes_read]);
    let status_line = response.lines().next().unwrap_or("");
    if status_line.contains("200") {
        std::process::exit(0);
    }

    eprintln!("healthcheck: unhealthy response: {status_line}");
    std::process::exit(1);
}

/// Best-effort removal of stale comparison-graph temp directories left by
/// prior server processes that exited without running `Drop` (`kill -9`,
/// crashes). Comparison graphs self-remove on graceful shutdown; this only
/// reclaims orphans. A directory still open by a live process fails
/// `remove_dir_all` and is skipped, and only entries older than one hour are
/// considered so a concurrently-starting sibling server is never touched.
fn sweep_stale_comparison_dirs() {
    let base = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&base) else {
        return;
    };
    let now = std::time::SystemTime::now();
    const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(3600);
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with("nexus-cmp-") {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|mtime| now.duration_since(mtime).unwrap_or_default() > STALE_AFTER)
            .unwrap_or(false);
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn main() -> anyhow::Result<()> {
    // Parse CLI arguments
    let args = Args::parse();

    // `--healthcheck` is a one-shot probe, not a server boot: run it before
    // any Tokio runtime / server startup work so it stays cheap and fast
    // even inside a `FROM scratch` container with no shell.
    if args.healthcheck {
        run_healthcheck();
    }

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

    // Security preflight: refuse to boot in an unsafe posture (public bind with
    // auth off; default root password with auth on). Intentional hard failure.
    if let Err(msg) = config.security_preflight() {
        tracing::error!("{msg}");
        anyhow::bail!("{msg}");
    }

    // Reclaim comparison-graph temp dirs orphaned by a prior process that
    // exited without running `Drop` (crash / `kill -9`).
    sweep_stale_comparison_dirs();

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
        // Hash password with Argon2id (per-user random salt)
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

    // Hub integration (phase5_hub-integration §1).
    // `HubClient::from_env()` returns `Ok(None)` when the operator
    // hasn't configured `HIVEHUB_CLOUD_BASE_URL` (single-tenant
    // standalone mode). With it configured, a missing API key is a
    // hard startup error; a probe failure is logged but not fatal so
    // a transient Hub outage doesn't take the server down.
    let hub_client = match nexus_server::hub::HubClient::from_env() {
        Ok(Some(client)) => {
            let probe = client.ping().await;
            info!(
                target: "nexus_server::hub",
                base_url = %client.base_url(),
                status = ?probe,
                "Hub liveness probe completed"
            );
            Some(client)
        }
        Ok(None) => None,
        Err(e) => {
            return Err(anyhow::anyhow!("Hub integration misconfigured: {e}"));
        }
    };
    // `hub_client` is consumed below by the Hub auth middleware
    // wiring (§2). §3-§7 wire on top of the same handle.

    // Create Nexus server state. Construct mutably so we can stash
    // boot-resolved config (encryption-at-rest fingerprint) before
    // sharing the handle as `Arc`.
    let mut nexus_server_owned = NexusServer::new(
        executor.clone(),
        engine_arc,
        database_manager_arc,
        rbac_arc,
        auth_manager.clone(),
        jwt_manager.clone(),
        audit_logger.clone(),
        config.root_user.clone(),
    );
    // Boot-time encryption invariant: scan the data dir for the
    // EaR magic, reject mixed-mode and flag-mismatch databases.
    // Runs after engine + database-manager init so a fresh boot
    // (where the engine creates zero-byte files) classifies cleanly
    // as "empty"; the engine itself does not write through the
    // encrypted page stream yet (storage-layer wiring is a separate
    // track), so the only way to see encrypted files today is for
    // the operator to have laid them down out-of-band — which is
    // exactly what the check is here to catch.
    let encryption_cfg = config::enforce_data_dir_invariants(
        config.encryption.clone(),
        std::path::Path::new(&data_dir),
    )?;
    nexus_server_owned.set_encryption_config(encryption_cfg.clone());
    if encryption_cfg.enabled {
        if let Some(fp) = encryption_cfg.fingerprint.as_deref() {
            info!(
                fingerprint = fp,
                source = ?encryption_cfg.source,
                inventory = ?encryption_cfg.inventory,
                "encryption-at-rest: master key resolved"
            );
        }
    } else if let Some(inv) = &encryption_cfg.inventory {
        info!(
            inventory = ?inv,
            "encryption-at-rest: disabled — data directory inventory clean"
        );
    }
    let nexus_server = Arc::new(nexus_server_owned);

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
    // Held for the process lifetime: dropping the Thunder `ListenerHandle`
    // triggers a graceful shutdown, so this binding must outlive the HTTP
    // serve below.
    let _rpc_handle = if config.rpc.enabled {
        match nexus_server::protocol::rpc::spawn_rpc_listener(
            nexus_server.clone(),
            config.rpc.addr,
            config.rpc.clone(),
            config.rpc.require_auth,
        )
        .await
        {
            Ok(handle) => {
                info!(
                    "Nexus RPC listener bound on {} (auth_required={}, max_frame_bytes={})",
                    config.rpc.addr, config.rpc.require_auth, config.rpc.max_frame_bytes
                );
                Some(handle)
            }
            Err(e) => {
                warn!(
                    "Failed to bind RPC listener on {}: {}. HTTP/RESP3 surfaces continue unaffected.",
                    config.rpc.addr, e
                );
                None
            }
        }
    } else {
        None
    };

    // Hoisted above `create_mcp_router` so both the MCP and main
    // routers see the same cluster flag. Legacy auth stays wired
    // up through `auth.enabled`; cluster mode piggy-backs on it.
    let cluster_enabled = config.cluster.enabled;

    // rmcp's `StreamableHttpServerConfig` default `allowed_hosts` list
    // (localhost/127.0.0.1/::1) 403s any other `Host` header. A server
    // bound to a non-loopback address with no allow-list configured will
    // silently reject every remote MCP client until an operator notices
    // the 403s — warn at startup instead so the gap surfaces immediately.
    if !config.addr.ip().is_loopback()
        && config.mcp_allowed_hosts.is_empty()
        && !config.mcp_allowed_hosts_disable
    {
        warn!(
            "/mcp is bound on non-loopback address {} but NEXUS_MCP_ALLOWED_HOSTS is unset; \
             rmcp's DNS-rebinding guard will return 403 for any Host header other than \
             localhost/127.0.0.1/::1. Set NEXUS_MCP_ALLOWED_HOSTS (comma-separated hostnames \
             or host:port authorities) to the values remote clients will send, or \
             NEXUS_MCP_ALLOWED_HOSTS_DISABLE=true to disable the check (not recommended for \
             public deployments).",
            config.addr
        );
    }

    // Create MCP router with StreamableHTTP transport
    let mcp_router = create_mcp_router(
        nexus_server.clone(),
        cluster_enabled,
        config.mcp_allowed_hosts.clone(),
        config.mcp_allowed_hosts_disable,
    )
    .await?;

    // Health + Prometheus now read `server.start_time` and
    // `server.metrics` via State<Arc<NexusServer>> (phase2e); the
    // `api::health::init` + `api::prometheus::init` bootstrap pair
    // that used to live here is gone.

    // The comparison graphs + correlation manager + UMICP handler are
    // owned by NexusServer::new (phase2d), so the `init_graphs` /
    // `init_manager` scaffolding that used to live here is gone.

    // Initialize the per-IP rate limiter. Layered onto `app` below (H2);
    // relies on `ConnectInfo<SocketAddr>` being available, which requires
    // serving through `into_make_service_with_connect_info`. Sourced from
    // `config.rate_limit` (NEXUS_RATE_LIMIT_* env vars) so operators can
    // tune the budget or disable it, and so loopback clients are exempt by
    // default — fixes bulk `/ingest` loads getting their connection reset
    // instead of a clean 429 once the token bucket empties.
    let rate_limiter = RateLimiter::with_config(config.rate_limit.clone());

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
            config.auth.require_stats_auth,
            cluster_enabled,
        ))
    } else {
        None
    };

    // Build main router. Route table + MCP nesting + the auth-disabled
    // context layer live in `routes::build_router` (pure mechanical
    // extraction; see that module for the full route list).
    let mut app = routes::build_router(nexus_server.clone(), mcp_router, config.auth.enabled);

    // Global admission queue — caps concurrent engine-facing work so a
    // single client's burst can't wedge the process. Light-weight
    // endpoints (/health, /prometheus, /auth, …) bypass the queue via
    // `is_heavy_path`; only /cypher, /ingest, /knn_traverse, /graphql,
    // /umicp actually acquire a permit. Configurable via
    // NEXUS_ADMISSION_* env vars.
    app = app.layer(axum_middleware::from_fn_with_state(
        nexus_server.admission.clone(),
        nexus_server::middleware::admission_middleware_handler,
    ));

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

    // V2 sharding bootstrap (opt-in via NEXUS_SHARDING_MODE). When
    // enabled, we spin a metadata Raft group over the TCP transport
    // and install a ClusterController the /cluster/* endpoints will
    // serve from. Disabled → no-op; standalone deployments keep the
    // pre-V2 behaviour byte-identical.
    match nexus_server::cluster_bootstrap::parse_sharding_env() {
        Ok(None) => {
            tracing::debug!("V2 sharding disabled (NEXUS_SHARDING_MODE unset)");
        }
        Ok(Some(sharding_cfg)) => {
            let rt_handle = tokio::runtime::Handle::current();
            match nexus_server::cluster_bootstrap::bootstrap_sharding(sharding_cfg, rt_handle).await
            {
                Ok(Some(handle)) => {
                    tracing::info!("V2 sharding bootstrapped; controller installed on NexusServer");
                    nexus_server
                        .set_cluster_controller(Some(handle.controller.clone()))
                        .await;
                    // Leak the handle for the lifetime of the process
                    // so its driver + listener tasks stay alive. A
                    // graceful shutdown path reclaims it in a future
                    // iteration; for now the process-level Ctrl+C
                    // aborts the tokio runtime which cleans up tasks.
                    std::mem::forget(handle);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!("V2 sharding bootstrap failed: {e}");
                    return Err(anyhow::anyhow!("sharding bootstrap: {e}"));
                }
            }
        }
        Err(e) => {
            tracing::error!("Invalid NEXUS_SHARDING_* env configuration: {e}");
            return Err(anyhow::anyhow!("sharding env config: {e}"));
        }
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

    // Hub access-key middleware (phase5_hub-integration §2). When the
    // operator configured Hub integration the middleware enforces the
    // gateway-set `X-Hivehub-User-Id` header and inserts a
    // `UserContext` into request extensions; in standalone mode it
    // is a pass-through so single-tenant deployments keep working.
    let hub_middleware_state: Arc<Option<nexus_server::hub::HubClient>> = Arc::new(hub_client);
    app = app.layer(axum_middleware::from_fn_with_state(
        hub_middleware_state,
        nexus_server::hub::hub_auth_middleware,
    ));

    // Add GraphQL playground route in debug builds
    #[cfg(debug_assertions)]
    let app = app.route("/graphql/playground", get(api::graphql::graphql_playground));

    // M4: build the CORS layer from the configured allow-list instead of
    // `CorsLayer::permissive()` (which reflected any `Origin`, letting any
    // website read API responses cross-origin). Empty list (default) => no
    // cross-origin access; see `middleware::cors::build_cors_layer`.
    let cors_layer = nexus_server::middleware::build_cors_layer(&config.cors_allowed_origins);

    // Apply middleware layers
    let app = app
        // Cap request body size. Without this, Axum allows bodies up to its
        // internal default (2 MB) but we want the value to come from config
        // so ops can tune it per deployment. A single oversized POST must
        // not be able to exhaust the server allocator.
        .layer(DefaultBodyLimit::max(config.max_body_size_bytes))
        // Compression for responses (gzip, deflate, br)
        .layer(CompressionLayer::new())
        // CORS — restricted to the configured allow-list (M4).
        .layer(cors_layer)
        // H4: per-request wall-clock timeout, so a slowloris-style connection
        // or a request that never completes cannot pin a worker indefinitely.
        // Covers the HTTP/connection vector; CPU-bound statement cancellation
        // inside the executor is a separate follow-up.
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            std::time::Duration::from_secs(config.request_timeout_secs),
        ))
        // Per-IP rate limiting (H2). Reads `ConnectInfo<SocketAddr>`, which
        // requires serving through `into_make_service_with_connect_info`
        // below.
        .layer(axum_middleware::from_fn_with_state(
            rate_limiter.clone(),
            nexus_server::middleware::rate_limit::rate_limit_middleware,
        ))
        // Request/response tracing
        .layer(TraceLayer::new_for_http());

    // phase9_store-lock-read-concurrency §1 — when NEXUS_PERF_PROBE=1,
    // periodically dump the diagnostic lock/wait counters
    // (nexus_core::perf_probe) to stderr so a bench run's server log
    // captures a ranked cost breakdown alongside the qps numbers. A
    // no-op background task otherwise (single env-var check, then the
    // task exits immediately without spawning a timer).
    if nexus_core::perf_probe::enabled() {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            loop {
                interval.tick().await;
                eprintln!("{}", nexus_core::perf_probe::render_snapshot());
            }
        });
    }

    // Start server with optimized configuration for high concurrency
    let listener = TcpListener::bind(&config.addr).await?;
    info!("Nexus Server listening on {}", config.addr);

    tracing::debug!("Starting optimized Axum server with high concurrency settings");

    // Start server. `into_make_service_with_connect_info` is required so the
    // rate-limit middleware's `ConnectInfo<SocketAddr>` extractor resolves.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
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

    #[test]
    fn test_healthcheck_port_from_env_default_when_none() {
        assert_eq!(healthcheck_port_from_env(None), 15474);
    }

    #[test]
    fn test_healthcheck_port_from_env_valid_host_port() {
        assert_eq!(healthcheck_port_from_env(Some("0.0.0.0:15474")), 15474);
        assert_eq!(healthcheck_port_from_env(Some("192.168.1.5:8080")), 8080);
    }

    #[test]
    fn test_healthcheck_port_from_env_garbage_falls_back_to_default() {
        assert_eq!(healthcheck_port_from_env(Some("not-an-address")), 15474);
        assert_eq!(healthcheck_port_from_env(Some("")), 15474);
    }

    #[test]
    fn test_healthcheck_port_from_env_ipv6_style_takes_last_segment() {
        assert_eq!(healthcheck_port_from_env(Some("[::]:15474")), 15474);
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
        let result = create_mcp_router(server, false, Vec::new(), false).await;
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
        let mcp_router_result = create_mcp_router(server.clone(), false, Vec::new(), false).await;
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
