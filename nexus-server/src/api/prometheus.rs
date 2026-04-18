//! Prometheus metrics endpoint
//!
//! Provides Prometheus-compatible metrics for monitoring and observability.
//!
//! # phase2e
//!
//! The process-wide `METRICS: OnceLock<PrometheusMetrics>` is gone.
//! The counter pack lives on `NexusServer::metrics` and every handler
//! reads it via the `State<Arc<NexusServer>>` extractor. Process-wide
//! counters that are *genuinely* shared across subsystems (RESP3
//! listener, RPC listener, audit-log failure total) keep their own
//! atomic counters in their own modules and are snapshotted at render
//! time.

use axum::extract::State;
use axum::response::IntoResponse;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::NexusServer;

/// Prometheus metrics collector
pub struct PrometheusMetrics {
    /// Total queries executed
    pub total_queries: Arc<AtomicU64>,
    /// Successful queries
    pub successful_queries: Arc<AtomicU64>,
    /// Failed queries
    pub failed_queries: Arc<AtomicU64>,
    /// Total query execution time in milliseconds
    pub total_execution_time_ms: Arc<AtomicU64>,
    /// Cache hits
    pub cache_hits: Arc<AtomicU64>,
    /// Cache misses
    pub cache_misses: Arc<AtomicU64>,
    /// Active connections
    pub active_connections: Arc<AtomicU64>,
}

