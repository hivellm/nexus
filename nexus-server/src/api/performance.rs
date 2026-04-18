//! Performance monitoring API endpoints
//!
//! Provides REST endpoints for:
//! - GET /performance/statistics - Query execution statistics
//! - GET /performance/slow-queries - Slow query log
//! - GET /performance/plan-cache - Plan cache statistics
//! - POST /performance/plan-cache/clear - Clear plan cache

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Json;
use serde::Serialize;
use std::sync::Arc;

use crate::NexusServer;

/// Query statistics response
#[derive(Debug, Serialize)]
pub struct QueryStatisticsResponse {
    /// Overall statistics
    pub statistics: QueryStatisticsSummary,
    /// Pattern statistics
    pub patterns: Vec<QueryPatternStatsResponse>,
}

/// Query statistics summary response
#[derive(Debug, Serialize)]
pub struct QueryStatisticsSummary {
    /// Total queries executed
    pub total_queries: u64,
    /// Successful queries
    pub successful_queries: u64,
    /// Failed queries
    pub failed_queries: u64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Average execution time in milliseconds
    pub average_execution_time_ms: u64,
    /// Minimum execution time in milliseconds
    pub min_execution_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_execution_time_ms: u64,
    /// Number of slow queries logged
    pub slow_query_count: usize,
}

/// Query pattern statistics response
#[derive(Debug, Serialize)]
pub struct QueryPatternStatsResponse {
    /// Query pattern (normalized)
    pub pattern: String,
    /// Execution count
    pub count: u64,
    /// Average execution time in milliseconds
    pub avg_time_ms: f64,
    /// Minimum execution time in milliseconds
    pub min_time_ms: u64,
    /// Maximum execution time in milliseconds
    pub max_time_ms: u64,
    /// Success count
    pub success_count: u64,
    /// Failure count
    pub failure_count: u64,
}

/// Slow queries response
#[derive(Debug, Serialize)]
pub struct SlowQueriesResponse {
    /// Slow query records
    pub queries: Vec<SlowQueryRecord>,
    /// Total count
    pub count: usize,
}

/// Slow query record response
#[derive(Debug, Serialize)]
pub struct SlowQueryRecord {
    /// Query text
    pub query: String,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Timestamp when query was executed (Unix timestamp)
    pub timestamp: u64,
    /// Whether query succeeded
    pub success: bool,
    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Number of rows returned
    pub rows_returned: usize,
}

/// Plan cache statistics response
#[derive(Debug, Serialize)]
pub struct PlanCacheStatisticsResponse {
    /// Number of cached plans
    pub cached_plans: usize,
    /// Maximum cache size
    pub max_size: usize,
    /// Current memory usage in bytes
    pub current_memory_bytes: usize,
    /// Maximum memory usage in bytes
    pub max_memory_bytes: usize,
    /// Cache hit rate (0.0 to 1.0)
    pub hit_rate: f64,
}

/// Get query statistics
/// GET /performance/statistics
pub async fn get_query_statistics(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<QueryStatisticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.query_stats.clone();
    let summary = stats.get_statistics();
    let pattern_stats = stats.get_pattern_stats();

    let patterns: Vec<QueryPatternStatsResponse> = pattern_stats
        .into_values()
        .map(|stats| QueryPatternStatsResponse {
            pattern: stats.pattern,
            count: stats.count,
            avg_time_ms: stats.avg_time_ms,
            min_time_ms: stats.min_time_ms,
            max_time_ms: stats.max_time_ms,
            success_count: stats.success_count,
            failure_count: stats.failure_count,
        })
        .collect();

    Ok(Json(QueryStatisticsResponse {
        statistics: QueryStatisticsSummary {
            total_queries: summary.total_queries,
            successful_queries: summary.successful_queries,
            failed_queries: summary.failed_queries,
            total_execution_time_ms: summary.total_execution_time_ms,
            average_execution_time_ms: summary.average_execution_time_ms,
            min_execution_time_ms: summary.min_execution_time_ms,
            max_execution_time_ms: summary.max_execution_time_ms,
            slow_query_count: summary.slow_query_count,
        },
        patterns,
    }))
}

/// Get slow queries
/// GET /performance/slow-queries
pub async fn get_slow_queries(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<SlowQueriesResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.query_stats.clone();
    let slow_queries = stats.get_slow_queries();
    let records: Vec<SlowQueryRecord> = slow_queries
        .into_iter()
        .map(|q| SlowQueryRecord {
            query: q.query,
            execution_time_ms: q.execution_time_ms,
            timestamp: q.timestamp,
            success: q.success,
            error: q.error,
            rows_returned: q.rows_returned,
        })
        .collect();

    Ok(Json(SlowQueriesResponse {
        count: records.len(),
        queries: records,
    }))
}

