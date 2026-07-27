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
    /// Master switch. `false` disables rate limiting entirely — every
    /// request, from every client, bypasses the limiter untouched.
    /// Configurable via `NEXUS_RATE_LIMIT_ENABLED`.
    pub enabled: bool,
    /// Maximum requests per window
    pub max_requests: usize,
    /// Time window duration
    pub window_duration: Duration,
    /// Burst capacity (extra tokens)
    pub burst_capacity: usize,
    /// When `true`, requests from a loopback client IP (127.0.0.1 /
    /// ::1) bypass the limiter entirely — no token is consumed and no
    /// `X-RateLimit-*` headers are attached. Mirrors the project's
    /// existing "auth disabled for localhost" posture and prevents
    /// local bulk loads (e.g. LDBC ingest) from tripping the limiter
    /// and being met with a connection reset instead of a clean 429
    /// (see the body-draining fix in [`rate_limit_middleware`]).
    /// Configurable via `NEXUS_RATE_LIMIT_EXEMPT_LOOPBACK`.
    pub exempt_loopback: bool,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_requests: 100,
            window_duration: Duration::from_secs(60),
            burst_capacity: 20,
            exempt_loopback: true,
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

/// Cap on the number of body bytes drained when a request is rejected
/// with 429. Mirrors [`crate::config::Config::default`]'s
/// `max_body_size_bytes` (16 MiB) — large enough to fully drain any
/// body accepted by `DefaultBodyLimit`, so hyper always sees the
/// stream reach EOF and keeps the keep-alive connection alive instead
/// of issuing a TCP RST. `to_bytes` errors past this cap are ignored
/// (the connection may still be reset for truly oversized bodies, but
/// that path already independently rejects via `DefaultBodyLimit`).
const RATE_LIMIT_BODY_DRAIN_CAP_BYTES: usize = 16 * 1024 * 1024;

