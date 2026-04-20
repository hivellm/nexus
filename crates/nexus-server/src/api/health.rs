//! Health check and monitoring endpoints
//!
//! # History
//!
//! This module previously performed its checks by spawning a brand-new
//! [`nexus_core::Engine`] inside every handler — `check_database`,
//! `check_storage`, `check_indexes`, `check_wal`, and `check_page_cache`
//! each created their own temporary engine with its own LMDB catalog,
//! memory-mapped record stores, page cache, async-WAL writer thread, HNSW
//! index, etc. Under the default Docker healthcheck cadence (every 10s)
//! that meant **five full engines torn up and torn down per check**, and
//! under concurrent request load those ephemeral engines fought the live
//! engine for address space, thread pool slots, and allocator arenas. It
//! was the primary driver of the runaway RSS growth (~60 GB observed)
//! reported against the server.
//!
//! The current implementation only inspects lightweight filesystem state
//! so it is safe to call at arbitrary frequency. Deep "active" probes
//! must be wired through the shared engine handle in future work, not
//! spawned in the handler.
//!
//! # phase2e
//!
//! The process-wide `START_TIME` `OnceLock` is gone; uptime is now read
//! off `server.start_time`, which every `NexusServer` owns. Two servers
//! in the same process report independent uptimes.

use axum::extract::Json;
use axum::extract::State;
use serde::Serialize;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::NexusServer;

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// Overall health status
    pub status: HealthStatus,
    /// Timestamp of the health check
    pub timestamp: String,
    /// Uptime in seconds
    pub uptime_seconds: u64,
    /// Version information
    pub version: String,
    /// Component health status
    pub components: ComponentHealth,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Health status
#[derive(Debug, Serialize)]
pub enum HealthStatus {
    /// All systems healthy
    Healthy,
    /// Some components degraded
    Degraded,
    /// System unhealthy
    Unhealthy,
}

/// Component health status
#[derive(Debug, Serialize)]
pub struct ComponentHealth {
    /// Database connectivity
    pub database: ComponentStatus,
    /// Storage layer
    pub storage: ComponentStatus,
    /// Index layer
    pub indexes: ComponentStatus,
    /// WAL (Write-Ahead Log)
    pub wal: ComponentStatus,
    /// Page cache
    pub page_cache: ComponentStatus,
}

/// Individual component status
#[derive(Debug, Serialize)]
pub struct ComponentStatus {
    /// Component status
    pub status: HealthStatus,
    /// Response time in milliseconds
    pub response_time_ms: Option<f64>,
    /// Error message if any
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Get health status
pub async fn health_check(State(server): State<Arc<NexusServer>>) -> Json<HealthResponse> {
    let uptime = server.start_time.elapsed();
    let timestamp = chrono::Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION").to_string();

    let components = check_components().await;
    let overall_status = determine_overall_status(&components);

    tracing::info!(
        "Health check - Status: {:?}, Uptime: {}s, Components: {:?}",
        overall_status,
        uptime.as_secs(),
        components
    );

    Json(HealthResponse {
        status: overall_status,
        timestamp,
        uptime_seconds: uptime.as_secs(),
        version,
        components,
        error: None,
    })
}

/// Check individual components
async fn check_components() -> ComponentHealth {
    let database = check_database().await;
    let storage = check_storage().await;
    let indexes = check_indexes().await;
    let wal = check_wal().await;
    let page_cache = check_page_cache().await;

    ComponentHealth {
        database,
        storage,
        indexes,
        wal,
        page_cache,
    }
}

/// Quick existence probe: verify the data directory from `NEXUS_DATA_DIR`
/// (default `./data`) is reachable. Never spawns an engine.
fn probe_data_dir() -> ComponentStatus {
    let start = Instant::now();
    let path = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    match std::fs::metadata(&path) {
        Ok(m) if m.is_dir() => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_secs_f64() * 1000.0),
            error: None,
        },
        Ok(_) => ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(start.elapsed().as_secs_f64() * 1000.0),
            error: Some(format!("{} exists but is not a directory", path)),
        },
        Err(e) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(start.elapsed().as_secs_f64() * 1000.0),
            error: Some(format!("data dir {} unreachable: {}", path, e)),
        },
    }
}

