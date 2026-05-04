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

use parking_lot::RwLock;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::RwLock as TokioRwLock;

pub mod api;
pub mod cluster_bootstrap;
pub mod config;
pub mod hub;
pub mod middleware;
pub mod protocol;

use config::RootUserConfig;

/// Nexus server state
#[derive(Clone)]
pub struct NexusServer {
    /// Executor for Cypher queries
    /// Executor is Clone and contains only Arc internally, so no RwLock needed
    pub executor: Arc<nexus_core::executor::Executor>,
    /// Engine for all operations (contains Catalog, LabelIndex, KnnIndex, etc.)
    pub engine: Arc<TokioRwLock<nexus_core::Engine>>,
    /// Database manager for multi-database support
    pub database_manager: Arc<RwLock<nexus_core::database::DatabaseManager>>,
    /// RBAC system for user management
    pub rbac: Arc<TokioRwLock<nexus_core::auth::RoleBasedAccessControl>>,
    /// Authentication manager for API key management
    pub auth_manager: Arc<nexus_core::auth::AuthManager>,
    /// JWT manager for token generation and validation
    pub jwt_manager: Arc<nexus_core::auth::JwtManager>,
    /// Audit logger for security operations
    pub audit_logger: Arc<nexus_core::auth::AuditLogger>,
    /// Root user configuration
    pub root_user_config: RootUserConfig,

    // ── Performance monitoring (phase2c) ────────────────────────────────
    /// Per-query statistics (duration, success/failure, pattern counters).
    /// Read by `api::performance` handlers and the cypher execute path.
    pub query_stats: Arc<nexus_core::performance::query_stats::QueryStatistics>,
    /// Plan cache for rewrite hits / misses per query hash.
    pub plan_cache: Arc<nexus_core::performance::plan_cache::QueryPlanCache>,
    /// DBMS procedures (connection + query tracker). A background cleanup
    /// task for its trackers is spawned by [`NexusServer::new`].
    pub dbms_procedures: Arc<nexus_core::performance::dbms_procedures::DbmsProcedures>,
    /// MCP tool statistics.
    pub mcp_tool_stats: Arc<nexus_core::performance::mcp_tool_stats::McpToolStatistics>,
    /// MCP tool response cache.
    pub mcp_tool_cache: Arc<nexus_core::performance::McpToolCache>,

    // ── Graph correlation + comparison + UMICP (phase2d) ────────────────
    /// Shared correlation-graph builder for `/correlation/graphs/*`
    /// handlers and the MCP `graph_correlation_*` tools. A `std::Mutex`
    /// — not a `tokio::sync::Mutex` — because
    /// `nexus_core::graph::correlation::GraphCorrelationManager` holds
    /// data that is not `Send` across `.await` points; every caller
    /// locks synchronously and releases before awaiting again.
    pub graph_correlation_manager:
        Arc<Mutex<nexus_core::graph::correlation::GraphCorrelationManager>>,
    /// First of the two graphs the `/comparison/*` endpoints diff.
    /// Wrapped in `Arc<Mutex<Graph>>` because `Graph` contains a
    /// `RefCell` internally (inherited from the record-store cache),
    /// so it is not `Sync`.
    pub graph_a: Arc<Mutex<nexus_core::Graph>>,
    /// Second of the two graphs the `/comparison/*` endpoints diff.
    pub graph_b: Arc<Mutex<nexus_core::Graph>>,
    /// UMICP dispatcher used by `POST /umicp/graph` — routes JSON-RPC
    /// style requests (`graph.generate`, `graph.analyze`, ...) onto the
    /// shared correlation subsystem.
    pub umicp_handler: Arc<crate::api::graph_correlation_umicp::GraphUmicpHandler>,

