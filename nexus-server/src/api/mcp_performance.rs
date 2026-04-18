//! MCP Tool Performance Monitoring API endpoints
//!
//! Provides REST endpoints for:
//! - GET /mcp/performance/statistics - MCP tool execution statistics
//! - GET /mcp/performance/tools/{tool_name} - Statistics for specific tool
//! - GET /mcp/performance/slow-tools - Slow tool call log
//! - GET /mcp/performance/cache - Cache statistics
//! - POST /mcp/performance/cache/clear - Clear cache

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use nexus_core::performance::mcp_tool_cache::CacheStatistics;
use serde::Serialize;
use std::sync::Arc;

use crate::NexusServer;

/// MCP tool statistics response
#[derive(Debug, Serialize)]
pub struct McpToolStatisticsResponse {
    /// Overall statistics
    pub statistics: McpToolStatisticsSummary,
    /// Per-tool statistics
    pub tools: Vec<nexus_core::performance::ToolStats>,
}

/// MCP tool statistics summary response
#[derive(Debug, Serialize)]
pub struct McpToolStatisticsSummary {
    /// Total tool calls executed
    pub total_calls: u64,
    /// Successful tool calls
    pub successful_calls: u64,
    /// Failed tool calls
    pub failed_calls: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Number of slow tool calls logged
    pub slow_tool_count: usize,
}

/// Slow tool calls response
#[derive(Debug, Serialize)]
pub struct SlowToolCallsResponse {
    /// Slow tool records
    pub tools: Vec<SlowToolRecord>,
    /// Total count
    pub count: usize,
}

/// Slow tool record response
#[derive(Debug, Serialize)]
pub struct SlowToolRecord {
    /// Tool name
    pub tool_name: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp when tool was executed (Unix timestamp)
    pub timestamp: u64,
    /// Whether tool call succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get MCP tool statistics
/// GET /mcp/performance/statistics
pub async fn get_mcp_tool_statistics(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<McpToolStatisticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.mcp_tool_stats.clone();
    let summary = stats.get_statistics();
    let tool_stats = stats.get_all_tool_stats();

    Ok(Json(McpToolStatisticsResponse {
        statistics: McpToolStatisticsSummary {
            total_calls: summary.total_calls,
            successful_calls: summary.successful_calls,
            failed_calls: summary.failed_calls,
            total_execution_time_ms: summary.total_execution_time_ms,
            average_execution_time_ms: summary.average_execution_time_ms,
            min_execution_time_ms: summary.min_execution_time_ms,
            max_execution_time_ms: summary.max_execution_time_ms,
            slow_tool_count: summary.slow_tool_count,
        },
        tools: tool_stats,
    }))
}

/// Get statistics for a specific tool
/// GET /mcp/performance/tools/{tool_name}
pub async fn get_tool_statistics(
    State(server): State<Arc<NexusServer>>,
    axum::extract::Path(tool_name): axum::extract::Path<String>,
) -> Result<Json<nexus_core::performance::ToolStats>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.mcp_tool_stats.clone();
    let tool_stats = stats.get_tool_stats(&tool_name).ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("Tool '{}' not found", tool_name)
            })),
        )
    })?;

    Ok(Json(tool_stats))
}

/// Get slow tool calls
/// GET /mcp/performance/slow-tools
pub async fn get_slow_tool_calls(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<SlowToolCallsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.mcp_tool_stats.clone();
    let slow_tools = stats.get_slow_tools();
    let records: Vec<SlowToolRecord> = slow_tools
        .into_iter()
        .map(|t| SlowToolRecord {
            tool_name: t.tool_name,
            execution_time_ms: t.execution_time_ms,
            timestamp: t.timestamp,
            success: t.success,
            error: t.error,
        })
        .collect();

    Ok(Json(SlowToolCallsResponse {
        count: records.len(),
        tools: records,
    }))
}

