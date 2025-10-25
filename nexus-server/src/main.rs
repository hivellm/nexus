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
    extract::{Json, Request},
    response::IntoResponse,
    routing::{any, delete, get, post, put},
};
use serde::Serialize;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod api;
mod config;

use config::Config;

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

    // Initialize executor
    let executor = api::cypher::init_executor()?;

    // Share executor with other modules
    api::knn::init_executor(executor.clone())?;
    api::ingest::init_executor(executor)?;

    // Initialize catalog and indexes for new endpoints
    let catalog = nexus_core::catalog::Catalog::new(tempdir()?)?;
    let catalog_arc = Arc::new(RwLock::new(catalog));

    let label_index = nexus_core::index::LabelIndex::new();
    let label_index_arc = Arc::new(RwLock::new(label_index));

    let knn_index = nexus_core::index::KnnIndex::new(128)?;
    let knn_index_arc = Arc::new(RwLock::new(knn_index));

    // Initialize new API modules
    api::schema::init_catalog(catalog_arc.clone())?;
    api::data::init_catalog(catalog_arc.clone())?;
    api::stats::init_instances(
        catalog_arc.clone(),
        label_index_arc.clone(),
        knn_index_arc.clone(),
    )?;

    // Create Nexus server state
    let nexus_server = Arc::new(NexusServer {
        executor: api::cypher::get_executor(),
        catalog: catalog_arc,
        label_index: label_index_arc,
        knn_index: knn_index_arc,
    });

    info!("Starting Nexus Server on {}", config.addr);

    // Create MCP router with StreamableHTTP transport
    let mcp_router = create_mcp_router(nexus_server.clone()).await?;

    // Build main router
    let app = Router::new()
        .route("/", get(health_check))
        .route("/health", get(health_check))
        .route("/cypher", post(api::cypher::execute_cypher))
        .route("/knn_traverse", post(api::knn::knn_traverse))
        .route("/ingest", post(api::ingest::ingest_data))
        // Schema management endpoints
        .route("/schema/labels", post(api::schema::create_label))
        .route("/schema/labels", get(api::schema::list_labels))
        .route("/schema/rel_types", post(api::schema::create_rel_type))
        .route("/schema/rel_types", get(api::schema::list_rel_types))
        // Data management endpoints
        .route("/data/nodes", post(api::data::create_node))
        .route("/data/relationships", post(api::data::create_rel))
        .route("/data/nodes", put(api::data::update_node))
        .route("/data/nodes", delete(api::data::delete_node))
        // Statistics endpoint
        .route("/stats", get(api::stats::get_stats))
        // SSE streaming endpoints
        .route("/sse/cypher", get({
            let server = nexus_server.clone();
            move |query| api::streaming::stream_cypher_query(query, server)
        }))
        .route("/sse/stats", get({
            let server = nexus_server.clone();
            move |query| api::streaming::stream_stats(query, server)
        }))
        .route("/sse/heartbeat", get(api::streaming::stream_heartbeat))
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

/// Health check endpoint
async fn health_check() -> impl IntoResponse {
    #[derive(Serialize)]
    struct HealthResponse {
        status: &'static str,
        version: &'static str,
    }

    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.addr.port(), 15474);
    }
}
