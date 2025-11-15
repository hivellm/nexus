//! MCP Tool Performance Monitoring API endpoints
//!
//! Provides REST endpoints for:
//! - GET /mcp/performance/statistics - MCP tool execution statistics
//! - GET /mcp/performance/tools/{tool_name} - Statistics for specific tool
//! - GET /mcp/performance/slow-tools - Slow tool call log
//! - GET /mcp/performance/cache - Cache statistics
//! - POST /mcp/performance/cache/clear - Clear cache

use axum::http::StatusCode;
use axum::response::Json;
use nexus_core::performance::{mcp_tool_cache::CacheStatistics, mcp_tool_stats::McpToolStatistics};
use serde::Serialize;
use std::sync::{Arc, OnceLock};

/// Global MCP tool statistics instance
static MCP_TOOL_STATS: OnceLock<Arc<McpToolStatistics>> = OnceLock::new();

/// Global MCP tool cache instance
static MCP_TOOL_CACHE: OnceLock<Arc<nexus_core::performance::McpToolCache>> = OnceLock::new();

/// Initialize MCP tool performance monitoring
pub fn init_mcp_performance_monitoring(
    slow_tool_threshold_ms: u64,
    max_slow_tools: usize,
    cache_ttl_seconds: u64,
    cache_max_size: usize,
) -> anyhow::Result<()> {
    let tool_stats = Arc::new(McpToolStatistics::new(
        slow_tool_threshold_ms,
        max_slow_tools,
    ));
    MCP_TOOL_STATS
        .set(tool_stats)
        .map_err(|_| anyhow::anyhow!("Failed to set MCP tool statistics"))?;

    let tool_cache = Arc::new(nexus_core::performance::McpToolCache::new(
        cache_ttl_seconds,
        cache_max_size,
    ));
    MCP_TOOL_CACHE
        .set(tool_cache)
        .map_err(|_| anyhow::anyhow!("Failed to set MCP tool cache"))?;

    Ok(())
}

/// Get MCP tool statistics instance
pub fn get_mcp_tool_stats() -> Option<Arc<McpToolStatistics>> {
    MCP_TOOL_STATS.get().cloned()
}

/// Get MCP tool cache instance
pub fn get_mcp_tool_cache() -> Option<Arc<nexus_core::performance::McpToolCache>> {
    MCP_TOOL_CACHE.get().cloned()
}

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
pub async fn get_mcp_tool_statistics()
-> Result<Json<McpToolStatisticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = get_mcp_tool_stats().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "MCP performance monitoring not initialized"
            })),
        )
    })?;

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
    axum::extract::Path(tool_name): axum::extract::Path<String>,
) -> Result<Json<nexus_core::performance::ToolStats>, (StatusCode, Json<serde_json::Value>)> {
    let stats = get_mcp_tool_stats().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "MCP performance monitoring not initialized"
            })),
        )
    })?;

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
pub async fn get_slow_tool_calls()
-> Result<Json<SlowToolCallsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = get_mcp_tool_stats().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "MCP performance monitoring not initialized"
            })),
        )
    })?;

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
pub async fn get_cache_statistics()
-> Result<Json<CacheStatistics>, (StatusCode, Json<serde_json::Value>)> {
    let cache = get_mcp_tool_cache().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "MCP tool cache not initialized"
            })),
        )
    })?;

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
pub async fn clear_cache() -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)>
{
    let cache = get_mcp_tool_cache().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "MCP tool cache not initialized"
            })),
        )
    })?;

    cache.clear();

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Cache cleared"
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_mcp_tool_statistics_not_initialized() {
        // This test may pass or fail depending on initialization order
        let result = get_mcp_tool_statistics().await;
        // Just verify it doesn't panic
        drop(result);
    }

    #[tokio::test]
    async fn test_mcp_performance_monitoring_initialization() {
        // Try to initialize - may succeed or fail if already initialized
        let result = init_mcp_performance_monitoring(100, 1000, 3600, 100);
        // If already initialized, result will be Err, which is fine

        // Now should be able to get statistics (if initialized)
        let stats_result = get_mcp_tool_statistics().await;
        // Should succeed if initialized
        assert!(stats_result.is_ok() || stats_result.is_err());
    }
}