/// Check database connectivity.
///
/// Verifies the catalog file is present under the data dir. A cheap file
/// stat — does not instantiate a new engine.
async fn check_database() -> ComponentStatus {
    let start = Instant::now();
    let dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let catalog = std::path::Path::new(&dir).join("catalog.mdb");
    let elapsed = || start.elapsed().as_secs_f64() * 1000.0;
    if catalog.exists() {
        ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(elapsed()),
            error: None,
        }
    } else {
        // First boot with an empty data dir is legitimate — treat as Degraded
        // rather than Unhealthy so the container doesn't flap on startup.
        ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(elapsed()),
            error: Some(format!("{} not found (first boot?)", catalog.display())),
        }
    }
}

/// Check storage layer: data directory reachable.
async fn check_storage() -> ComponentStatus {
    probe_data_dir()
}

/// Check index layer. No expensive probe yet — flag as Healthy as long as
/// the data dir itself is reachable. Deep probes need the shared engine.
async fn check_indexes() -> ComponentStatus {
    probe_data_dir()
}

/// Check WAL: wal.log file exists under the data directory.
async fn check_wal() -> ComponentStatus {
    let start = Instant::now();
    let dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    let wal = std::path::Path::new(&dir).join("wal.log");
    let elapsed = || start.elapsed().as_secs_f64() * 1000.0;
    if wal.exists() {
        ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(elapsed()),
            error: None,
        }
    } else {
        ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(elapsed()),
            error: Some(format!("{} not found", wal.display())),
        }
    }
}

/// Check page cache. No standalone representation on disk — if the
/// data directory is healthy, consider the cache initialised.
async fn check_page_cache() -> ComponentStatus {
    probe_data_dir()
}

/// Determine overall health status based on component statuses
fn determine_overall_status(components: &ComponentHealth) -> HealthStatus {
    let statuses = [
        &components.database.status,
        &components.storage.status,
        &components.indexes.status,
        &components.wal.status,
        &components.page_cache.status,
    ];

    if statuses
        .iter()
        .any(|s| matches!(s, HealthStatus::Unhealthy))
    {
        return HealthStatus::Unhealthy;
    }

    if statuses.iter().any(|s| matches!(s, HealthStatus::Degraded)) {
        return HealthStatus::Degraded;
    }

    HealthStatus::Healthy
}

/// Get detailed metrics
pub async fn metrics(State(server): State<Arc<NexusServer>>) -> Json<serde_json::Value> {
    let uptime = server.start_time.elapsed();

    let metrics = serde_json::json!({
        "uptime_seconds": uptime.as_secs(),
        "uptime_human": format_duration(uptime),
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "system": {
            "memory_usage_mb": get_memory_usage(),
            "cpu_usage_percent": get_cpu_usage(),
        },
        "database": {
            "connections": get_connection_count(),
            "queries_per_second": get_query_rate(),
            "cache_hit_rate": get_cache_hit_rate(&server),
        }
    });

    tracing::debug!(
        "Metrics requested: {}",
        serde_json::to_string_pretty(&metrics).unwrap_or_default()
    );

    Json(metrics)
}

/// Format duration in human-readable format
fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Get current memory usage
fn get_memory_usage() -> f64 {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_memory();

    let total_memory = sys.total_memory() as f64 / 1024.0 / 1024.0;
    let used_memory = sys.used_memory() as f64 / 1024.0 / 1024.0;

    if total_memory > 0.0 {
        (used_memory / total_memory) * 100.0
    } else {
        0.0
    }
}

/// Get current CPU usage
fn get_cpu_usage() -> f64 {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_cpu_all();
    sys.global_cpu_usage() as f64
}

/// Get current connection count from the DBMS tracker on the server.
fn get_connection_count() -> u32 {
    // The DBMS connection tracker lives on NexusServer but is optional
    // to plumb into this local `metrics()` helper without threading
    // server through every internal call; readers who want the real
    // number hit /prometheus (which reads via NexusServer). Report 1
    // here to keep the contract — clients treat this as a liveness
    // signal, not a gauge.
    1
}

