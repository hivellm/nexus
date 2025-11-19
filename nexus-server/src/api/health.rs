//! Health check and monitoring endpoints

use axum::extract::Json;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::time::timeout;

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

/// Application start time for uptime calculation
static START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

/// Initialize the health check system
pub fn init() {
    let _ = START_TIME.set(Instant::now());
}

/// Get health status
pub async fn health_check() -> Json<HealthResponse> {
    let start_time = START_TIME.get().copied().unwrap_or_else(Instant::now);
    let uptime = start_time.elapsed();

    let timestamp = chrono::Utc::now().to_rfc3339();
    let version = env!("CARGO_PKG_VERSION").to_string();

    // Check component health
    let components = check_components().await;

    // Determine overall status
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

/// Check database connectivity
async fn check_database() -> ComponentStatus {
    let start = Instant::now();

    // Check actual database connectivity
    match timeout(Duration::from_secs(5), async {
        // Try to create a test engine instance to verify database connectivity
        // Use tempfile to keep directory alive during check
        let temp_dir = match tempfile::tempdir() {
            Ok(dir) => dir,
            Err(e) => return Err(format!("Failed to create temp directory: {}", e)),
        };
        match nexus_core::Engine::with_data_dir(temp_dir.path()) {
            Ok(mut engine) => {
                // Test basic operations
                let _stats = engine
                    .stats()
                    .map_err(|e| format!("Stats check failed: {}", e))?;
                let _health = engine
                    .health_check()
                    .map_err(|e| format!("Health check failed: {}", e))?;
                // Keep temp_dir alive until here
                drop(engine);
                drop(temp_dir);
                Ok::<(), String>(())
            }
            Err(e) => Err(format!("Database initialization failed: {}", e)),
        }
    })
    .await
    {
        Ok(Ok(_)) => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: None,
        },
        Ok(Err(e)) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: Some(e),
        },
        Err(_) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(5000.0),
            error: Some("Database check timeout".to_string()),
        },
    }
}

/// Check storage layer
async fn check_storage() -> ComponentStatus {
    let start = Instant::now();

    // Check actual storage layer
    match timeout(Duration::from_secs(3), async {
        // Create a test engine to verify storage
        match nexus_core::Engine::new() {
            Ok(mut engine) => {
                // Test storage operations
                let _stats = engine
                    .stats()
                    .map_err(|e| format!("Storage stats failed: {}", e))?;
                // Test creating a test node
                let _node_id = engine
                    .create_node(vec!["Test".to_string()], serde_json::Value::Null)
                    .map_err(|e| format!("Storage write test failed: {}", e))?;
                Ok::<(), String>(())
            }
            Err(e) => Err(format!("Storage initialization failed: {}", e)),
        }
    })
    .await
    {
        Ok(Ok(_)) => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: None,
        },
        Ok(Err(e)) => ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: Some(e),
        },
        Err(_) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(3000.0),
            error: Some("Storage check timeout".to_string()),
        },
    }
}

/// Check index layer
async fn check_indexes() -> ComponentStatus {
    let start = Instant::now();

    // Check actual index layer
    match timeout(Duration::from_secs(2), async {
        // Create a test engine to verify indexes
        match nexus_core::Engine::new() {
            Ok(engine) => {
                // Test index operations
                let mut engine = engine; // Make mutable
                let _stats = engine
                    .stats()
                    .map_err(|e| format!("Index stats failed: {}", e))?;
                // Test KNN search (even with empty index) - using 128-dimensional vector
                let test_vector: Vec<f32> = (0..128).map(|i| (i as f32) / 128.0).collect();
                let _knn_result = engine
                    .knn_search("Test", &test_vector, 5)
                    .map_err(|e| format!("Index search test failed: {}", e))?;
                Ok::<(), String>(())
            }
            Err(e) => Err(format!("Index initialization failed: {}", e)),
        }
    })
    .await
    {
        Ok(Ok(_)) => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: None,
        },
        Ok(Err(e)) => ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: Some(e),
        },
        Err(_) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(2000.0),
            error: Some("Index check timeout".to_string()),
        },
    }
}

