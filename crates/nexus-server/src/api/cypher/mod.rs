//! Cypher query execution endpoint. Façade with submodules:
//! - `execute` — the main `execute_cypher` HTTP handler.
//! - `commands` — admin commands (database, user, query management, API key).
//! - `routing` — shared AST-predicate write/read routing decision (used by
//!   both this crate's HTTP handler and the RPC dispatcher).
//! - `tests` — integration tests.

pub mod commands;
pub mod execute;
pub(crate) mod routing;

#[cfg(test)]
mod schema_procedures_test;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod write_path_parity;

pub(crate) use commands::{
    execute_api_key_commands, execute_database_commands, execute_query_management_commands,
    execute_user_commands,
};
pub use execute::execute_cypher;

use crate::NexusServer;
use axum::extract::{Extension, Json, State};
use nexus_core::auth::{Permission, middleware::AuthContext};
use nexus_core::executor::{Executor, Query};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Build the server's shared `Executor` with the intelligent query cache
/// enabled. `main.rs` calls this once, wraps the result in an `Arc`, and
/// hands it to `NexusServer::new` — every Axum handler reads the same
/// `Arc` via the `State<Arc<NexusServer>>` extractor.
pub fn build_executor() -> anyhow::Result<Executor> {
    let mut executor = Executor::default();

    // Operator override for the cartesian-product memory budget. Absent env
    // var keeps the `ExecutorConfig` default (1 GiB); a present-but-invalid
    // value is a warning, not a fatal error — this is a server binary path
    // and must never panic on operator-supplied input.
    match std::env::var("NEXUS_CARTESIAN_PRODUCT_MAX_BYTES") {
        Ok(raw) => match raw.parse::<usize>() {
            Ok(max_bytes) if max_bytes > 0 => {
                executor.set_cartesian_product_max_bytes(max_bytes);
                tracing::info!(
                    "NEXUS_CARTESIAN_PRODUCT_MAX_BYTES applied: cartesian_product_max_bytes={} bytes",
                    max_bytes
                );
            }
            Ok(_) => {
                tracing::warn!(
                    "NEXUS_CARTESIAN_PRODUCT_MAX_BYTES=\"{}\" is zero; keeping the default cartesian_product_max_bytes",
                    raw
                );
            }
            Err(e) => {
                tracing::warn!(
                    "NEXUS_CARTESIAN_PRODUCT_MAX_BYTES=\"{}\" is not a valid usize ({}); keeping the default cartesian_product_max_bytes",
                    raw,
                    e
                );
            }
        },
        Err(std::env::VarError::NotPresent) => {}
        Err(std::env::VarError::NotUnicode(raw)) => {
            tracing::warn!(
                "NEXUS_CARTESIAN_PRODUCT_MAX_BYTES={:?} is not valid unicode; keeping the default cartesian_product_max_bytes",
                raw
            );
        }
    }

    // Enable intelligent query cache with default configuration
    let cache_config = nexus_core::query_cache::QueryCacheConfig {
        max_entries: 10000,
        max_memory_bytes: 512 * 1024 * 1024, // 512MB
        default_ttl: std::time::Duration::from_secs(3600), // 1 hour
        adaptive_ttl: true,
        min_ttl: std::time::Duration::from_secs(30), // 30 seconds
        max_ttl: std::time::Duration::from_secs(3600), // 1 hour
    };
    executor.enable_query_cache_with_config(cache_config.clone())?;
    tracing::info!(
        "Query cache enabled with config: max_entries={}, max_memory={}MB",
        cache_config.max_entries,
        cache_config.max_memory_bytes / (1024 * 1024)
    );

    Ok(executor)
}

/// Get query cache statistics
pub async fn get_cache_stats(
    State(server): State<Arc<NexusServer>>,
) -> impl axum::response::IntoResponse {
    let executor = server.executor.clone();
    if let Some(stats) = executor.get_query_cache_stats() {
        axum::Json(serde_json::json!({
            "cache_enabled": true,
            "lookups": stats.lookups,
            "hits": stats.hits,
            "misses": stats.misses,
            "hit_rate": stats.hit_rate,
            "memory_usage_bytes": stats.memory_usage_bytes,
            "ttl_evictions": stats.ttl_evictions,
            "size_evictions": stats.size_evictions,
            "avg_time_saved_ms": stats.avg_time_saved_ms
        }))
    } else {
        axum::Json(serde_json::json!({
            "cache_enabled": false,
            "message": "Query cache is not enabled"
        }))
    }
}

/// Clear query cache
#[derive(Deserialize)]
pub struct ClearCacheRequest {
    #[serde(default)]
    pub affected_labels: Vec<String>,
    #[serde(default)]
    pub affected_properties: Vec<String>,
}

/// Clear query cache endpoint
pub async fn clear_cache(
    State(server): State<Arc<NexusServer>>,
    Json(request): Json<ClearCacheRequest>,
) -> impl axum::response::IntoResponse {
    let executor = server.executor.clone();

    if request.affected_labels.is_empty() && request.affected_properties.is_empty() {
        // Clear entire cache
        executor.clear_query_cache();
        axum::Json(serde_json::json!({
            "success": true,
            "message": "Query cache cleared successfully"
        }))
    } else {
        // Invalidate by pattern
        let labels: Vec<&str> = request.affected_labels.iter().map(|s| s.as_str()).collect();
        let properties: Vec<&str> = request
            .affected_properties
            .iter()
            .map(|s| s.as_str())
            .collect();
        executor.invalidate_query_cache(&labels, &properties);
        axum::Json(serde_json::json!({
            "success": true,
            "message": format!("Cache invalidated for labels: {:?}, properties: {:?}", labels, properties)
        }))
    }
}