impl Default for PrometheusMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PrometheusMetrics {
    /// Create a new Prometheus metrics collector
    pub fn new() -> Self {
        Self {
            total_queries: Arc::new(AtomicU64::new(0)),
            successful_queries: Arc::new(AtomicU64::new(0)),
            failed_queries: Arc::new(AtomicU64::new(0)),
            total_execution_time_ms: Arc::new(AtomicU64::new(0)),
            cache_hits: Arc::new(AtomicU64::new(0)),
            cache_misses: Arc::new(AtomicU64::new(0)),
            active_connections: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Record a query execution
    pub fn record_query(&self, success: bool, execution_time_ms: u64) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        if success {
            self.successful_queries.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failed_queries.fetch_add(1, Ordering::Relaxed);
        }
        self.total_execution_time_ms
            .fetch_add(execution_time_ms, Ordering::Relaxed);
    }

    /// Record a cache hit
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment active connections
    pub fn increment_connections(&self) {
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement active connections
    pub fn decrement_connections(&self) {
        self.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Format metrics in Prometheus format
    pub fn format_prometheus(&self) -> String {
        let total = self.total_queries.load(Ordering::Relaxed);
        let successful = self.successful_queries.load(Ordering::Relaxed);
        let failed = self.failed_queries.load(Ordering::Relaxed);
        let total_time = self.total_execution_time_ms.load(Ordering::Relaxed);
        let cache_hits = self.cache_hits.load(Ordering::Relaxed);
        let cache_misses = self.cache_misses.load(Ordering::Relaxed);
        let active_conns = self.active_connections.load(Ordering::Relaxed);
        // Pulled directly from the static counter inside
        // `nexus_core::auth::middleware` — bumped every time an audit-log
        // write fails on a failed-auth path. Fail-open policy (see
        // docs/security/SECURITY_AUDIT.md): the request still returns 401/429/500,
        // but ops can alarm on this counter.
        let audit_log_failures = nexus_core::auth::audit_log_failures_total();
        // RESP3 listener metrics — these are process-wide counters
        // maintained inside `nexus_server::protocol::resp3::server`, so
        // they pick up bumps from every connection regardless of which
        // `PrometheusMetrics` instance formatted the response.
        let resp3 = crate::protocol::resp3::server::metrics_snapshot();
        // Native binary RPC metrics — same pattern (process-wide
        // AtomicU64 counters).
        let rpc = crate::protocol::rpc::metrics::snapshot();

        let avg_time = if total > 0 {
            total_time as f64 / total as f64
        } else {
            0.0
        };

        let cache_total = cache_hits + cache_misses;
        let cache_hit_rate = if cache_total > 0 {
            cache_hits as f64 / cache_total as f64
        } else {
            0.0
        };

        format!(
            r#"# HELP nexus_queries_total Total number of queries executed
# TYPE nexus_queries_total counter
nexus_queries_total {total}

# HELP nexus_queries_successful Total number of successful queries
# TYPE nexus_queries_successful counter
nexus_queries_successful {successful}

# HELP nexus_queries_failed Total number of failed queries
# TYPE nexus_queries_failed counter
nexus_queries_failed {failed}

# HELP nexus_query_execution_time_ms_total Total query execution time in milliseconds
# TYPE nexus_query_execution_time_ms_total counter
nexus_query_execution_time_ms_total {total_time}

# HELP nexus_query_execution_time_ms_avg Average query execution time in milliseconds
# TYPE nexus_query_execution_time_ms_avg gauge
nexus_query_execution_time_ms_avg {avg_time}

# HELP nexus_cache_hits_total Total number of cache hits
# TYPE nexus_cache_hits_total counter
nexus_cache_hits_total {cache_hits}

# HELP nexus_cache_misses_total Total number of cache misses
# TYPE nexus_cache_misses_total counter
nexus_cache_misses_total {cache_misses}

# HELP nexus_cache_hit_rate Cache hit rate (0.0 to 1.0)
# TYPE nexus_cache_hit_rate gauge
nexus_cache_hit_rate {cache_hit_rate}

# HELP nexus_active_connections Current number of active connections
# TYPE nexus_active_connections gauge
nexus_active_connections {active_conns}

# HELP nexus_audit_log_failures_total Audit-log write failures observed by the auth middleware (fail-open: request still returned original auth error, but the event was not persisted). Alarm when this counter moves. See docs/security/SECURITY_AUDIT.md.
# TYPE nexus_audit_log_failures_total counter
nexus_audit_log_failures_total {audit_log_failures}

# HELP nexus_resp3_connections Currently-live RESP3 TCP connections.
# TYPE nexus_resp3_connections gauge
nexus_resp3_connections {resp3_connections}

# HELP nexus_resp3_commands_total Total RESP3 commands dispatched since server start.
# TYPE nexus_resp3_commands_total counter
nexus_resp3_commands_total {resp3_commands}

# HELP nexus_resp3_commands_error_total RESP3 commands that returned an error response.
# TYPE nexus_resp3_commands_error_total counter
nexus_resp3_commands_error_total {resp3_commands_error}

# HELP nexus_resp3_command_duration_microseconds_total Sum of RESP3 handler wall-clock durations in microseconds. Divide by nexus_resp3_commands_total for an average.
# TYPE nexus_resp3_command_duration_microseconds_total counter
nexus_resp3_command_duration_microseconds_total {resp3_duration}

# HELP nexus_resp3_bytes_read_total Bytes read from RESP3 sockets since start.
# TYPE nexus_resp3_bytes_read_total counter
nexus_resp3_bytes_read_total {resp3_bytes_read}

# HELP nexus_resp3_bytes_written_total Bytes written to RESP3 sockets since start.
# TYPE nexus_resp3_bytes_written_total counter
nexus_resp3_bytes_written_total {resp3_bytes_written}

# HELP nexus_rpc_connections Currently-live native binary RPC connections.
# TYPE nexus_rpc_connections gauge
nexus_rpc_connections {rpc_connections}

# HELP nexus_rpc_commands_total Total RPC commands dispatched since server start.
# TYPE nexus_rpc_commands_total counter
nexus_rpc_commands_total {rpc_commands}

# HELP nexus_rpc_commands_error_total RPC commands that returned an error response.
# TYPE nexus_rpc_commands_error_total counter
nexus_rpc_commands_error_total {rpc_commands_error}

# HELP nexus_rpc_command_duration_microseconds_total Sum of RPC handler wall-clock durations in microseconds. Divide by nexus_rpc_commands_total for an average.
# TYPE nexus_rpc_command_duration_microseconds_total counter
nexus_rpc_command_duration_microseconds_total {rpc_duration}

# HELP nexus_rpc_frame_bytes_in_total Sum of incoming RPC frame body sizes in bytes (including 4-byte length prefix).
# TYPE nexus_rpc_frame_bytes_in_total counter
nexus_rpc_frame_bytes_in_total {rpc_bytes_in}

# HELP nexus_rpc_frame_bytes_out_total Sum of outgoing RPC frame body sizes in bytes.
# TYPE nexus_rpc_frame_bytes_out_total counter
nexus_rpc_frame_bytes_out_total {rpc_bytes_out}

# HELP nexus_rpc_slow_commands_total RPC commands that exceeded the configured slow-threshold.
# TYPE nexus_rpc_slow_commands_total counter
nexus_rpc_slow_commands_total {rpc_slow}
"#,
            total = total,
            successful = successful,
            failed = failed,
            total_time = total_time,
            avg_time = avg_time,
            cache_hits = cache_hits,
            cache_misses = cache_misses,
            cache_hit_rate = cache_hit_rate,
            active_conns = active_conns,
            audit_log_failures = audit_log_failures,
            resp3_connections = resp3.active_connections,
            resp3_commands = resp3.commands_total,
            resp3_commands_error = resp3.commands_error_total,
            resp3_duration = resp3.command_duration_microseconds_total,
            resp3_bytes_read = resp3.bytes_read_total,
            resp3_bytes_written = resp3.bytes_written_total,
            rpc_connections = rpc.active_connections,
            rpc_commands = rpc.commands_total,
            rpc_commands_error = rpc.commands_error_total,
            rpc_duration = rpc.command_duration_us_total,
            rpc_bytes_in = rpc.bytes_in_total,
            rpc_bytes_out = rpc.bytes_out_total,
            rpc_slow = rpc.slow_commands_total,
        )
    }
}

/// Prometheus metrics endpoint handler. Reads the counter pack the
/// server owns via `NexusServer::metrics`.
pub async fn prometheus_metrics(State(server): State<Arc<NexusServer>>) -> impl IntoResponse {
    let formatted = server.metrics.format_prometheus();

    (
        axum::http::StatusCode::OK,
        [("Content-Type", "text/plain; version=0.0.4; charset=utf-8")],
        formatted,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_metrics() {
        let metrics = PrometheusMetrics::new();

        // Record some metrics
        metrics.record_query(true, 100);
        metrics.record_query(false, 200);
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let formatted = metrics.format_prometheus();

        // Check that metrics are formatted correctly
        assert!(formatted.contains("nexus_queries_total 2"));
        assert!(formatted.contains("nexus_queries_successful 1"));
        assert!(formatted.contains("nexus_queries_failed 1"));
        assert!(formatted.contains("nexus_cache_hits_total 1"));
        assert!(formatted.contains("nexus_cache_misses_total 1"));
    }

    // Confirms the new audit-log failure counter is exported with the
    // stable `nexus_audit_log_failures_total` name + HELP/TYPE metadata so
    // operators can reliably scrape and alarm on it (see
    // docs/security/SECURITY_AUDIT.md).
    #[test]
    fn audit_log_failures_metric_is_exported() {
        let metrics = PrometheusMetrics::new();
        let formatted = metrics.format_prometheus();

        assert!(
            formatted.contains("# TYPE nexus_audit_log_failures_total counter"),
            "metric must be advertised as a Prometheus counter so PromQL rate() works",
        );
        assert!(
            formatted.contains("nexus_audit_log_failures_total "),
            "counter value line must be present in the exported metrics",
        );
        assert!(
            formatted.contains("Alarm when this counter moves"),
            "HELP text must steer ops toward the right alert action",
        );
    }

    // ── phase2e: isolation guard ─────────────────────────────────────
    //
    // Two NexusServers in the same process must keep independent
    // Prometheus counter packs. The previous OnceLock-based METRICS
    // singleton made this silently impossible; every handler saw
    // whichever server happened to call `init()` first.

    use parking_lot::RwLock as PlRwLock;
    use std::sync::atomic::Ordering;
    use tokio::sync::RwLock as TokioRwLock;

    fn build_test_server() -> Arc<crate::NexusServer> {
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

        Arc::new(crate::NexusServer::new(
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
    async fn test_two_servers_keep_independent_prometheus_counters() {
        let server_a = build_test_server();
        let server_b = build_test_server();

        // Bump every counter on server A.
        server_a.metrics.record_query(true, 42);
        server_a.metrics.record_query(false, 100);
        server_a.metrics.record_cache_hit();
        server_a.metrics.record_cache_miss();
        server_a.metrics.increment_connections();

        // Server A observes the updates.
        assert_eq!(server_a.metrics.total_queries.load(Ordering::Relaxed), 2);
        assert_eq!(
            server_a.metrics.successful_queries.load(Ordering::Relaxed),
            1
        );
        assert_eq!(server_a.metrics.failed_queries.load(Ordering::Relaxed), 1);
        assert_eq!(server_a.metrics.cache_hits.load(Ordering::Relaxed), 1);
        assert_eq!(server_a.metrics.cache_misses.load(Ordering::Relaxed), 1);
        assert_eq!(
            server_a.metrics.active_connections.load(Ordering::Relaxed),
            1
        );

        // Server B must still be pristine — no shared state.
        assert_eq!(server_b.metrics.total_queries.load(Ordering::Relaxed), 0);
        assert_eq!(
            server_b.metrics.successful_queries.load(Ordering::Relaxed),
            0
        );
        assert_eq!(server_b.metrics.failed_queries.load(Ordering::Relaxed), 0);
        assert_eq!(server_b.metrics.cache_hits.load(Ordering::Relaxed), 0);
        assert_eq!(server_b.metrics.cache_misses.load(Ordering::Relaxed), 0);
        assert_eq!(
            server_b.metrics.active_connections.load(Ordering::Relaxed),
            0
        );

        // Arc identities differ too.
        assert!(!Arc::ptr_eq(&server_a.metrics, &server_b.metrics));
    }
}