    // ── Observability (phase2e) ─────────────────────────────────────────
    /// Wall-clock instant the server was constructed. Used by
    /// `api::health::health_check` and `api::health::metrics` to report
    /// `uptime_seconds`. Scoped to a single `NexusServer` instance so
    /// two servers in the same process report independent uptimes.
    pub start_time: Instant,
    /// Prometheus counter pack read by `GET /prometheus`. The cypher
    /// execute path records query successes / failures + cache hits /
    /// misses on this handle.
    pub metrics: Arc<crate::api::prometheus::PrometheusMetrics>,

    /// Optional cluster-mode quota provider. `None` in standalone
    /// deployments. When set, the server wires it into two places
    /// that together constitute the write-path quota contract:
    ///
    /// 1. The Axum quota middleware uses it for per-request rate
    ///    limiting (429 + Retry-After + X-RateLimit-Remaining).
    /// 2. The inner `nexus_core::Engine` uses it for the
    ///    storage-quota pre-check and post-write usage accounting.
    ///
    /// Exposed as `Arc<dyn QuotaProvider>` so stats handlers can
    /// query per-tenant snapshots without needing access to the
    /// middleware state.
    pub quota_provider:
        Arc<tokio::sync::RwLock<Option<Arc<dyn nexus_core::cluster::QuotaProvider>>>>,

    /// V2 cluster controller — owns the sharded-cluster metadata and
    /// serves the `/cluster/*` API. `None` on standalone deployments
    /// (sharding disabled). When populated, the metadata Raft driver
    /// promotes / demotes this node via
    /// [`nexus_core::sharding::controller::ClusterController::set_leader`].
    pub cluster_controller:
        Arc<tokio::sync::RwLock<Option<Arc<nexus_core::sharding::controller::ClusterController>>>>,

    /// Global admission queue shared by every query-bearing endpoint
    /// (`/cypher`, `/ingest`, RPC `CYPHER`). Back-pressure layer on
    /// top of the per-key rate limiter and the per-connection RPC
    /// semaphore — without it, a single client can fan out
    /// unbounded Cypher through a single keep-alive connection and
    /// wedge the engine.
    pub admission: Arc<crate::middleware::AdmissionQueue>,

    /// Encryption-at-rest configuration as resolved at boot —
    /// provider source + key fingerprint, never the key bytes
    /// themselves. Surfaced via the `/admin/encryption/status`
    /// endpoint so operators can verify two replicas are using
    /// the same master key without leaking it. Standalone
    /// deployments leave this at the default (`enabled = false`).
    pub encryption_config: crate::config::EncryptionConfig,
}

