//! Bulk data ingestion endpoint

use axum::extract::Json;
use serde::{Deserialize, Serialize};

/// Ingestion request (NDJSON format)
#[derive(Debug, Deserialize)]
pub struct IngestRequest {
    /// Nodes to ingest
    #[serde(default)]
    pub nodes: Vec<NodeIngest>,
    /// Relationships to ingest
    #[serde(default)]
    pub relationships: Vec<RelIngest>,
}

/// Node to ingest
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NodeIngest {
    /// Node ID (optional, auto-generated if not provided)
    pub id: Option<u64>,
    /// Labels
    pub labels: Vec<String>,
    /// Properties
    pub properties: serde_json::Value,
}

/// Relationship to ingest
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct RelIngest {
    /// Relationship ID (optional)
    pub id: Option<u64>,
    /// Source node ID
    pub src: u64,
    /// Destination node ID
    pub dst: u64,
    /// Relationship type
    pub r#type: String,
    /// Properties
    pub properties: serde_json::Value,
}

/// Ingestion response
#[derive(Debug, Serialize)]
pub struct IngestResponse {
    /// Number of nodes ingested
    pub nodes_ingested: usize,
    /// Number of relationships ingested
    pub relationships_ingested: usize,
    /// Ingestion time in milliseconds
    pub ingestion_time_ms: u64,
}

/// Ingest bulk data
pub async fn ingest_data(Json(request): Json<IngestRequest>) -> Json<IngestResponse> {
    // TODO: Implement bulk ingestion via nexus-core
    tracing::info!(
        "Ingesting {} nodes and {} relationships",
        request.nodes.len(),
        request.relationships.len()
    );

    Json(IngestResponse {
        nodes_ingested: request.nodes.len(),
        relationships_ingested: request.relationships.len(),
        ingestion_time_ms: 0,
    })
}
