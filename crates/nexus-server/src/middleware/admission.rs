//! Global admission control for query-bearing endpoints.
//!
//! Every Cypher-executing path (`POST /cypher`, `POST /ingest`, the
//! RPC `CYPHER` command) acquires a permit from the shared
//! [`AdmissionQueue`] before touching the engine. The queue is a
//! bounded semaphore: callers that would push concurrency over
//! `max_concurrent` wait up to `queue_timeout`; beyond that they are
//! rejected with [`AdmissionError::Overloaded`], which the HTTP
//! layer maps to `503 Service Unavailable + Retry-After`.
//!
//! # Why a new layer on top of rate limiting?
//!
//! [`crate::middleware::RateLimiter`] gates requests on per-API-key
//! per-minute / per-hour quotas — it stops sustained floods, not
//! short bursts from a single authenticated caller. The RPC
//! transport already has a **per-connection** concurrency cap
//! (`protocol::rpc::server::in_flight`); this layer adds the
//! **global** cap the engine actually needs.
//!
//! # Example (in a handler)
//!
//! ```ignore
//! let _permit = server.admission.acquire().await.map_err(|e| ...)?;
//! let result = server.engine.write().await.execute_cypher(query)?;
//! // permit released on drop here
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout};

/// Tunables for the admission queue. Parse from env via
/// [`AdmissionConfig::from_env`].
#[derive(Debug, Clone, Copy)]
pub struct AdmissionConfig {
    /// Max concurrent permits. `0` disables admission control (every
    /// `acquire` short-circuits and returns an inert permit).
    pub max_concurrent: u32,
    /// How long a caller may wait for a permit before being
    /// rejected.
    pub queue_timeout: Duration,
    /// Master kill-switch. `false` is equivalent to
    /// `max_concurrent = 0`.
    pub enabled: bool,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(4);
        Self {
            max_concurrent: (cpus as u32).clamp(4, 32),
            queue_timeout: Duration::from_millis(5_000),
            enabled: true,
        }
    }
}

impl AdmissionConfig {
    /// Build from environment variables, falling back to
    /// [`Self::default`] for anything missing.
    ///
    /// Recognised:
    ///
    /// * `NEXUS_ADMISSION_MAX_CONCURRENT` — u32.
    /// * `NEXUS_ADMISSION_QUEUE_TIMEOUT_MS` — u64.
    /// * `NEXUS_ADMISSION_ENABLED` — `true` / `false`.
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Env parser with an injected lookup — lets tests drive the
    /// parser without touching the process environment.
    #[must_use]
    pub fn from_lookup<F>(lookup: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let mut cfg = Self::default();
        if let Some(v) = lookup("NEXUS_ADMISSION_MAX_CONCURRENT") {
            if let Ok(n) = v.parse::<u32>() {
                cfg.max_concurrent = n;
            }
        }
        if let Some(v) = lookup("NEXUS_ADMISSION_QUEUE_TIMEOUT_MS") {
            if let Ok(ms) = v.parse::<u64>() {
                cfg.queue_timeout = Duration::from_millis(ms);
            }
        }
        if let Some(v) = lookup("NEXUS_ADMISSION_ENABLED") {
            cfg.enabled = matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes");
        }
        cfg
    }
}

/// Errors surfaced by the queue.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum AdmissionError {
    /// Queue was saturated and the caller's wait budget expired.
    #[error("server overloaded: waited {waited_ms} ms, queue_timeout {timeout_ms} ms")]
    Overloaded {
        /// How long the caller actually waited.
        waited_ms: u64,
        /// Configured queue timeout.
        timeout_ms: u64,
    },
}

impl AdmissionError {
    /// Suggested `Retry-After` value in seconds.
    #[must_use]
    pub fn retry_after_seconds(&self) -> u64 {
        match self {
            // Wait a few multiples of the queue timeout before
            // retrying — enough for the backlog to drain.
            Self::Overloaded { timeout_ms, .. } => (timeout_ms / 1_000).max(1) * 2,
        }
    }
}