impl NexusServer {
    /// Create a new Nexus server instance. Performance monitoring
    /// components are constructed internally with the same defaults the
    /// pre-phase2c `init_performance_monitoring(1000, 1000, 100, 10)` +
    /// `init_mcp_performance_monitoring(500, 1000, 3600, 100)` calls
    /// installed; `main.rs` no longer has to wire them separately.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        executor: Arc<nexus_core::executor::Executor>,
        engine: Arc<TokioRwLock<nexus_core::Engine>>,
        database_manager: Arc<RwLock<nexus_core::database::DatabaseManager>>,
        rbac: Arc<TokioRwLock<nexus_core::auth::RoleBasedAccessControl>>,
        auth_manager: Arc<nexus_core::auth::AuthManager>,
        jwt_manager: Arc<nexus_core::auth::JwtManager>,
        audit_logger: Arc<nexus_core::auth::AuditLogger>,
        root_user_config: RootUserConfig,
    ) -> Self {
        // Defaults preserved from the pre-phase2c init_* calls.
        let query_stats = Arc::new(nexus_core::performance::query_stats::QueryStatistics::new(
            1000, 1000,
        ));
        let plan_cache = Arc::new(nexus_core::performance::plan_cache::QueryPlanCache::new(
            100, 10,
        ));
        let dbms_procedures =
            Arc::new(nexus_core::performance::dbms_procedures::DbmsProcedures::new());
        let mcp_tool_stats =
            Arc::new(nexus_core::performance::mcp_tool_stats::McpToolStatistics::new(500, 1000));
        let mcp_tool_cache = Arc::new(nexus_core::performance::McpToolCache::new(3600, 100));

        // Graph correlation / comparison / UMICP state (phase2d).
        //
        // Each server owns two empty graphs rooted at fresh temp dirs;
        // the comparison endpoints diff these until callers overwrite
        // them. `new` stays infallible so test fixtures do not have to
        // plumb `Result` through — if temp-dir creation fails we fall
        // back to a fresh in-memory catalog that still produces a
        // structurally valid empty graph.
        let (graph_a, graph_b) = build_default_comparison_graphs();

        let graph_correlation_manager = Arc::new(Mutex::new(
            nexus_core::graph::correlation::GraphCorrelationManager::new(),
        ));
        let umicp_handler = Arc::new(crate::api::graph_correlation_umicp::GraphUmicpHandler::new());

        // Observability (phase2e): capture start time and a fresh
        // Prometheus counter pack per server instance. Process-wide
        // counters that are intentionally shared (RESP3 / RPC listener
        // stats) still live in their own modules; only the per-query
        // counters move here.
        let start_time = Instant::now();
        let metrics = Arc::new(crate::api::prometheus::PrometheusMetrics::new());

        // Periodic sweeper for the DBMS connection / query trackers.
        //
        // The Cypher handler calls `register_connection` on every request
        // and HTTP keep-alive / crashed clients rarely trigger a clean
        // `unregister_connection`, so without this task the tracker
        // HashMap grows monotonically under load (~520 B per request —
        // confirmed via jemalloc heap diff on fix/memory-leak-v1).
        const CLEANUP_INTERVAL_SECS: u64 = 60;
        const CONNECTION_IDLE_SECS: u64 = 300; // evict after 5 min idle
        const QUERY_MAX_AGE_SECS: u64 = 600; // forget completed queries after 10 min

        let tracker_for_cleanup = dbms_procedures.get_connection_tracker();
        tokio::spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_secs(CLEANUP_INTERVAL_SECS));
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip the immediate first tick.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                tracker_for_cleanup.cleanup_stale_connections(CONNECTION_IDLE_SECS);
                tracker_for_cleanup.cleanup_old_queries(QUERY_MAX_AGE_SECS);
            }
        });

        // Slow-query log tick (phase6_slow-query-log-and-active-queries §2).
        //
        // The completion log line `Query executed successfully in Nms`
        // only fires post-completion. When a query is wedged (the
        // 2026-05-04 cortex-nexus 100% CPU incident — a `MERGE` against
        // an unindexed property doing a full label scan), `docker logs`
        // stays silent because nothing completes. This tick scans the
        // active-query map every `NEXUS_SLOW_QUERY_TICK_MS` ms (default
        // 1000), and for any entry whose `elapsed >=
        // NEXUS_SLOW_QUERY_THRESHOLD_MS` (default 1000) emits a WARN
        // log so operators can identify the offender via `docker logs`
        // without needing the HTTP / Cypher introspection paths
        // (which themselves time out when the writer thread is
        // saturated).
        //
        // Per-query throttle: a query is logged once on the first
        // threshold crossing, then once per `NEXUS_SLOW_QUERY_REPEAT_SECS`
        // (default 30) for as long as it is still running. Without the
        // throttle, the tick would spam one line per second per
        // wedged query — useless. Setting `NEXUS_SLOW_QUERY_THRESHOLD_MS=0`
        // disables the tick entirely.
        let tick_ms = std::env::var("NEXUS_SLOW_QUERY_TICK_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1000);
        let threshold_ms = std::env::var("NEXUS_SLOW_QUERY_THRESHOLD_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(1000);
        let repeat_secs = std::env::var("NEXUS_SLOW_QUERY_REPEAT_SECS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(30);

        if threshold_ms > 0 && tick_ms > 0 {
            let tracker_for_slow = dbms_procedures.get_connection_tracker();
            tokio::spawn(async move {
                use std::collections::HashMap as StdHashMap;
                use std::time::{SystemTime, UNIX_EPOCH};

                // Per-query "last warned at" wall-clock seconds. Pruned
                // implicitly: queries that exit the running set are not
                // re-touched, so stale entries are evicted by the next
                // sweep that runs after the cleanup task drops the
                // entry below `QUERY_MAX_AGE_SECS`.
                let mut last_warned: StdHashMap<String, u64> = StdHashMap::new();
                let mut ticker = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                ticker.tick().await; // skip immediate first tick
                let threshold_secs = threshold_ms.div_ceil(1000);

                loop {
                    ticker.tick().await;
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let running = tracker_for_slow.get_running_queries();
                    // Keep `last_warned` bounded by the running set.
                    last_warned.retain(|qid, _| running.iter().any(|q| q.query_id == *qid));

                    for q in running {
                        let elapsed_secs = now.saturating_sub(q.started_at);
                        if elapsed_secs < threshold_secs {
                            continue;
                        }
                        let last = last_warned.get(&q.query_id).copied().unwrap_or(0);
                        if last == 0 || now.saturating_sub(last) >= repeat_secs {
                            // Truncate the query text so a 2 MB
                            // payload doesn't blow up `docker logs`.
                            let truncated: String = if q.query.len() > 512 {
                                let mut t: String = q.query.chars().take(512).collect();
                                t.push_str("…<<truncated>>");
                                t
                            } else {
                                q.query.clone()
                            };
                            tracing::warn!(
                                target: "nexus_server::slow_query",
                                query_id = %q.query_id,
                                connection_id = %q.connection_id,
                                elapsed_ms = elapsed_secs * 1000,
                                "slow query still running: {}",
                                truncated,
                            );
                            last_warned.insert(q.query_id, now);
                        }
                    }
                }
            });
        }

        Self {
            executor,
            engine,
            database_manager,
            rbac,
            auth_manager,
            jwt_manager,
            audit_logger,
            root_user_config,
            query_stats,
            plan_cache,
            dbms_procedures,
            mcp_tool_stats,
            mcp_tool_cache,
            graph_correlation_manager,
            graph_a,
            graph_b,
            umicp_handler,
            start_time,
            metrics,
            quota_provider: Arc::new(tokio::sync::RwLock::new(None)),
            cluster_controller: Arc::new(tokio::sync::RwLock::new(None)),
            admission: Arc::new(crate::middleware::AdmissionQueue::new(
                crate::middleware::AdmissionConfig::from_env(),
            )),
            // Default-disabled — main.rs overrides via
            // `set_encryption_config` after parsing the runtime
            // Config. Tests can leave this at the default.
            encryption_config: crate::config::EncryptionConfig::default(),
        }
    }

    /// Install the encryption-at-rest configuration resolved at
    /// boot. Called from `main.rs` after `Config::from_env`. The
    /// status endpoint reads from this field; nothing in the
    /// engine path consumes it yet (the storage hooks land in a
    /// separate follow-up).
    pub fn set_encryption_config(&mut self, cfg: crate::config::EncryptionConfig) {
        self.encryption_config = cfg;
    }

    /// Install (or clear) the V2 cluster controller. Called from the
    /// server bootstrap once sharding has started. Idempotent —
    /// passing `None` clears the controller.
    pub async fn set_cluster_controller(
        &self,
        controller: Option<Arc<nexus_core::sharding::controller::ClusterController>>,
    ) {
        *self.cluster_controller.write().await = controller;
    }

    /// Install a cluster-mode quota provider and plumb it through
    /// the inner `Engine` so storage-quota gating on writes uses
    /// the same provider the HTTP rate-limit middleware consults.
    /// Idempotent — replacing the provider with `None` clears it
    /// on both sides.
    pub async fn set_cluster_quota_provider(
        &self,
        provider: Option<Arc<dyn nexus_core::cluster::QuotaProvider>>,
    ) {
        *self.quota_provider.write().await = provider.clone();
        self.engine.write().await.set_quota_provider(provider);
    }

    /// Snapshot of a tenant's current rate / storage usage, if the
    /// provider knows the tenant. Returns `None` on standalone
    /// deployments (no provider wired) or for namespaces the
    /// provider has not seen yet.
    pub async fn tenant_quota_snapshot(
        &self,
        ns: &nexus_core::cluster::UserNamespace,
    ) -> Option<nexus_core::cluster::QuotaSnapshot> {
        let provider = self.quota_provider.read().await.clone()?;
        provider.snapshot(ns)
    }

    /// Check if root user should be disabled after first admin user creation
    /// Returns true if root was disabled, false otherwise
    pub async fn check_and_disable_root_if_needed(&self) -> bool {
        // Only disable if configured to do so
        if !self.root_user_config.disable_after_setup {
            return false;
        }

        // Check if root is currently enabled
        let rbac = self.rbac.read().await;
        if !rbac.is_root_enabled() {
            return false; // Root already disabled
        }
        drop(rbac);

        // Check if there's at least one admin user (non-root, active, with Admin permission)
        let rbac = self.rbac.read().await;
        let has_admin = rbac.list_users().iter().any(|user| {
            !user.is_root
                && user.is_active
                && rbac.user_has_permission(&user.id, &nexus_core::auth::Permission::Admin)
        });
        drop(rbac);

        if has_admin {
            // Disable root user
            let mut rbac = self.rbac.write().await;
            if let Err(e) = rbac.disable_root_user() {
                tracing::warn!("Failed to disable root user: {}", e);
                return false;
            }
            tracing::info!("Root user automatically disabled after first admin user creation");
            return true;
        }

        false
    }

    /// Start the expired API keys cleanup job
    /// Runs cleanup every hour (or specified interval)
    pub fn start_expired_keys_cleanup_job(
        auth_manager: Arc<nexus_core::auth::AuthManager>,
        interval_seconds: u64,
    ) {
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(tokio::time::Duration::from_secs(interval_seconds));

            loop {
                interval.tick().await;

                match auth_manager.cleanup_expired_keys() {
                    Ok(count) => {
                        if count > 0 {
                            tracing::info!("Cleaned up {} expired API keys", count);
                        } else {
                            tracing::debug!("No expired API keys to clean up");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to cleanup expired API keys: {}", e);
                    }
                }
            }
        });
    }
}

