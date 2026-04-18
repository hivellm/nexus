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
use nexus_core::executor::{Executor, ExecutorShared, Query};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock as TokioRwLock;

/// Global executor instance for concurrent execution
/// Executor is Clone and contains only Arc internally, so no RwLock needed
/// Multiple queries can clone the executor and execute in parallel
static EXECUTOR: std::sync::OnceLock<Arc<Executor>> = std::sync::OnceLock::new();

/// Global executor shared state for concurrent execution
/// Note: Currently not used - Executor is cloned directly from EXECUTOR
static _EXECUTOR_SHARED: std::sync::OnceLock<Arc<ExecutorShared>> = std::sync::OnceLock::new();

/// Global engine instance for CREATE operations
static ENGINE: std::sync::OnceLock<Arc<TokioRwLock<nexus_core::Engine>>> =
    std::sync::OnceLock::new();

/// Global database manager instance for multi-database support
static DATABASE_MANAGER: std::sync::OnceLock<Arc<RwLock<nexus_core::database::DatabaseManager>>> =
    std::sync::OnceLock::new();

/// Initialize the executor (deprecated - use init_engine_with_executor instead)
pub fn init_executor() -> anyhow::Result<Arc<Executor>> {
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

    let executor_arc = Arc::new(executor);
    EXECUTOR
        .set(executor_arc.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;
    Ok(executor_arc)
}

/// Initialize the engine
pub fn init_engine(engine: Arc<TokioRwLock<nexus_core::Engine>>) -> anyhow::Result<()> {
    ENGINE
        .set(engine.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set engine"))?;
    Ok(())
}

/// Initialize the database manager for multi-database support
pub fn init_database_manager(
    manager: Arc<RwLock<nexus_core::database::DatabaseManager>>,
) -> anyhow::Result<()> {
    DATABASE_MANAGER
        .set(manager.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set database manager"))?;

    // Set the database manager on the executor if it's already initialized
    if let Some(executor) = EXECUTOR.get() {
        executor
            .set_database_manager(manager)
            .map_err(|_| anyhow::anyhow!("Failed to set database manager on executor"))?;
    }

    tracing::info!("Multi-database support enabled");
    Ok(())
}

/// Get the database manager instance
pub fn get_database_manager()
-> Option<Arc<parking_lot::RwLock<nexus_core::database::DatabaseManager>>> {
    DATABASE_MANAGER.get().cloned()
}

/// Initialize both engine and executor with shared storage
pub fn init_engine_with_executor(
    engine: Arc<TokioRwLock<nexus_core::Engine>>,
) -> anyhow::Result<()> {
    // Set the engine
    ENGINE
        .set(engine.clone())
        .map_err(|_| anyhow::anyhow!("Failed to set engine"))?;

    // Create a wrapper for the executor that's inside the engine
    // We'll use a pattern where we access the engine's executor via the engine itself
    // For now, we'll still use a dummy executor for non-CREATE queries
    // The real solution is to make CREATE and MATCH both use the engine
    // Executor is Clone and contains only Arc internally, so no RwLock needed
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

    let executor_arc = Arc::new(executor);
    EXECUTOR
        .set(executor_arc)
        .map_err(|_| anyhow::anyhow!("Failed to set executor"))?;

    Ok(())
}

/// Get the executor instance
pub fn get_executor() -> Arc<Executor> {
    EXECUTOR.get().expect("Executor not initialized").clone()
}

/// Get query cache statistics
pub async fn get_cache_stats() -> impl axum::response::IntoResponse {
    let executor = get_executor();
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
    Json(request): Json<ClearCacheRequest>,
) -> impl axum::response::IntoResponse {
    let executor = get_executor();

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
pub async fn clean_cache() -> impl axum::response::IntoResponse {
    let executor = get_executor();
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

/// Helper function to record query execution for performance monitoring
#[allow(dead_code)]
fn record_query_execution(
    query: &str,
    execution_time: Duration,
    success: bool,
    error: Option<String>,
    rows_returned: usize,
) {
    record_query_execution_with_metrics(
        query,
        execution_time,
        success,
        error,
        rows_returned,
        None,
        None,
        None,
    );
}

/// Record Prometheus metrics for query execution
fn record_prometheus_metrics(execution_time_ms: u64, success: bool, cache_hit: bool) {
    if let Some(metrics) = crate::api::prometheus::METRICS.get() {
        metrics.record_query(success, execution_time_ms);
        if cache_hit {
            metrics.record_cache_hit();
        } else {
            metrics.record_cache_miss();
        }
    }
}

/// Helper function to record query execution with additional metrics
#[allow(clippy::too_many_arguments)]
fn record_query_execution_with_metrics(
    query: &str,
    execution_time: Duration,
    success: bool,
    error: Option<String>,
    rows_returned: usize,
    memory_usage: Option<u64>,
    cache_hits: Option<u64>,
    cache_misses: Option<u64>,
) {
    if let Some(stats) = crate::api::performance::get_query_stats() {
        stats.record_query_with_metrics(
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
}

/// Register connection and query for tracking (with SocketAddr)
#[allow(dead_code)]
fn register_connection_and_query(
    query: &str,
    addr: &std::net::SocketAddr,
    auth_context: &Option<AuthContext>,
) -> String {
    register_connection_and_query_fallback(query, &addr.to_string(), auth_context)
}

/// Register connection and query for tracking (fallback version)
fn register_connection_and_query_fallback(
    query: &str,
    client_address: &str,
    auth_context: &Option<AuthContext>,
) -> String {
    if let Some(dbms_procedures) = crate::api::performance::get_dbms_procedures() {
        let tracker = dbms_procedures.get_connection_tracker();
        let username = auth_context
            .as_ref()
            .and_then(|ctx| ctx.api_key.user_id.clone());

        // Register connection (or get existing)
        let connection_id = tracker.register_connection(username, client_address.to_string());

        // Register query
        let _query_id = tracker.register_query(connection_id.clone(), query.to_string());

        connection_id
    } else {
        // Fallback if DBMS procedures not initialized
        format!(
            "conn-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }
}

/// Check cache status for a query
fn check_query_cache_status(query: &str) -> (u64, u64) {
    if let Some(cache) = crate::api::performance::get_plan_cache() {
        // Normalize query to match cache pattern (simple approach)
        // In a real implementation, we'd use the same normalization as the optimizer
        let query_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            query.hash(&mut hasher);
            hasher.finish().to_string()
        };

        let (exists, _) = cache.check_cache_status(&query_hash);
        if exists {
            let (hits, misses) = cache.get_query_cache_metrics(&query_hash);
            (hits.max(1), misses) // At least 1 hit if exists
        } else {
            (0, 1) // Miss
        }
    } else {
        (0, 0) // Cache not available
    }
}

/// Mark query as completed in tracker
fn mark_query_completed(query_id: &str) {
    if let Some(dbms_procedures) = crate::api::performance::get_dbms_procedures() {
        let tracker = dbms_procedures.get_connection_tracker();
        tracker.complete_query(query_id);
    }
}