/// Shared semaphore + counters that every endpoint consults.
///
/// Cloneable through the usual `Arc` wrap. The counters are
/// `AtomicU64` so a Prometheus handler can read them without
/// acquiring a permit.
pub struct AdmissionQueue {
    cfg: AdmissionConfig,
    sem: Arc<Semaphore>,
    granted: AtomicU64,
    rejected: AtomicU64,
    in_flight: AtomicU64,
    /// Running total of `wait_micros` so the Prometheus histogram
    /// can expose `count` / `sum` without allocating a bucket
    /// array per-request here.
    wait_micros_total: AtomicU64,
}

impl std::fmt::Debug for AdmissionQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmissionQueue")
            .field("max_concurrent", &self.cfg.max_concurrent)
            .field("enabled", &self.cfg.enabled)
            .field("queue_timeout", &self.cfg.queue_timeout)
            .field("in_flight", &self.in_flight.load(Ordering::Relaxed))
            .field("granted", &self.granted.load(Ordering::Relaxed))
            .field("rejected", &self.rejected.load(Ordering::Relaxed))
            .finish()
    }
}

impl AdmissionQueue {
    /// Build from a config.
    #[must_use]
    pub fn new(cfg: AdmissionConfig) -> Self {
        let permits = if cfg.enabled && cfg.max_concurrent > 0 {
            cfg.max_concurrent as usize
        } else {
            // Disabled → give Semaphore::MAX_PERMITS so acquire is a
            // no-op. Still go through the same code path so metrics
            // stay consistent.
            Semaphore::MAX_PERMITS
        };
        Self {
            cfg,
            sem: Arc::new(Semaphore::new(permits)),
            granted: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            in_flight: AtomicU64::new(0),
            wait_micros_total: AtomicU64::new(0),
        }
    }

    /// Default-config queue (CPU-derived cap, 5 s timeout).
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(AdmissionConfig::default())
    }

    /// Configured knobs (useful for `GET /health` / Prometheus
    /// exposition).
    #[must_use]
    pub fn config(&self) -> AdmissionConfig {
        self.cfg
    }

    /// Ask for a permit. Blocks up to [`AdmissionConfig::queue_timeout`];
    /// rejects with [`AdmissionError::Overloaded`] afterwards.
    pub async fn acquire(self: &Arc<Self>) -> Result<AdmissionPermit, AdmissionError> {
        if !self.cfg.enabled {
            // Disabled — no-op permit.
            return Ok(AdmissionPermit {
                _inner: None,
                queue: self.clone(),
            });
        }
        let start = Instant::now();
        let acquire_fut = self.sem.clone().acquire_owned();
        let permit = match timeout(self.cfg.queue_timeout, acquire_fut).await {
            Ok(Ok(p)) => p,
            Ok(Err(_)) => {
                // Semaphore closed — treat as overload to avoid
                // silently passing through. Shouldn't happen unless
                // the queue is being torn down.
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(AdmissionError::Overloaded {
                    waited_ms: start.elapsed().as_millis() as u64,
                    timeout_ms: self.cfg.queue_timeout.as_millis() as u64,
                });
            }
            Err(_) => {
                self.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(AdmissionError::Overloaded {
                    waited_ms: self.cfg.queue_timeout.as_millis() as u64,
                    timeout_ms: self.cfg.queue_timeout.as_millis() as u64,
                });
            }
        };
        self.granted.fetch_add(1, Ordering::Relaxed);
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        self.wait_micros_total
            .fetch_add(start.elapsed().as_micros() as u64, Ordering::Relaxed);
        Ok(AdmissionPermit {
            _inner: Some(permit),
            queue: self.clone(),
        })
    }

    /// Observability snapshot.
    #[must_use]
    pub fn metrics(&self) -> AdmissionMetrics {
        AdmissionMetrics {
            granted_total: self.granted.load(Ordering::Relaxed),
            rejected_total: self.rejected.load(Ordering::Relaxed),
            in_flight: self.in_flight.load(Ordering::Relaxed),
            wait_micros_total: self.wait_micros_total.load(Ordering::Relaxed),
            configured_max_concurrent: self.cfg.max_concurrent,
            configured_queue_timeout_ms: self.cfg.queue_timeout.as_millis() as u64,
            enabled: self.cfg.enabled,
        }
    }
}

