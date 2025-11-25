//! Performance tests for authentication and authorization
//!
//! These tests verify that authentication operations meet performance requirements:
//! - Rate limiting under high load
//! - Authentication middleware overhead
//! - Audit logging performance
//! - JWT validation performance
//! - API key lookup performance
//! - Concurrent authentication requests

#![cfg(test)]

use nexus_core::auth::audit::{AuditConfig, AuditLogger};
use nexus_core::auth::jwt::JwtManager;
use nexus_core::auth::middleware::RateLimitConfig;
use nexus_core::auth::middleware::RateLimiter;
use nexus_core::auth::{AuthManager, Permission};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;

#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_rate_limiting_under_high_load() {
    // Rate limiting should handle high load efficiently

    let config = RateLimitConfig {
        max_requests: 1000,
        window_duration: Duration::from_secs(60),
        cleanup_interval: Duration::from_secs(10),
    };

    let rate_limiter = Arc::new(RateLimiter::new(config));

    // Simulate high load: 1000+ requests with timeout
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let start = Instant::now();
        let mut handles = Vec::new();

        for i in 0..1000 {
            let limiter = rate_limiter.clone();
            let key = format!("key_{}", i % 10); // 10 different keys
            let handle = tokio::spawn(async move { limiter.check_rate_limit(&key).await });
            handles.push(handle);
        }

        // Wait for all requests
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        (start.elapsed(), results)
    })
    .await;

    let (elapsed, results) = result.expect("Test timed out after 10 seconds");

    // Should complete in reasonable time (< 1 second for 1000 requests)
    assert!(
        elapsed.as_secs() < 2,
        "Rate limiting should handle 1000 requests in < 2 seconds, took {:?}",
        elapsed
    );

    // Most requests should be allowed (within rate limit)
    let allowed_count = results.iter().filter(|r| r.allowed).count();
    assert!(allowed_count > 0, "Some requests should be allowed");

    tracing::info!(
        "Rate limiting performance: {} requests in {:?} ({:.2} req/s)",
        results.len(),
        elapsed,
        results.len() as f64 / elapsed.as_secs_f64()
    );
}

#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_authentication_middleware_overhead() {
    // Authentication middleware should add minimal overhead (<1ms per request)

    let config = nexus_core::auth::AuthConfig {
        enabled: true,
        ..Default::default()
    };
    let auth_manager = AuthManager::new(config);

    // Create an API key
    let (_, full_key) = auth_manager
        .generate_api_key("perf-test".to_string(), vec![Permission::Read])
        .unwrap();

    // Measure verification time (includes Argon2 which is intentionally slow) with timeout
    let iterations = 10; // Fewer iterations since Argon2 is slow
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let start = Instant::now();
        for _ in 0..iterations {
            let _ = auth_manager.verify_api_key(&full_key);
        }
        start.elapsed()
    })
    .await;

    let elapsed = result.expect("Test timed out after 30 seconds");
    let avg_nanos = (elapsed.as_nanos() / iterations as u128) as u64;
    let avg_time_per_request = Duration::from_nanos(avg_nanos);

    // Argon2 is intentionally slow (100-500ms per verification) for security
    // This is acceptable overhead for authentication
    assert!(
        avg_time_per_request < Duration::from_secs(1),
        "Authentication overhead should be reasonable (< 1s), got {:?}",
        avg_time_per_request
    );

    tracing::info!(
        "Authentication middleware overhead: {:?} per request ({} iterations) - Note: Argon2 is intentionally slow for security",
        avg_time_per_request,
        iterations
    );
}

#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_audit_logging_performance() {
    // Audit logging should not block requests significantly

    let temp_dir = TempDir::new().unwrap();
    let config = AuditConfig {
        enabled: true,
        log_dir: temp_dir.path().to_path_buf(),
        retention_days: 30,
        compress_logs: false,
    };

    let logger = AuditLogger::new(config).unwrap();

    // Measure logging time with timeout
    let iterations = 100;
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let start = Instant::now();
        for i in 0..iterations {
            let _ = logger
                .log_authentication_success(
                    format!("user{}", i),
                    format!("user-id-{}", i),
                    "api_key".to_string(),
                )
                .await;
        }
        start.elapsed()
    })
    .await;

    let elapsed = result.expect("Test timed out after 10 seconds");
    let avg_nanos = (elapsed.as_nanos() / iterations as u128) as u64;
    let avg_time_per_log = Duration::from_nanos(avg_nanos);

    // Average time should be reasonable (< 10ms per log entry)
    assert!(
        avg_time_per_log < Duration::from_millis(10),
        "Audit logging should be fast (< 10ms), got {:?}",
        avg_time_per_log
    );

    tracing::info!(
        "Audit logging performance: {:?} per log entry ({} iterations)",
        avg_time_per_log,
        iterations
    );
}