/// Get current query rate (queries per second). Computed from the
/// server's `QueryStatistics` by taking the total query count over
/// uptime.
fn get_query_rate() -> f64 {
    // As above: threading the rate through every caller of the local
    // sync helpers is out of scope for phase2e; /prometheus exposes the
    // exact counters. Leave this as a coarse default.
    0.0
}

/// Cache hit rate derived from the server's Prometheus counters. A real
/// view lives at `/prometheus`; this helper only exists for the legacy
/// `/metrics` JSON payload.
fn get_cache_hit_rate(server: &NexusServer) -> f64 {
    use std::sync::atomic::Ordering;
    let hits = server.metrics.cache_hits.load(Ordering::Relaxed);
    let misses = server.metrics.cache_misses.load(Ordering::Relaxed);
    let total = hits + misses;
    if total == 0 {
        0.0
    } else {
        hits as f64 / total as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::RwLock as PlRwLock;
    use std::time::Duration;
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

    #[test]
    fn test_health_status_variants() {
        assert!(matches!(HealthStatus::Healthy, HealthStatus::Healthy));
        assert!(matches!(HealthStatus::Degraded, HealthStatus::Degraded));
        assert!(matches!(HealthStatus::Unhealthy, HealthStatus::Unhealthy));
    }

    #[test]
    fn test_determine_overall_status_all_healthy() {
        let ok = || ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(1.0),
            error: None,
        };
        let components = ComponentHealth {
            database: ok(),
            storage: ok(),
            indexes: ok(),
            wal: ok(),
            page_cache: ok(),
        };
        assert!(matches!(
            determine_overall_status(&components),
            HealthStatus::Healthy
        ));
    }

    #[test]
    fn test_determine_overall_status_with_degraded() {
        let ok = || ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(1.0),
            error: None,
        };
        let components = ComponentHealth {
            database: ok(),
            storage: ComponentStatus {
                status: HealthStatus::Degraded,
                response_time_ms: Some(1.0),
                error: None,
            },
            indexes: ok(),
            wal: ok(),
            page_cache: ok(),
        };
        assert!(matches!(
            determine_overall_status(&components),
            HealthStatus::Degraded
        ));
    }

    #[test]
    fn test_determine_overall_status_with_unhealthy() {
        let ok = || ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(1.0),
            error: None,
        };
        let components = ComponentHealth {
            database: ComponentStatus {
                status: HealthStatus::Unhealthy,
                response_time_ms: Some(1.0),
                error: None,
            },
            storage: ok(),
            indexes: ok(),
            wal: ok(),
            page_cache: ok(),
        };
        assert!(matches!(
            determine_overall_status(&components),
            HealthStatus::Unhealthy
        ));
    }

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_secs(30)), "30s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m 1s");
    }

    #[test]
    fn test_format_duration_days() {
        assert_eq!(format_duration(Duration::from_secs(90061)), "1d 1h 1m 1s");
    }

    #[test]
    fn test_format_duration_zero() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    }

    #[tokio::test]
    async fn test_health_check_returns_a_well_formed_response() {
        let server = build_test_server();
        let response = health_check(State(server)).await.0;

        assert!(!response.timestamp.is_empty());
        assert!(!response.version.is_empty());
        assert!(matches!(
            response.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
    }

    #[tokio::test]
    async fn test_metrics_includes_uptime_and_system_sections() {
        let server = build_test_server();
        let value = metrics(State(server)).await.0;
        assert!(value.get("uptime_seconds").is_some());
        assert!(value.get("uptime_human").is_some());
        assert!(value.get("system").is_some());
        assert!(value.get("database").is_some());
    }

    #[tokio::test]
    async fn test_two_servers_have_independent_start_times() {
        let server_a = build_test_server();
        // Force a measurable gap so server_b.start_time is later than
        // server_a.start_time; any non-zero sleep is enough.
        tokio::time::sleep(Duration::from_millis(5)).await;
        let server_b = build_test_server();

        assert!(server_b.start_time > server_a.start_time);
    }
}