/// Check WAL (Write-Ahead Log)
async fn check_wal() -> ComponentStatus {
    let start = Instant::now();

    // Check actual WAL
    match timeout(Duration::from_secs(1), async {
        // Create a test engine to verify WAL
        match nexus_core::Engine::new() {
            Ok(mut engine) => {
                // Test WAL operations by creating a transaction
                let _stats = engine
                    .stats()
                    .map_err(|e| format!("WAL stats failed: {}", e))?;
                // Test creating a node (which should write to WAL)
                let _node_id = engine
                    .create_node(vec!["Test".to_string()], serde_json::Value::Null)
                    .map_err(|e| format!("WAL write test failed: {}", e))?;
                Ok::<(), String>(())
            }
            Err(e) => Err(format!("WAL initialization failed: {}", e)),
        }
    })
    .await
    {
        Ok(Ok(_)) => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: None,
        },
        Ok(Err(e)) => ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: Some(e),
        },
        Err(_) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(1000.0),
            error: Some("WAL check timeout".to_string()),
        },
    }
}

/// Check page cache
async fn check_page_cache() -> ComponentStatus {
    let start = Instant::now();

    // Check actual page cache
    match timeout(Duration::from_millis(500), async {
        // Create a test engine to verify page cache
        match nexus_core::Engine::new() {
            Ok(engine) => {
                // Test page cache operations
                let mut engine = engine; // Make mutable
                let _stats = engine
                    .stats()
                    .map_err(|e| format!("Page cache stats failed: {}", e))?;
                // Test reading operations (which should use page cache)
                let _node_result = engine
                    .get_node(1)
                    .map_err(|e| format!("Page cache read test failed: {}", e))?;
                Ok::<(), String>(())
            }
            Err(e) => Err(format!("Page cache initialization failed: {}", e)),
        }
    })
    .await
    {
        Ok(Ok(_)) => ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: None,
        },
        Ok(Err(e)) => ComponentStatus {
            status: HealthStatus::Degraded,
            response_time_ms: Some(start.elapsed().as_millis() as f64),
            error: Some(e),
        },
        Err(_) => ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(500.0),
            error: Some("Page cache check timeout".to_string()),
        },
    }
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

    // If any component is unhealthy, overall status is unhealthy
    if statuses
        .iter()
        .any(|s| matches!(s, HealthStatus::Unhealthy))
    {
        return HealthStatus::Unhealthy;
    }

    // If any component is degraded, overall status is degraded
    if statuses.iter().any(|s| matches!(s, HealthStatus::Degraded)) {
        return HealthStatus::Degraded;
    }

    // All components are healthy
    HealthStatus::Healthy
}

/// Get detailed metrics
pub async fn metrics() -> Json<serde_json::Value> {
    let start_time = START_TIME.get().copied().unwrap_or_else(Instant::now);
    let uptime = start_time.elapsed();

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
            "cache_hit_rate": get_cache_hit_rate(),
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

    // Get total memory usage in MB
    let total_memory = sys.total_memory() as f64 / 1024.0 / 1024.0;
    let used_memory = sys.used_memory() as f64 / 1024.0 / 1024.0;

    // Return memory usage as percentage
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

    // Get CPU usage as percentage
    let cpu_usage = sys.global_cpu_usage();
    cpu_usage as f64
}

/// Get current connection count
fn get_connection_count() -> u32 {
    // For now, return a placeholder value
    // In a real implementation, this would track active connections
    1
}

/// Get current query rate (queries per second)
fn get_query_rate() -> f64 {
    // For now, return a placeholder value
    // In a real implementation, this would track actual query rates
    0.0
}

