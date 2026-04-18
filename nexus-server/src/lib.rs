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
use std::sync::Arc;
use tokio::sync::RwLock as TokioRwLock;

pub mod api;
pub mod config;
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
        }
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
