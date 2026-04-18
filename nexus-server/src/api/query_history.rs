//! Query history API endpoint

use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Query history entry
#[derive(Debug, Clone, Serialize)]
pub struct QueryHistoryEntry {
    pub query: String,
    pub execution_time_ms: u64,
    pub timestamp: u64,
    pub success: bool,
    pub error: Option<String>,
    pub row_count: usize,
}

/// Query history state
#[derive(Clone)]
pub struct QueryHistoryState {
    pub history: Arc<RwLock<VecDeque<QueryHistoryEntry>>>,
}

impl QueryHistoryState {
    pub fn new() -> Self {
        Self {
            history: Arc::new(RwLock::new(VecDeque::with_capacity(1000))),
        }
    }

    pub async fn add_entry(&self, entry: QueryHistoryEntry) {
        let mut history = self.history.write().await;
        history.push_back(entry);
        if history.len() > 1000 {
            history.pop_front();
        }
    }
}

/// List query history response
#[derive(Debug, Serialize)]
pub struct QueryHistoryResponse {
    pub queries: Vec<QueryHistoryEntry>,
    pub total: usize,
}

/// Get query history
pub async fn get_query_history(
    axum::extract::State(state): axum::extract::State<QueryHistoryState>,
) -> Response {
    let history = state.history.read().await;
    let queries: Vec<QueryHistoryEntry> = history.iter().rev().take(100).cloned().collect();

    Json(QueryHistoryResponse {
        total: queries.len(),
        queries,
    })
    .into_response()
}