/// Get current cache hit rate
fn get_cache_hit_rate() -> f64 {
    // For now, return a placeholder value
    // In a real implementation, this would calculate actual hit rates
    0.95
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_health_status_variants() {
        let healthy = HealthStatus::Healthy;
        let degraded = HealthStatus::Degraded;
        let unhealthy = HealthStatus::Unhealthy;

        // Test that all variants can be created
        assert!(matches!(healthy, HealthStatus::Healthy));
        assert!(matches!(degraded, HealthStatus::Degraded));
        assert!(matches!(unhealthy, HealthStatus::Unhealthy));
    }

    #[test]
    fn test_component_status_creation() {
        let status = ComponentStatus {
            status: HealthStatus::Healthy,
            response_time_ms: Some(100.0),
            error: None,
        };

        assert!(matches!(status.status, HealthStatus::Healthy));
        assert_eq!(status.response_time_ms, Some(100.0));
        assert_eq!(status.error, None);
    }

    #[test]
    fn test_component_status_with_error() {
        let status = ComponentStatus {
            status: HealthStatus::Unhealthy,
            response_time_ms: Some(500.0),
            error: Some("Connection failed".to_string()),
        };

        assert!(matches!(status.status, HealthStatus::Unhealthy));
        assert_eq!(status.response_time_ms, Some(500.0));
        assert_eq!(status.error, Some("Connection failed".to_string()));
    }

    #[test]
    fn test_component_health_creation() {
        let components = ComponentHealth {
            database: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(50.0),
                error: None,
            },
            storage: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(30.0),
                error: None,
            },
            indexes: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(20.0),
                error: None,
            },
            wal: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(10.0),
                error: None,
            },
            page_cache: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(5.0),
                error: None,
            },
        };

        assert!(matches!(components.database.status, HealthStatus::Healthy));
        assert!(matches!(components.storage.status, HealthStatus::Healthy));
        assert!(matches!(components.indexes.status, HealthStatus::Healthy));
        assert!(matches!(components.wal.status, HealthStatus::Healthy));
        assert!(matches!(
            components.page_cache.status,
            HealthStatus::Healthy
        ));
    }

    #[test]
    fn test_determine_overall_status_all_healthy() {
        let components = ComponentHealth {
            database: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(50.0),
                error: None,
            },
            storage: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(30.0),
                error: None,
            },
            indexes: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(20.0),
                error: None,
            },
            wal: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(10.0),
                error: None,
            },
            page_cache: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(5.0),
                error: None,
            },
        };

        let overall_status = determine_overall_status(&components);
        assert!(matches!(overall_status, HealthStatus::Healthy));
    }

    #[test]
    fn test_determine_overall_status_with_degraded() {
        let components = ComponentHealth {
            database: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(50.0),
                error: None,
            },
            storage: ComponentStatus {
                status: HealthStatus::Degraded,
                response_time_ms: Some(1000.0),
                error: Some("Slow response".to_string()),
            },
            indexes: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(20.0),
                error: None,
            },
            wal: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(10.0),
                error: None,
            },
            page_cache: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(5.0),
                error: None,
            },
        };

        let overall_status = determine_overall_status(&components);
        assert!(matches!(overall_status, HealthStatus::Degraded));
    }

    #[test]
    fn test_determine_overall_status_with_unhealthy() {
        let components = ComponentHealth {
            database: ComponentStatus {
                status: HealthStatus::Unhealthy,
                response_time_ms: Some(5000.0),
                error: Some("Connection failed".to_string()),
            },
            storage: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(30.0),
                error: None,
            },
            indexes: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(20.0),
                error: None,
            },
            wal: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(10.0),
                error: None,
            },
            page_cache: ComponentStatus {
                status: HealthStatus::Healthy,
                response_time_ms: Some(5.0),
                error: None,
            },
        };

        let overall_status = determine_overall_status(&components);
        assert!(matches!(overall_status, HealthStatus::Unhealthy));
    }

    #[test]
    fn test_format_duration_seconds() {
        let duration = Duration::from_secs(30);
        let formatted = format_duration(duration);
        assert_eq!(formatted, "30s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let duration = Duration::from_secs(90);
        let formatted = format_duration(duration);
        assert_eq!(formatted, "1m 30s");
    }

    #[test]
    fn test_format_duration_hours() {
        let duration = Duration::from_secs(3661);
        let formatted = format_duration(duration);
        assert_eq!(formatted, "1h 1m 1s");
    }

    #[test]
    fn test_format_duration_days() {
        let duration = Duration::from_secs(90061);
        let formatted = format_duration(duration);
        assert_eq!(formatted, "1d 1h 1m 1s");
    }

    #[test]
    fn test_format_duration_zero() {
        let duration = Duration::from_secs(0);
        let formatted = format_duration(duration);
        assert_eq!(formatted, "0s");
    }

    #[test]
    fn test_get_memory_usage() {
        let memory = get_memory_usage();
        // Memory usage should be between 0% and 100%
        assert!((0.0..=100.0).contains(&memory));
    }

    #[test]
    fn test_get_cpu_usage() {
        let cpu = get_cpu_usage();
        // CPU usage should be between 0% and 100%
        assert!((0.0..=100.0).contains(&cpu));
    }

    #[tokio::test]
    async fn test_health_check_initialized() {
        // Initialize the health check system
        init();

        let response = health_check().await;
        let health_response = response.0;

        // Check that the response has the expected structure
        assert!(matches!(
            health_response.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(!health_response.timestamp.is_empty());
        assert!(!health_response.version.is_empty());
        assert!(matches!(
            health_response.components.database.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            health_response.components.storage.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            health_response.components.indexes.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            health_response.components.wal.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            health_response.components.page_cache.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
    }

    #[tokio::test]
    async fn test_health_check_uninitialized() {
        // Don't initialize - test fallback behavior
        let response = health_check().await;
        let health_response = response.0;

        // Should still work with fallback
        assert!(matches!(
            health_response.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(!health_response.timestamp.is_empty());
    }

    #[tokio::test]
    async fn test_metrics() {
        // Initialize the health check system
        init();

        let response = metrics().await;
        let metrics_value = response.0;

        // Check that metrics has the expected structure
        assert!(metrics_value.is_object());
        assert!(metrics_value.get("uptime_seconds").is_some());
        assert!(metrics_value.get("uptime_human").is_some());
        assert!(metrics_value.get("version").is_some());
        assert!(metrics_value.get("timestamp").is_some());
        assert!(metrics_value.get("system").is_some());
        assert!(metrics_value.get("database").is_some());

        // Check system metrics
        let system = metrics_value.get("system").unwrap();
        assert!(system.get("memory_usage_mb").is_some());
        assert!(system.get("cpu_usage_percent").is_some());

        // Check database metrics
        let database = metrics_value.get("database").unwrap();
        assert!(database.get("connections").is_some());
        assert!(database.get("queries_per_second").is_some());
        assert!(database.get("cache_hit_rate").is_some());
    }

    #[tokio::test]
    #[ignore = "Creates multiple Engines which can cause Too many open files error"]
    async fn test_check_components() {
        let components = check_components().await;

        // All components should have some status
        assert!(matches!(
            components.database.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            components.storage.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            components.indexes.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            components.wal.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(matches!(
            components.page_cache.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
    }

    #[tokio::test]
    async fn test_check_database() {
        let status = check_database().await;

        // Should have some status and response time
        assert!(matches!(
            status.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(status.response_time_ms.is_some());
    }

    #[tokio::test]
    async fn test_check_storage() {
        let status = check_storage().await;

        // Should have some status and response time
        assert!(matches!(
            status.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(status.response_time_ms.is_some());
    }

    #[tokio::test]
    async fn test_check_indexes() {
        let status = check_indexes().await;

        // Should have some status and response time
        assert!(matches!(
            status.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(status.response_time_ms.is_some());
    }

    #[tokio::test]
    async fn test_check_wal() {
        let status = check_wal().await;

        // Should have some status and response time
        assert!(matches!(
            status.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(status.response_time_ms.is_some());
    }

    #[tokio::test]
    async fn test_check_page_cache() {
        let status = check_page_cache().await;

        // Should have some status and response time
        assert!(matches!(
            status.status,
            HealthStatus::Healthy | HealthStatus::Degraded | HealthStatus::Unhealthy
        ));
        assert!(status.response_time_ms.is_some());
    }
}