/// Get cache statistics
/// GET /mcp/performance/cache
pub async fn get_cache_statistics(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<CacheStatistics>, (StatusCode, Json<serde_json::Value>)> {
    let cache = server.mcp_tool_cache.clone();
    let stats = cache.get_statistics();

    Ok(Json(CacheStatistics {
        hits: stats.hits,
        misses: stats.misses,
        evictions: stats.evictions,
        current_size: stats.current_size,
        max_size: stats.max_size,
    }))
}

/// Clear cache
/// POST /mcp/performance/cache/clear
pub async fn clear_cache(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    server.mcp_tool_cache.clear();

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Cache cleared"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock as PlRwLock;
    use tokio::sync::RwLock as TokioRwLock;

    fn build_test_server() -> Arc<NexusServer> {
        let ctx = nexus_core::testing::TestContext::new();
        let engine = nexus_core::Engine::with_isolated_catalog(ctx.path()).expect("engine init");
        let engine_arc = Arc::new(TokioRwLock::new(engine));
        let executor = Arc::new(nexus_core::executor::Executor::default());
        let dbm = Arc::new(PlRwLock::new(
            nexus_core::database::DatabaseManager::new(ctx.path().to_path_buf()).expect("dbm init"),
        ));
        let rbac = Arc::new(TokioRwLock::new(
            nexus_core::auth::RoleBasedAccessControl::new(),
        ));
        let auth_mgr = Arc::new(nexus_core::auth::AuthManager::new(
            nexus_core::auth::AuthConfig::default(),
        ));
        let jwt = Arc::new(nexus_core::auth::JwtManager::new(
            nexus_core::auth::JwtConfig::default(),
        ));
        let audit = Arc::new(
            nexus_core::auth::AuditLogger::new(nexus_core::auth::AuditConfig {
                enabled: false,
                log_dir: ctx.path().join("audit"),
                retention_days: 1,
                compress_logs: false,
            })
            .expect("audit init"),
        );
        let _leaked = Box::leak(Box::new(ctx));

        Arc::new(NexusServer::new(
            executor,
            engine_arc,
            dbm,
            rbac,
            auth_mgr,
            jwt,
            audit,
            crate::config::RootUserConfig::default(),
        ))
    }

    #[tokio::test]
    async fn test_get_mcp_tool_statistics_empty_server_returns_zero_counters() {
        let server = build_test_server();
        let response = get_mcp_tool_statistics(State(server)).await.expect("ok");
        assert_eq!(response.statistics.total_calls, 0);
        assert_eq!(response.statistics.slow_tool_count, 0);
        assert!(response.tools.is_empty());
    }

    #[tokio::test]
    async fn test_get_tool_statistics_unknown_tool_returns_404() {
        let server = build_test_server();
        let result = get_tool_statistics(
            State(server),
            axum::extract::Path("nonexistent".to_string()),
        )
        .await;
        assert!(result.is_err());
        let (status, _) = result.expect_err("error branch");
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_clear_cache_returns_success() {
        let server = build_test_server();
        let response = clear_cache(State(server)).await.expect("ok");
        assert_eq!(response.0["status"], "success");
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_mcp_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        // Both start empty and independent.
        let resp_a = get_mcp_tool_statistics(State(Arc::clone(&server_a)))
            .await
            .expect("ok");
        let resp_b = get_mcp_tool_statistics(State(Arc::clone(&server_b)))
            .await
            .expect("ok");
        assert_eq!(resp_a.statistics.total_calls, 0);
        assert_eq!(resp_b.statistics.total_calls, 0);

        // Arc identities must differ — they are separately constructed.
        assert!(!Arc::ptr_eq(
            &server_a.mcp_tool_stats,
            &server_b.mcp_tool_stats
        ));
        assert!(!Arc::ptr_eq(
            &server_a.mcp_tool_cache,
            &server_b.mcp_tool_cache
        ));
    }
}
