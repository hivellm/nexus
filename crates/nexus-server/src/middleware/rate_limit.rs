//! Rate Limiting Middleware for Graph Correlation API
//!
//! Implements token bucket algorithm with per-IP rate limiting

use axum::{
    extract::ConnectInfo,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: usize,
    /// Time window duration
    pub window_duration: Duration,
    /// Burst capacity (extra tokens)
    pub burst_capacity: usize,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_duration: Duration::from_secs(60),
            burst_capacity: 20,
        }
    }
}

/// Token bucket for rate limiting
#[derive(Debug, Clone)]
struct TokenBucket {
    /// Current token count
    tokens: f64,
    /// Maximum tokens (capacity)
    capacity: f64,
    /// Token refill rate (tokens per second)
    refill_rate: f64,
    /// Last refill time
    last_refill: Instant,
}

impl TokenBucket {
    fn new(max_requests: usize, window_duration: Duration, burst_capacity: usize) -> Self {
        let capacity = (max_requests + burst_capacity) as f64;
        let refill_rate = max_requests as f64 / window_duration.as_secs_f64();

        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self) -> bool {
        self.refill();

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        let new_tokens = elapsed * self.refill_rate;

        self.tokens = (self.tokens + new_tokens).min(self.capacity);
        self.last_refill = now;
    }

    fn remaining(&mut self) -> usize {
        self.refill();
        self.tokens.floor() as usize
    }

    fn reset_after(&self) -> Duration {
        if self.tokens >= self.capacity {
            Duration::from_secs(0)
        } else {
            let tokens_needed = 1.0 - self.tokens;
            let seconds = tokens_needed / self.refill_rate;
            Duration::from_secs_f64(seconds.max(0.0))
        }
    }
}

/// Rate limiter state
#[derive(Clone)]
pub struct RateLimiter {
    config: RateLimitConfig,
    buckets: Arc<RwLock<HashMap<String, TokenBucket>>>,
}

impl RateLimiter {
    /// Create new rate limiter with default config
    pub fn new() -> Self {
        Self::with_config(RateLimitConfig::default())
    }

    /// Create rate limiter with custom config
    pub fn with_config(config: RateLimitConfig) -> Self {
        Self {
            config,
            buckets: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if request is allowed for given key (usually IP address)
    pub async fn check_rate_limit(&self, key: &str) -> RateLimitResult {
        let mut buckets = self.buckets.write().await;

        let bucket = buckets.entry(key.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.config.max_requests,
                self.config.window_duration,
                self.config.burst_capacity,
            )
        });

        if bucket.try_consume() {
            RateLimitResult::Allowed {
                remaining: bucket.remaining(),
                reset_after: bucket.reset_after(),
            }
        } else {
            RateLimitResult::RateLimited {
                retry_after: bucket.reset_after(),
            }
        }
    }

    /// Clean up old entries (call periodically)
    pub async fn cleanup(&self) {
        let mut buckets = self.buckets.write().await;
        buckets.retain(|_, bucket| {
            bucket.tokens < bucket.capacity
                || bucket.last_refill.elapsed() < Duration::from_secs(300)
        });
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limit check result
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed {
        remaining: usize,
        reset_after: Duration,
    },
    RateLimited {
        retry_after: Duration,
    },
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    limiter: axum::extract::State<RateLimiter>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let key = addr.ip().to_string();

    match limiter.check_rate_limit(&key).await {
        RateLimitResult::Allowed {
            remaining,
            reset_after,
        } => {
            let mut response = next.run(request).await;
            let headers = response.headers_mut();

            headers.insert(
                "X-RateLimit-Limit",
                limiter.config.max_requests.to_string().parse().unwrap(),
            );
            headers.insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            headers.insert(
                "X-RateLimit-Reset",
                reset_after.as_secs().to_string().parse().unwrap(),
            );

            response
        }
        RateLimitResult::RateLimited { retry_after } => {
            let body = serde_json::json!({
                "error": "Rate limit exceeded",
                "retry_after_seconds": retry_after.as_secs(),
            });

            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("Content-Type", "application/json"),
                    ("Retry-After", &retry_after.as_secs().to_string()),
                ],
                serde_json::to_string(&body).unwrap(),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 100);
        assert_eq!(config.window_duration, Duration::from_secs(60));
        assert_eq!(config.burst_capacity, 20);
    }

    #[test]
    fn test_token_bucket_creation() {
        let bucket = TokenBucket::new(100, Duration::from_secs(60), 20);
        assert_eq!(bucket.capacity, 120.0);
        assert_eq!(bucket.tokens, 120.0);
    }

    #[test]
    fn test_token_bucket_consume() {
        let mut bucket = TokenBucket::new(100, Duration::from_secs(60), 20);
        assert!(bucket.try_consume());
        assert_eq!(bucket.tokens.floor() as usize, 119);
    }

    #[test]
    fn test_token_bucket_exhaustion() {
        let mut bucket = TokenBucket::new(5, Duration::from_secs(60), 0);

        // Consume all tokens
        for _ in 0..5 {
            assert!(bucket.try_consume());
        }

        // Should be rate limited
        assert!(!bucket.try_consume());
    }

    #[tokio::test]
    async fn test_rate_limiter_allows_requests() {
        let config = RateLimitConfig {
            max_requests: 10,
            window_duration: Duration::from_secs(60),
            burst_capacity: 5,
        };
        let limiter = RateLimiter::with_config(config);

        match limiter.check_rate_limit("test-ip").await {
            RateLimitResult::Allowed { remaining, .. } => {
                assert_eq!(remaining, 14); // 10 + 5 - 1
            }
            _ => panic!("Expected allowed"),
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_excess() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_duration: Duration::from_secs(60),
            burst_capacity: 0,
        };
        let limiter = RateLimiter::with_config(config);

        // Use all tokens
        for _ in 0..2 {
            match limiter.check_rate_limit("test-ip").await {
                RateLimitResult::Allowed { .. } => {}
                _ => panic!("Expected allowed"),
            }
        }

        // Should be rate limited
        match limiter.check_rate_limit("test-ip").await {
            RateLimitResult::RateLimited { .. } => {}
            _ => panic!("Expected rate limited"),
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_different_ips() {
        let limiter = RateLimiter::new();

        // Two different IPs should have separate limits
        match limiter.check_rate_limit("ip1").await {
            RateLimitResult::Allowed { .. } => {}
            _ => panic!("Expected allowed for ip1"),
        }

        match limiter.check_rate_limit("ip2").await {
            RateLimitResult::Allowed { .. } => {}
            _ => panic!("Expected allowed for ip2"),
        }
    }

    #[tokio::test]
    async fn test_cleanup_removes_old_entries() {
        let limiter = RateLimiter::new();

        limiter.check_rate_limit("test-ip").await;

        {
            let buckets = limiter.buckets.read().await;
            assert_eq!(buckets.len(), 1);
        }

        limiter.cleanup().await;

        // Cleanup shouldn't remove recent entries
        {
            let buckets = limiter.buckets.read().await;
            assert_eq!(buckets.len(), 1);
        }
    }
}
