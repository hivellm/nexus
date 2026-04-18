//! Index management API endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use nexus_core::Engine;

/// Server state with engine
#[derive(Clone)]
pub struct IndexState {
    pub engine: Arc<RwLock<Engine>>,
}

/// Index information
#[derive(Debug, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub label: String,
    pub properties: Vec<String>,
    pub index_type: String,
}

/// List indexes response
#[derive(Debug, Serialize)]
pub struct ListIndexesResponse {
    pub indexes: Vec<IndexInfo>,
}

/// Create index request
#[derive(Debug, Deserialize)]
pub struct CreateIndexRequest {
    pub label: String,
    pub properties: Vec<String>,
}

/// Create index response
#[derive(Debug, Serialize)]
pub struct CreateIndexResponse {
    pub success: bool,
    pub message: String,
    pub index_name: Option<String>,
}

/// List all indexes
pub async fn list_indexes(State(state): State<IndexState>) -> Response {
    let engine = state.engine.read().await;

    // For now, return empty list since index management is not fully implemented
    // This is a placeholder that returns the structure the GUI expects
    Json(ListIndexesResponse { indexes: vec![] }).into_response()
}

/// Create a new index
pub async fn create_index(
    State(state): State<IndexState>,
    Json(req): Json<CreateIndexRequest>,
) -> Response {
    let _engine = state.engine.read().await;

    // Placeholder implementation
    // TODO: Implement actual index creation when index management is added
    let index_name = format!("{}_{}", req.label, req.properties.join("_"));

    Json(CreateIndexResponse {
        success: true,
        message: format!("Index '{}' created successfully", index_name),
        index_name: Some(index_name),
    })
    .into_response()
}

/// Delete an index
pub async fn delete_index(State(_state): State<IndexState>, Path(name): Path<String>) -> Response {
    // Placeholder implementation
    // TODO: Implement actual index deletion when index management is added
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "message": format!("Index '{}' deleted successfully", name)
        })),
    )
        .into_response()
}