/// Get plan cache statistics
/// GET /performance/plan-cache
pub async fn get_plan_cache_statistics(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<PlanCacheStatisticsResponse>, (StatusCode, Json<serde_json::Value>)> {
    let cache = server.plan_cache.clone();
    let stats = cache.get_statistics();

    Ok(Json(PlanCacheStatisticsResponse {
        cached_plans: stats.cached_plans,
        max_size: stats.max_size,
        current_memory_bytes: stats.current_memory_bytes,
        max_memory_bytes: stats.max_memory_bytes,
        hit_rate: stats.hit_rate,
    }))
}

/// Clear plan cache
/// POST /performance/plan-cache/clear
pub async fn clear_plan_cache(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    server.plan_cache.invalidate_all();

    Ok(Json(serde_json::json!({
        "status": "success",
        "message": "Plan cache cleared"
    })))
}

/// Slow query analysis response
#[derive(Debug, Serialize)]
pub struct SlowQueryAnalysisResponse {
    /// Analysis results
    pub analyses: Vec<SlowQueryAnalysisItem>,
    /// Total patterns analyzed
    pub total_patterns: usize,
}

/// Slow query analysis item
#[derive(Debug, Serialize)]
pub struct SlowQueryAnalysisItem {
    /// Query pattern
    pub pattern: String,
    /// Number of occurrences
    pub occurrences: usize,
    /// Average execution time in milliseconds
    pub avg_execution_time_ms: f64,
    /// Total execution time in milliseconds
    pub total_execution_time_ms: u64,
    /// Recommendations
    pub recommendations: Vec<String>,
}

/// Analyze slow queries
/// GET /performance/slow-queries/analysis
pub async fn analyze_slow_queries(
    State(server): State<Arc<NexusServer>>,
) -> Result<Json<SlowQueryAnalysisResponse>, (StatusCode, Json<serde_json::Value>)> {
    let stats = server.query_stats.clone();
    let analyzer = nexus_core::performance::slow_query_analysis::SlowQueryAnalyzer::new();
    let analyses = analyzer.analyze(&stats);

    let items: Vec<SlowQueryAnalysisItem> = analyses
        .into_iter()
        .map(|a| SlowQueryAnalysisItem {
            pattern: a.pattern,
            occurrences: a.occurrences,
            avg_execution_time_ms: a.avg_execution_time_ms,
            total_execution_time_ms: a.total_execution_time_ms,
            recommendations: a.recommendations,
        })
        .collect();

    Ok(Json(SlowQueryAnalysisResponse {
        total_patterns: items.len(),
        analyses: items,
    }))
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
    async fn test_get_query_statistics_empty_server_returns_zero_counters() {
        let server = build_test_server();
        let response = get_query_statistics(State(server)).await.expect("ok");
        assert_eq!(response.statistics.total_queries, 0);
        assert_eq!(response.statistics.successful_queries, 0);
        assert_eq!(response.statistics.failed_queries, 0);
        assert!(response.patterns.is_empty());
    }

    #[tokio::test]
    async fn test_get_slow_queries_empty_server_returns_empty_list() {
        let server = build_test_server();
        let response = get_slow_queries(State(server)).await.expect("ok");
        assert_eq!(response.count, 0);
        assert!(response.queries.is_empty());
    }

    #[tokio::test]
    async fn test_plan_cache_statistics_starts_empty() {
        let server = build_test_server();
        let response = get_plan_cache_statistics(State(server)).await.expect("ok");
        assert_eq!(response.cached_plans, 0);
        // hit_rate on an empty cache is defined as 0.0 by QueryPlanCache.
        assert!(response.hit_rate >= 0.0);
    }

    #[tokio::test]
    async fn test_clear_plan_cache_returns_success() {
        let server = build_test_server();
        let response = clear_plan_cache(State(server)).await.expect("ok");
        assert_eq!(response.0["status"], "success");
    }

    #[tokio::test]
    async fn test_analyze_slow_queries_empty_server_returns_empty_list() {
        let server = build_test_server();
        let response = analyze_slow_queries(State(server)).await.expect("ok");
        assert_eq!(response.total_patterns, 0);
        assert!(response.analyses.is_empty());
    }

    #[tokio::test]
    async fn test_two_servers_do_not_share_performance_state() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        // Record a synthetic query on A's stats. B must not observe it.
        server_a.query_stats.record_query(
            "MATCH (n) RETURN n",
            std::time::Duration::from_millis(1),
            true,
            None,
            0,
        );

        let resp_a = get_query_statistics(State(Arc::clone(&server_a)))
            .await
            .expect("ok");
        let resp_b = get_query_statistics(State(Arc::clone(&server_b)))
            .await
            .expect("ok");

        assert_eq!(resp_a.statistics.total_queries, 1);
        assert_eq!(resp_b.statistics.total_queries, 0);

        assert!(!Arc::ptr_eq(&server_a.query_stats, &server_b.query_stats));
        assert!(!Arc::ptr_eq(&server_a.plan_cache, &server_b.plan_cache));
        assert!(!Arc::ptr_eq(
            &server_a.dbms_procedures,
            &server_b.dbms_procedures
        ));
    }
}
