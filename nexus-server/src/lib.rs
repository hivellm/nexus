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

use std::sync::Arc;
use tokio::sync::RwLock;

pub mod api;
pub mod config;
pub mod middleware;

use config::RootUserConfig;

/// Nexus server state
#[derive(Clone)]
pub struct NexusServer {
    /// Executor for Cypher queries
    pub executor: Arc<RwLock<nexus_core::executor::Executor>>,
    /// Engine for all operations (contains Catalog, LabelIndex, KnnIndex, etc.)
    pub engine: Arc<RwLock<nexus_core::Engine>>,
    /// Database manager for multi-database support
    pub database_manager: Arc<RwLock<nexus_core::database::DatabaseManager>>,
    /// RBAC system for user management
    pub rbac: Arc<RwLock<nexus_core::auth::RoleBasedAccessControl>>,
    /// Authentication manager for API key management
    pub auth_manager: Arc<nexus_core::auth::AuthManager>,
    /// JWT manager for token generation and validation
    pub jwt_manager: Arc<nexus_core::auth::JwtManager>,
    /// Audit logger for security operations
    pub audit_logger: Arc<nexus_core::auth::AuditLogger>,
    /// Root user configuration
    pub root_user_config: RootUserConfig,
}

impl NexusServer {
    /// Create a new Nexus server instance
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        executor: Arc<RwLock<nexus_core::executor::Executor>>,
        engine: Arc<RwLock<nexus_core::Engine>>,
        database_manager: Arc<RwLock<nexus_core::database::DatabaseManager>>,
        rbac: Arc<RwLock<nexus_core::auth::RoleBasedAccessControl>>,
        auth_manager: Arc<nexus_core::auth::AuthManager>,
        jwt_manager: Arc<nexus_core::auth::JwtManager>,
        audit_logger: Arc<nexus_core::auth::AuditLogger>,
        root_user_config: RootUserConfig,
    ) -> Self {
        Self {
            executor,
            engine,
            database_manager,
            rbac,
            auth_manager,
            jwt_manager,
            audit_logger,
            root_user_config,
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