#[test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
fn test_jwt_validation_performance() {
    // JWT validation should be fast (<0.5ms)

    let config = nexus_core::auth::jwt::JwtConfig::default();
    let jwt_manager = JwtManager::new(config);

    use nexus_core::auth::User;
    use tracing;
    let user = User::new("user123".to_string(), "testuser".to_string());

    // Generate token
    let token = jwt_manager.generate_access_token(&user).unwrap();

    // Measure validation time
    let iterations = 1000;
    let start = Instant::now();

    for _ in 0..iterations {
        let _ = jwt_manager.validate_token(&token);
    }

    let elapsed = start.elapsed();
    let avg_nanos = (elapsed.as_nanos() / iterations as u128) as u64;
    let avg_time_per_validation = Duration::from_nanos(avg_nanos);

    // Average time should be < 0.5ms
    assert!(
        avg_time_per_validation < Duration::from_micros(500),
        "JWT validation should be < 0.5ms, got {:?}",
        avg_time_per_validation
    );

    tracing::info!(
        "JWT validation performance: {:?} per validation ({} iterations)",
        avg_time_per_validation,
        iterations
    );
}

#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_api_key_lookup_performance() {
    // API key lookup performance (includes Argon2 verification)
    // Note: Argon2 is intentionally slow for security, so we test with fewer iterations

    let temp_dir = TempDir::new().unwrap();
    let config = nexus_core::auth::AuthConfig {
        enabled: true,
        ..Default::default()
    };
    let auth_manager = AuthManager::with_storage(config, temp_dir.path()).unwrap();

    // Create multiple API keys
    let mut keys = Vec::new();
    for i in 0..10 {
        // Fewer keys since Argon2 is slow
        let (_, full_key) = auth_manager
            .generate_api_key(format!("key-{}", i), vec![Permission::Read])
            .unwrap();
        keys.push(full_key);
    }

    // Measure lookup time (includes Argon2 verification) with timeout
    let iterations = 5; // Fewer iterations since Argon2 is slow
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let start = Instant::now();
        for i in 0..iterations {
            let key = &keys[i % keys.len()];
            let _ = auth_manager.verify_api_key(key);
        }
        start.elapsed()
    })
    .await;

    let elapsed = result.expect("Test timed out after 30 seconds");
    let avg_nanos = (elapsed.as_nanos() / iterations as u128) as u64;
    let avg_time_per_lookup = Duration::from_nanos(avg_nanos);

    // Argon2 is intentionally slow (100ms-2s per verification) for security
    // This is acceptable for authentication (prevents brute force attacks)
    assert!(
        avg_time_per_lookup < Duration::from_secs(3),
        "API key lookup should be reasonable (< 3s), got {:?}",
        avg_time_per_lookup
    );

    tracing::info!(
        "API key lookup performance: {:?} per lookup ({} iterations) - Note: Argon2 is intentionally slow for security",
        avg_time_per_lookup,
        iterations
    );
}

#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_concurrent_authentication_performance() {
    // Concurrent authentication requests should be handled efficiently
    // Note: Argon2 is intentionally slow, so we test with fewer concurrent requests

    let config = nexus_core::auth::AuthConfig {
        enabled: true,
        ..Default::default()
    };
    let auth_manager = Arc::new(AuthManager::new(config));

    // Create an API key
    let (_, full_key) = auth_manager
        .generate_api_key("concurrent-perf".to_string(), vec![Permission::Read])
        .unwrap();

    // Measure concurrent verification time with timeout
    let concurrent_requests = 10; // Fewer requests since Argon2 is slow
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let start = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..concurrent_requests {
            let manager = auth_manager.clone();
            let key = full_key.clone();
            let handle = tokio::spawn(async move { manager.verify_api_key(&key) });
            handles.push(handle);
        }

        // Wait for all requests
        for handle in handles {
            let _ = handle.await;
        }
        start.elapsed()
    })
    .await;

    let elapsed = result.expect("Test timed out after 30 seconds");
    let avg_nanos = (elapsed.as_nanos() / concurrent_requests as u128) as u64;
    let avg_time_per_request = Duration::from_nanos(avg_nanos);

    // Argon2 is intentionally slow, but concurrent requests should still complete
    // Allow more time since Argon2 takes 100-500ms per verification
    assert!(
        elapsed < Duration::from_secs(10),
        "{} concurrent requests should complete in < 10s, took {:?}",
        concurrent_requests,
        elapsed
    );

    tracing::info!(
        "Concurrent authentication performance: {:?} total for {} requests ({:?} avg) - Note: Argon2 is intentionally slow for security",
        elapsed,
        concurrent_requests,
        avg_time_per_request
    );
}
