//! Property keys management API
//!
//! Provides endpoints for listing and analyzing property keys:
//! - GET /management/property-keys - List all property keys with statistics

use axum::{
    extract::State,
    response::{IntoResponse, Json, Response},
};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Server state with engine
pub struct PropertyKeysState {
    /// Graph engine
    pub engine: Arc<RwLock<nexus_core::Engine>>,
}

/// Property key information with usage statistics
#[derive(Debug, Serialize)]
pub struct PropertyKeyInfo {
    /// Property key name
    pub name: String,
    /// Number of nodes using this property
    pub node_count: u64,
    /// Number of relationships using this property
    pub relationship_count: u64,
    /// Total usage count
    pub total_count: u64,
    /// Data types observed (string, number, boolean, etc.)
    pub types: Vec<String>,
}

/// Response for property keys list
#[derive(Debug, Serialize)]
pub struct PropertyKeysResponse {
    /// List of property keys with statistics
    pub property_keys: Vec<PropertyKeyInfo>,
    /// Total number of unique keys
    pub total_keys: usize,
}

/// List all property keys with usage statistics
pub async fn list_property_keys(State(state): State<PropertyKeysState>) -> Response {
    let engine = state.engine.read().await;

    // Get all property keys from catalog
    let catalog = &engine.catalog;
    let keys: Vec<String> = catalog
        .list_all_keys()
        .into_iter()
        .map(|(_, name)| name)
        .collect();

    // Build property key info with basic statistics
    // Note: Full statistics would require scanning all nodes/relationships
    let property_keys: Vec<PropertyKeyInfo> = keys
        .into_iter()
        .map(|name| PropertyKeyInfo {
            name: name.clone(),
            node_count: 0,         // TODO: Implement full scan for statistics
            relationship_count: 0, // TODO: Implement full scan for statistics
            total_count: 0,
            types: vec![], // TODO: Track types during scan
        })
        .collect();

    let total_keys = property_keys.len();

    Json(PropertyKeysResponse {
        property_keys,
        total_keys,
    })
    .into_response()
}

/// Get property key statistics by analyzing the graph
pub async fn get_property_key_stats(State(state): State<PropertyKeysState>) -> Response {
    let engine = state.engine.read().await;

    // Scan graph to collect property key statistics
    // This is an expensive operation for large graphs
    let mut key_stats: HashMap<String, PropertyKeyInfo> = HashMap::new();

    // TODO: Implement full graph scan
    // For now, return catalog keys
    let catalog = &engine.catalog;
    let keys = catalog.list_all_keys();

    for (_, name) in keys {
        key_stats.insert(
            name.clone(),
            PropertyKeyInfo {
                name,
                node_count: 0,
                relationship_count: 0,
                total_count: 0,
                types: vec![],
            },
        );
    }

    let property_keys: Vec<PropertyKeyInfo> = key_stats.into_values().collect();
    let total_keys = property_keys.len();

    Json(PropertyKeysResponse {
        property_keys,
        total_keys,
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_core::Engine;
    use tempfile::TempDir;

    async fn create_test_state() -> PropertyKeysState {
        let dir = TempDir::new().unwrap();
        let engine = Engine::with_data_dir(dir.path()).unwrap();
        PropertyKeysState {
            engine: Arc::new(RwLock::new(engine)),
        }
    }

    #[tokio::test]
    async fn test_list_property_keys() {
        let state = create_test_state().await;

        let response = list_property_keys(State(state)).await;

        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn test_get_property_key_stats() {
        let state = create_test_state().await;

        let response = get_property_key_stats(State(state)).await;

        assert_eq!(response.status(), 200);
    }
}