/// Clean expired cache entries
pub async fn clean_cache(
    State(server): State<Arc<NexusServer>>,
) -> impl axum::response::IntoResponse {
    let executor = server.executor.clone();
    executor.clean_query_cache();
    axum::Json(serde_json::json!({
        "success": true,
        "message": "Expired cache entries cleaned successfully"
    }))
}

/// Deserialize a parameter map that may be explicitly `null`.
///
/// The published SDK serializes `parameters` as explicit JSON `null` for
/// no-parameter queries. serde rejects `null` for a non-`Option` `HashMap`
/// (HTTP 422). Treat `null` (and a missing field, via `default`) as an empty
/// map, restoring 2.2.0 behaviour (issue #7).
fn deserialize_null_default<'de, D>(
    deserializer: D,
) -> std::result::Result<HashMap<String, serde_json::Value>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let opt = Option::<HashMap<String, serde_json::Value>>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

/// Cypher query request
#[derive(Debug, Deserialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters. Accepts both `params` and the Neo4j/SDK-standard
    /// `parameters` key (issue #3 — clients send `parameters`; without the
    /// alias serde silently dropped it and every parametrized query saw an
    /// empty map). Explicit JSON `null` is treated as an empty map (issue #7).
    #[serde(
        default,
        alias = "parameters",
        deserialize_with = "deserialize_null_default"
    )]
    pub params: HashMap<String, serde_json::Value>,
    /// Database name (optional, defaults to "neo4j")
    #[serde(default)]
    pub database: Option<String>,
}

/// Cypher query response
#[derive(Debug, Serialize)]
pub struct CypherResponse {
    /// Column names
    pub columns: Vec<String>,
    /// Result rows
    pub rows: Vec<serde_json::Value>,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Planner / executor notifications (Neo4j-shape: code, title,
    /// description, severity, category). Currently sourced from the
    /// `Nexus.Performance.UnindexedPropertyAccess` planner hint.
    /// Field is omitted from the wire format when empty so the hot
    /// path (no notifications) keeps the same byte count it had
    /// before phase6.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub notifications: Vec<nexus_core::executor::types::Notification>,
}

/// Record Prometheus metrics for query execution against the server's
/// Prometheus counter pack. The handler always has `Arc<NexusServer>`
/// in scope, so there is no global fallback.
fn record_prometheus_metrics(
    server: &NexusServer,
    execution_time_ms: u64,
    success: bool,
    cache_hit: bool,
) {
    server.metrics.record_query(success, execution_time_ms);
    if cache_hit {
        server.metrics.record_cache_hit();
    } else {
        server.metrics.record_cache_miss();
    }
}

/// Record a query execution against the server's `query_stats`. The
/// handler owns the `Arc<NexusServer>` and passes it through every
/// call site, so there is no fallback path — the stats struct is
/// always reachable.
#[allow(clippy::too_many_arguments)]
fn record_query_execution_with_metrics(
    server: &NexusServer,
    query: &str,
    execution_time: Duration,
    success: bool,
    error: Option<String>,
    rows_returned: usize,
    memory_usage: Option<u64>,
    cache_hits: Option<u64>,
    cache_misses: Option<u64>,
) {
    server.query_stats.record_query_with_metrics(
        query,
        execution_time,
        success,
        error,
        rows_returned,
        memory_usage,
        cache_hits,
        cache_misses,
    );
}

/// Register a connection and query against the server's DBMS
/// procedures tracker. Returns `(connection_id, guard)` where the
/// guard's `Drop` impl marks the query completed automatically — so
/// any panic, early return, or `?` propagation in the handler still
/// flips `is_running` back to `false`. Without the guard, an abnormal
/// exit leaks a "running" entry that pollutes both `SHOW QUERIES`
/// and the slow-query log tick until `cleanup_old_queries` reaps it
/// 10 minutes later.
fn register_connection_and_query_fallback(
    server: &NexusServer,
    query: &str,
    client_address: &str,
    auth_context: &Option<AuthContext>,
) -> (
    String,
    nexus_core::performance::connection_tracking::RegisteredQueryGuard,
) {
    let tracker = server.dbms_procedures.get_connection_tracker();
    let username = auth_context
        .as_ref()
        .and_then(|ctx| ctx.api_key.user_id.clone());
    let connection_id = tracker.register_connection(username, client_address.to_string());
    let guard = tracker.register_query_guarded(connection_id.clone(), query.to_string());
    (connection_id, guard)
}

/// Check plan-cache status for a query. Hashes the query text with
/// `DefaultHasher` and asks `server.plan_cache` whether that hash has
/// been seen; returns `(hits, misses)` suitable for
/// `QueryStatistics::record_query_with_metrics`.
fn check_query_cache_status(server: &NexusServer, query: &str) -> (u64, u64) {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let query_hash = {
        let mut hasher = DefaultHasher::new();
        query.hash(&mut hasher);
        hasher.finish().to_string()
    };

    let (exists, _) = server.plan_cache.check_cache_status(&query_hash);
    if exists {
        let (hits, misses) = server.plan_cache.get_query_cache_metrics(&query_hash);
        (hits.max(1), misses)
    } else {
        (0, 1)
    }
}

/// Mark a query as completed in the DBMS connection tracker.
///
/// Kept for callers that need to mark a specific id as completed
/// outside the RAII guard's scope (TERMINATE QUERY surfaces, plus
/// any future code path that registers a query via the manual
/// `register_query` API rather than `register_query_guarded`).
#[allow(dead_code)]
fn mark_query_completed(server: &NexusServer, query_id: &str) {
    server
        .dbms_procedures
        .get_connection_tracker()
        .complete_query(query_id);
}