/// Rate limiting middleware
///
/// Bypasses the limiter entirely (no token consumed, no headers set)
/// when disabled via config, or when the client is loopback and
/// `exempt_loopback` is set — see [`RateLimitConfig`]. Otherwise, a
/// request that exhausts its IP's token bucket is rejected with a
/// clean `429 Too Many Requests` after fully draining the request
/// body: returning 429 without consuming the body left hyper unable
/// to keep the connection alive for a large unread body (e.g. an
/// `/ingest` bulk payload), so it issued a TCP RST that surfaced to
/// the client mid-send as a connection-reset error instead of a
/// readable HTTP response.
pub async fn rate_limit_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    limiter: axum::extract::State<RateLimiter>,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    if !limiter.config.enabled || (limiter.config.exempt_loopback && addr.ip().is_loopback()) {
        return next.run(request).await;
    }

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
            // Drain the unread request body before responding. hyper will
            // not drain a body it never sees consumed; leaving it unread
            // here caused it to abort the keep-alive connection with a
            // TCP RST instead of delivering this 429 to the client (the
            // client would see e.g. WSAECONNABORTED mid-send on Windows).
            // Errors (body too large, malformed chunk, etc.) are ignored —
            // best-effort draining, not a correctness requirement: the
            // response below is returned either way.
            let (_parts, req_body) = request.into_parts();
            let _ = axum::body::to_bytes(req_body, RATE_LIMIT_BODY_DRAIN_CAP_BYTES).await;

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
            enabled: true,
            max_requests: 10,
            window_duration: Duration::from_secs(60),
            burst_capacity: 5,
            exempt_loopback: true,
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
            enabled: true,
            max_requests: 2,
            window_duration: Duration::from_secs(60),
            burst_capacity: 0,
            exempt_loopback: true,
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

    // H2 — exercises the limiter as it is now wired onto the HTTP server:
    // default config (`RateLimiter::new()`), consumed to exhaustion for one
    // key, confirming a different key is tracked independently. `oneshot`
    // cannot supply a real `ConnectInfo<SocketAddr>`, so the middleware
    // function itself is exercised at the integration level; this unit
    // test proves the underlying token-bucket accounting the middleware
    // relies on.
    #[tokio::test]
    async fn test_rate_limiter_default_budget_then_independent_key() {
        let limiter = RateLimiter::new();
        let config = RateLimitConfig::default();
        let budget = config.max_requests + config.burst_capacity;

        // Every request within the default budget must be allowed.
        for _ in 0..budget {
            match limiter.check_rate_limit("key-a").await {
                RateLimitResult::Allowed { .. } => {}
                RateLimitResult::RateLimited { .. } => {
                    panic!("request within budget must not be rate limited")
                }
            }
        }

        // One more request over budget must be rate limited.
        match limiter.check_rate_limit("key-a").await {
            RateLimitResult::RateLimited { .. } => {}
            RateLimitResult::Allowed { .. } => {
                panic!("request beyond budget must be rate limited")
            }
        }

        // A different key has its own bucket and is unaffected by key-a's
        // exhaustion.
        match limiter.check_rate_limit("key-b").await {
            RateLimitResult::Allowed { .. } => {}
            RateLimitResult::RateLimited { .. } => {
                panic!("a different key must not inherit another key's rate limit")
            }
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

    /// Builds a minimal router wired exactly like the production stack:
    /// `rate_limit_middleware` as a `from_fn_with_state` layer, with the
    /// per-request client address supplied via axum's `MockConnectInfo`
    /// (the documented test substitute for
    /// `into_make_service_with_connect_info`, which only runs over a real
    /// listener).
    fn router_with_limiter(limiter: RateLimiter, client_addr: SocketAddr) -> axum::Router {
        use axum::{Router, extract::connect_info::MockConnectInfo, routing::post};

        Router::new()
            .route("/", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                limiter,
                rate_limit_middleware,
            ))
            .layer(MockConnectInfo(client_addr))
    }

    #[tokio::test]
    async fn loopback_client_is_exempt_from_rate_limit() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let config = RateLimitConfig {
            enabled: true,
            max_requests: 1,
            window_duration: Duration::from_secs(60),
            burst_capacity: 0,
            exempt_loopback: true,
        };
        let limiter = RateLimiter::with_config(config);
        let addr: SocketAddr = "127.0.0.1:44100".parse().unwrap();
        let app = router_with_limiter(limiter, addr);

        // A one-request budget would 429 on the second call if the
        // exemption weren't in effect — send well past it.
        for _ in 0..5 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn disabled_limiter_bypasses_all_clients() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;

        let config = RateLimitConfig {
            enabled: false,
            max_requests: 1,
            window_duration: Duration::from_secs(60),
            burst_capacity: 0,
            exempt_loopback: false,
        };
        let limiter = RateLimiter::with_config(config);
        // Non-loopback client — only the `enabled` flag should matter here.
        let addr: SocketAddr = "203.0.113.5:51000".parse().unwrap();
        let app = router_with_limiter(limiter, addr);

        for _ in 0..5 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn non_loopback_over_limit_gets_429_with_retry_after() {
        use axum::{
            body::Body,
            http::{Request, header::RETRY_AFTER},
        };
        use tower::ServiceExt;

        let config = RateLimitConfig {
            enabled: true,
            max_requests: 1,
            window_duration: Duration::from_secs(60),
            burst_capacity: 0,
            exempt_loopback: true,
        };
        let limiter = RateLimiter::with_config(config);
        // Non-loopback, so `exempt_loopback` does not shield this client.
        let addr: SocketAddr = "203.0.113.9:51555".parse().unwrap();
        let app = router_with_limiter(limiter, addr);

        // First request consumes the entire one-token budget.
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        // Second request from the same IP is over budget and must be
        // rejected — with a non-empty body, to exercise the drain-before-429
        // path instead of the empty-body case above.
        let second = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/")
                    .body(Body::from(
                        "payload that must be drained before the 429 is returned",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(second.headers().get(RETRY_AFTER).is_some());
    }
}
