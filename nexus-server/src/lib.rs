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

// use config::Config;

/// Nexus server state
#[derive(Clone)]
pub struct NexusServer {
    /// Executor for Cypher queries
    pub executor: Arc<RwLock<nexus_core::executor::Executor>>,
    /// Catalog for metadata
    pub catalog: Arc<RwLock<nexus_core::catalog::Catalog>>,
    /// Label index
    pub label_index: Arc<RwLock<nexus_core::index::LabelIndex>>,
    /// KNN index
    pub knn_index: Arc<RwLock<nexus_core::index::KnnIndex>>,
}

impl NexusServer {
    /// Create a new Nexus server instance
    pub fn new(
        executor: Arc<RwLock<nexus_core::executor::Executor>>,
        catalog: Arc<RwLock<nexus_core::catalog::Catalog>>,
        label_index: Arc<RwLock<nexus_core::index::LabelIndex>>,
        knn_index: Arc<RwLock<nexus_core::index::KnnIndex>>,
    ) -> Self {
        Self {
            executor,
            catalog,
            label_index,
            knn_index,
        }
    }
}