/// Build the two default comparison graphs the `/comparison/*` handlers
/// operate against. Each is a freshly-allocated `Graph` rooted at its
/// own `tempfile::tempdir` so the server starts with a well-defined
/// empty state. If temp-dir creation fails for whatever reason (read
/// only FS, exhausted handles, etc.) we fall back to a process-local
/// path under `std::env::temp_dir()` suffixed with a monotonic id; the
/// resulting graph is still structurally valid.
fn build_default_comparison_graphs()
-> (Arc<Mutex<nexus_core::Graph>>, Arc<Mutex<nexus_core::Graph>>) {
    fn build_one(label: &str) -> nexus_core::Graph {
        let dir = match tempfile::tempdir() {
            Ok(d) => d.keep(),
            Err(e) => {
                tracing::warn!(
                    "comparison graph '{}': tempdir() failed ({}), falling back to env::temp_dir",
                    label,
                    e
                );
                let fallback = std::env::temp_dir().join(format!(
                    "nexus-cmp-{}-{}",
                    label,
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or(0),
                ));
                std::fs::create_dir_all(&fallback).expect("fallback temp dir");
                fallback
            }
        };
        let store =
            nexus_core::storage::RecordStore::new(&dir).expect("RecordStore on fresh temp dir");
        let catalog = Arc::new(
            nexus_core::catalog::Catalog::new(dir.join("catalog"))
                .expect("Catalog on fresh temp dir"),
        );
        nexus_core::Graph::new(store, catalog)
    }

    (
        Arc::new(Mutex::new(build_one("a"))),
        Arc::new(Mutex::new(build_one("b"))),
    )
}