/// Read-only metrics snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionMetrics {
    pub granted_total: u64,
    pub rejected_total: u64,
    pub in_flight: u64,
    pub wait_micros_total: u64,
    pub configured_max_concurrent: u32,
    pub configured_queue_timeout_ms: u64,
    pub enabled: bool,
}

/// RAII permit. Dropping it releases the semaphore slot and
/// decrements the in-flight gauge.
pub struct AdmissionPermit {
    _inner: Option<tokio::sync::OwnedSemaphorePermit>,
    queue: Arc<AdmissionQueue>,
}

impl std::fmt::Debug for AdmissionPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdmissionPermit")
            .field("held", &self._inner.is_some())
            .finish()
    }
}

impl Drop for AdmissionPermit {
    fn drop(&mut self) {
        if self._inner.is_some() {
            self.queue.in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Request paths the admission queue actually gates. Light-weight
/// endpoints (`/health`, `/prometheus`, auth management, metadata
/// reads) bypass the queue so a saturated engine doesn't starve
/// diagnostics. The list is a prefix match — every route that
/// drives the Cypher executor or a bulk-ingest loop belongs here.
pub const HEAVY_PATH_PREFIXES: &[&str] =
    &["/cypher", "/ingest", "/knn_traverse", "/graphql", "/umicp"];

/// True iff `path` is one of the gated prefixes.
#[must_use]
pub fn is_heavy_path(path: &str) -> bool {
    HEAVY_PATH_PREFIXES
        .iter()
        .any(|p| path == *p || path.starts_with(&format!("{p}/")))
}

/// Axum middleware handler. Wraps query-bearing routes through the
/// global [`AdmissionQueue`]: if the caller can't get a permit
/// within the configured wait budget, the request is rejected with
/// `503 Service Unavailable + Retry-After`. Light-weight paths
/// (`/health`, auth, metrics, …) short-circuit — the queue is only
/// meaningful on the hot engine path.
pub async fn admission_middleware_handler(
    axum::extract::State(queue): axum::extract::State<Arc<AdmissionQueue>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if !is_heavy_path(request.uri().path()) {
        return next.run(request).await;
    }
    match queue.acquire().await {
        Ok(_permit) => next.run(request).await,
        Err(e) => admission_overloaded_response(&e),
    }
}

/// Build the canonical 503 response shape for an overload. Exposed
/// outside the middleware so the RPC dispatch path can share the
/// retry-after budget.
#[must_use]
pub fn admission_overloaded_response(err: &AdmissionError) -> axum::response::Response {
    use axum::http::{HeaderValue, StatusCode, header::RETRY_AFTER};
    use axum::response::IntoResponse;
    let retry_after_secs = err.retry_after_seconds();
    let body = serde_json::json!({
        "error": "server overloaded",
        "retry_after_ms": retry_after_secs * 1_000,
        "reason": err.to_string(),
    });
    let mut resp = (StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response();
    if let Ok(h) = HeaderValue::from_str(&retry_after_secs.to_string()) {
        resp.headers_mut().insert(RETRY_AFTER, h);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn queue(max: u32, to: Duration) -> Arc<AdmissionQueue> {
        Arc::new(AdmissionQueue::new(AdmissionConfig {
            max_concurrent: max,
            queue_timeout: to,
            enabled: true,
        }))
    }

    #[tokio::test]
    async fn grants_under_capacity() {
        let q = queue(2, Duration::from_millis(50));
        let p1 = q.acquire().await.unwrap();
        let p2 = q.acquire().await.unwrap();
        assert_eq!(q.metrics().in_flight, 2);
        drop(p1);
        assert_eq!(q.metrics().in_flight, 1);
        drop(p2);
        assert_eq!(q.metrics().in_flight, 0);
    }

    #[tokio::test]
    async fn rejects_over_capacity_after_timeout() {
        let q = queue(1, Duration::from_millis(30));
        let _held = q.acquire().await.unwrap();
        let err = q.acquire().await.unwrap_err();
        match err {
            AdmissionError::Overloaded { .. } => {}
        }
        assert_eq!(q.metrics().rejected_total, 1);
        assert_eq!(q.metrics().granted_total, 1);
    }

    #[tokio::test]
    async fn granted_after_wait_when_slot_frees() {
        let q = queue(1, Duration::from_millis(500));
        let held = q.acquire().await.unwrap();
        let q2 = q.clone();
        let waiter = tokio::spawn(async move {
            let _p = q2.acquire().await.unwrap();
        });
        // Give waiter a chance to enter the wait loop.
        tokio::time::sleep(Duration::from_millis(20)).await;
        drop(held);
        waiter.await.unwrap();
        assert_eq!(q.metrics().granted_total, 2);
        assert_eq!(q.metrics().rejected_total, 0);
    }

    #[tokio::test]
    async fn disabled_queue_is_no_op() {
        let q = Arc::new(AdmissionQueue::new(AdmissionConfig {
            max_concurrent: 0,
            queue_timeout: Duration::from_millis(1),
            enabled: false,
        }));
        // All acquires succeed immediately regardless of count.
        let mut permits = Vec::new();
        for _ in 0..100 {
            permits.push(q.acquire().await.unwrap());
        }
        assert_eq!(q.metrics().granted_total, 0); // no-op path doesn't count
        assert_eq!(q.metrics().rejected_total, 0);
    }

    #[tokio::test]
    async fn retry_after_is_at_least_one_second() {
        let q = queue(1, Duration::from_millis(500));
        let _h = q.acquire().await.unwrap();
        let err = q.acquire().await.unwrap_err();
        assert!(err.retry_after_seconds() >= 1);
    }

    #[test]
    fn default_config_derives_cpu_clamped() {
        let cfg = AdmissionConfig::default();
        assert!(cfg.max_concurrent >= 4);
        assert!(cfg.max_concurrent <= 32);
        assert_eq!(cfg.queue_timeout, Duration::from_millis(5_000));
        assert!(cfg.enabled);
    }

    #[test]
    fn env_parser_overrides_defaults() {
        let lookup = |k: &str| match k {
            "NEXUS_ADMISSION_MAX_CONCURRENT" => Some("7".into()),
            "NEXUS_ADMISSION_QUEUE_TIMEOUT_MS" => Some("250".into()),
            "NEXUS_ADMISSION_ENABLED" => Some("true".into()),
            _ => None,
        };
        let cfg = AdmissionConfig::from_lookup(lookup);
        assert_eq!(cfg.max_concurrent, 7);
        assert_eq!(cfg.queue_timeout, Duration::from_millis(250));
        assert!(cfg.enabled);
    }

    #[test]
    fn env_parser_disabled_flag_takes_effect() {
        let cfg = AdmissionConfig::from_lookup(|k| match k {
            "NEXUS_ADMISSION_ENABLED" => Some("false".into()),
            _ => None,
        });
        assert!(!cfg.enabled);
    }

    #[test]
    fn env_parser_rejects_garbage_falls_back_to_default() {
        let cfg = AdmissionConfig::from_lookup(|k| match k {
            "NEXUS_ADMISSION_MAX_CONCURRENT" => Some("not-a-number".into()),
            _ => None,
        });
        assert!(cfg.max_concurrent >= 4);
    }

    #[tokio::test]
    async fn metrics_reflect_activity() {
        let q = queue(2, Duration::from_millis(50));
        let _p1 = q.acquire().await.unwrap();
        let _p2 = q.acquire().await.unwrap();
        let _rejected = q.acquire().await.unwrap_err();
        let m = q.metrics();
        assert_eq!(m.granted_total, 2);
        assert_eq!(m.rejected_total, 1);
        assert_eq!(m.in_flight, 2);
        assert!(m.wait_micros_total < 50_000); // granted paths were fast
    }

    #[tokio::test]
    async fn debug_impl_includes_core_state() {
        let q = queue(3, Duration::from_millis(10));
        let d = format!("{q:?}");
        assert!(d.contains("max_concurrent: 3"));
        assert!(d.contains("enabled: true"));
    }

    #[tokio::test]
    async fn serial_permits_do_not_leak_counters() {
        let q = queue(1, Duration::from_millis(100));
        for _ in 0..5 {
            let _ = q.acquire().await.unwrap();
        }
        // All permits dropped; in_flight back to 0.
        assert_eq!(q.metrics().in_flight, 0);
        assert_eq!(q.metrics().granted_total, 5);
    }

    #[test]
    fn heavy_path_matcher_matches_prefixes() {
        assert!(is_heavy_path("/cypher"));
        assert!(is_heavy_path("/cypher/explain"));
        assert!(is_heavy_path("/ingest"));
        assert!(is_heavy_path("/knn_traverse"));
        assert!(is_heavy_path("/graphql"));
        assert!(!is_heavy_path("/health"));
        assert!(!is_heavy_path("/prometheus"));
        assert!(!is_heavy_path("/auth/users"));
        // Prefix-match guard — `/cypherx` must NOT be gated.
        assert!(!is_heavy_path("/cypherx"));
    }

    #[tokio::test]
    async fn overloaded_response_carries_retry_after_header() {
        use axum::http::{StatusCode, header::RETRY_AFTER};
        let err = AdmissionError::Overloaded {
            waited_ms: 5_000,
            timeout_ms: 5_000,
        };
        let resp = admission_overloaded_response(&err);
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        let retry = resp.headers().get(RETRY_AFTER).unwrap();
        assert!(retry.to_str().unwrap().parse::<u64>().unwrap() >= 1);
    }

    #[tokio::test]
    async fn middleware_short_circuits_light_paths() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::{Router, routing::get};
        use tower::ServiceExt;

        let q = queue(1, Duration::from_millis(10));
        let _held = q.acquire().await.unwrap(); // saturate the queue

        let app = Router::new()
            .route("/health", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                q.clone(),
                admission_middleware_handler,
            ));

        // /health is light → passes despite the saturated queue.
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn middleware_rejects_heavy_path_when_queue_is_full() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::{Router, routing::post};
        use tower::ServiceExt;

        let q = queue(1, Duration::from_millis(10));
        let _held = q.acquire().await.unwrap();

        let app = Router::new()
            .route("/cypher", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                q.clone(),
                admission_middleware_handler,
            ));

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cypher")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(q.metrics().rejected_total, 1);
    }

    #[tokio::test]
    async fn middleware_allows_heavy_path_when_slot_frees() {
        use axum::body::Body;
        use axum::http::Request;
        use axum::{Router, routing::post};
        use tower::ServiceExt;

        let q = queue(1, Duration::from_millis(500));
        let app = Router::new()
            .route("/cypher", post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                q.clone(),
                admission_middleware_handler,
            ));

        // First request grabs the slot + releases on handler return.
        let r1 = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cypher")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r1.status(), axum::http::StatusCode::OK);
        // Second request — slot is free again.
        let r2 = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/cypher")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r2.status(), axum::http::StatusCode::OK);
        assert_eq!(q.metrics().granted_total, 2);
        assert_eq!(q.metrics().in_flight, 0);
    }
}
