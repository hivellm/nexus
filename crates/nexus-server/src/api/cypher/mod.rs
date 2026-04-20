//! Cypher query execution endpoint. Façade with submodules:
//! - `execute` — the main `execute_cypher` HTTP handler.
//! - `commands` — admin commands (database, user, query management, API key).
//! - `tests` — integration tests.

pub mod commands;
pub mod execute;

#[cfg(test)]
mod tests;

pub(crate) use commands::{
    execute_api_key_commands, execute_database_commands, execute_query_management_commands,
    execute_user_commands,
};
pub use execute::execute_cypher;

use crate::NexusServer;
use axum::extract::{Extension, Json, State};
use nexus_core::auth::{Permission, middleware::AuthContext};
use nexus_core::executor::parser::PropertyMap;
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

/// Helper function to convert Expression to JSON Value
fn expression_to_json_value(expr: &nexus_core::executor::parser::Expression) -> serde_json::Value {
    match expr {
        nexus_core::executor::parser::Expression::Literal(lit) => match lit {
            nexus_core::executor::parser::Literal::String(s) => {
                serde_json::Value::String(s.clone())
            }
            nexus_core::executor::parser::Literal::Integer(i) => {
                serde_json::Value::Number((*i).into())
            }
            nexus_core::executor::parser::Literal::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            nexus_core::executor::parser::Literal::Boolean(b) => serde_json::Value::Bool(*b),
            nexus_core::executor::parser::Literal::Null => serde_json::Value::Null,
            nexus_core::executor::parser::Literal::Point(p) => p.to_json_value(),
        },
        nexus_core::executor::parser::Expression::PropertyAccess {
            variable: _,
            property: _,
        } => {
            tracing::warn!("expression_to_json_value: Property expression not supported in CREATE");
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Variable(_) => {
            tracing::warn!("expression_to_json_value: Variable expression not supported in CREATE");
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Parameter(_) => {
            tracing::warn!(
                "expression_to_json_value: Parameter expression not supported in CREATE"
            );
            serde_json::Value::Null
        }
        nexus_core::executor::parser::Expression::Map(map) => {
            // This is a nested map expression - convert it
            let mut result = serde_json::Map::new();
            for (key, expr) in map {
                result.insert(key.clone(), expression_to_json_value(expr));
            }
            serde_json::Value::Object(result)
        }
        _ => {
            tracing::warn!(
                "expression_to_json_value: Unsupported expression type: {:?}",
                expr
            );
            serde_json::Value::Null
        }
    }
}

fn property_map_to_json(property_map: &Option<PropertyMap>) -> serde_json::Value {
    let mut props = serde_json::Map::new();

    if let Some(prop_map) = property_map {
        for (key, expr) in &prop_map.properties {
            let value = expression_to_json_value(expr);
            props.insert(key.clone(), value);
        }
    }

    serde_json::Value::Object(props)
}

fn ensure_node_from_pattern(
    engine: &mut nexus_core::Engine,
    node_pattern: &nexus_core::executor::parser::NodePattern,
    variable_context: &mut HashMap<String, Vec<u64>>,
) -> Result<Vec<u64>, String> {
    if let Some(var_name) = &node_pattern.variable {
        if let Some(existing) = variable_context.get(var_name) {
            if !existing.is_empty() {
                return Ok(existing.clone());
            }
        }
    }

    let properties = property_map_to_json(&node_pattern.properties);

    match engine.create_node(node_pattern.labels.clone(), properties) {
        Ok(node_id) => {
            if let Some(var_name) = &node_pattern.variable {
                variable_context
                    .entry(var_name.clone())
                    .or_default()
                    .push(node_id);
            }
            Ok(vec![node_id])
        }
        Err(e) => Err(format!("Failed to create node: {}", e)),
    }
}

fn create_relationship_from_pattern(
    engine: &mut nexus_core::Engine,
    rel_pattern: &nexus_core::executor::parser::RelationshipPattern,
    source_ids: &[u64],
    target_ids: &[u64],
) -> Result<(), String> {
    if source_ids.is_empty() || target_ids.is_empty() {
        return Ok(());
    }

    let rel_type = rel_pattern
        .types
        .first()
        .cloned()
        .unwrap_or_else(|| "RELATIONSHIP".to_string());

    let properties = property_map_to_json(&rel_pattern.properties);

    let mut create_edge = |from: u64, to: u64| match engine.create_relationship(
        from,
        to,
        rel_type.clone(),
        properties.clone(),
    ) {
        Ok(_rel_id) => Ok(()),
        Err(e) => Err(format!("Failed to create relationship: {}", e)),
    };

    match rel_pattern.direction {
        nexus_core::executor::parser::RelationshipDirection::Outgoing => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(from, to)?;
                }
            }
        }
        nexus_core::executor::parser::RelationshipDirection::Incoming => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(to, from)?;
                }
            }
        }
        nexus_core::executor::parser::RelationshipDirection::Both => {
            for &from in source_ids {
                for &to in target_ids {
                    create_edge(from, to)?;
                    create_edge(to, from)?;
                }
            }
        }
    }

    Ok(())
}

/// Cypher query request
#[derive(Debug, Deserialize)]
pub struct CypherRequest {
    /// Cypher query string
    pub query: String,
    /// Query parameters
    #[serde(default)]
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
/// procedures tracker. Returns the generated `connection_id`, which
/// the handler reuses as the `query_id` for the subsequent
/// `mark_query_completed` call.
fn register_connection_and_query_fallback(
    server: &NexusServer,
    query: &str,
    client_address: &str,
    auth_context: &Option<AuthContext>,
) -> String {
    let tracker = server.dbms_procedures.get_connection_tracker();
    let username = auth_context
        .as_ref()
        .and_then(|ctx| ctx.api_key.user_id.clone());
    let connection_id = tracker.register_connection(username, client_address.to_string());
    let _query_id = tracker.register_query(connection_id.clone(), query.to_string());
    connection_id
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
fn mark_query_completed(server: &NexusServer, query_id: &str) {
    server
        .dbms_procedures
        .get_connection_tracker()
        .complete_query(query_id);
}
