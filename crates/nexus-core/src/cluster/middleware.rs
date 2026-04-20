//! Axum middleware for cluster-mode quota enforcement.
//!
//! Runs AFTER the auth middleware: by the time it fires, the
//! request's extensions either contain a [`UserContext`] (cluster
//! mode, authenticated) or they don't (standalone mode). The
//! middleware's job is simple:
//!
//! 1. If no `UserContext` is present → forward unchanged. This is
//!    the standalone-mode fast path, exercised on the vast majority
//!    of deployments; it must be branchless beyond a single lookup
//!    and must not allocate.
//! 2. If a `UserContext` is present → ask the [`QuotaProvider`]
//!    whether the tenant may perform another chargeable request.
//!    On `Allow` → tag the response with `X-RateLimit-*` headers
//!    and forward. On `Deny` → short-circuit with `429 Too Many
//!    Requests` and a `Retry-After` hint when the provider
//!    supplied one.
//!
//! The middleware is agnostic of the provider flavour —
//! [`LocalQuotaProvider`](super::quota::LocalQuotaProvider) in
//! tests / standalone, a remote-control-plane provider in SaaS
//! deployments. That indirection is the only reason the quota
//! layer lives behind a trait at all.

#![cfg(feature = "axum")]

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};

use super::context::UserContext;
use super::quota::{QuotaDecision, QuotaProvider};

/// Error body returned on a 429. Mirrors the shape of
/// [`AuthError`](crate::auth::AuthError) so SDK callers can share
/// one JSON decoder across both rejection classes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_seconds: Option<u64>,
}

impl QuotaError {
    fn rate_limit_exceeded(reason: impl Into<String>, retry_after_s: Option<u64>) -> Self {
        Self {
            code: "RATE_LIMIT_EXCEEDED".into(),
            message: reason.into(),
            retry_after_seconds: retry_after_s,
        }
    }
}

/// Shared state the quota middleware needs.
///
/// Held as `Arc<dyn QuotaProvider>` so the server can swap in
/// [`LocalQuotaProvider`](super::quota::LocalQuotaProvider) or a
/// future HiveHub-backed implementation without any of the
/// middleware's call sites changing shape.
#[derive(Clone)]
pub struct QuotaMiddlewareState {
    provider: Arc<dyn QuotaProvider>,
}

impl QuotaMiddlewareState {
    pub fn new(provider: Arc<dyn QuotaProvider>) -> Self {
        Self { provider }
    }
}

/// Axum middleware that enforces per-tenant rate limits.
///
/// Install after the auth middleware in the router layer stack —
/// the order matters, because the quota layer refuses to run
/// without a `UserContext` already populated by auth.
pub async fn quota_middleware_handler(
    State(state): State<QuotaMiddlewareState>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<QuotaError>)> {
    // Standalone-mode / unauthenticated request — pass through.
    let ctx = match request.extensions().get::<UserContext>() {
        Some(ctx) => ctx.clone(),
        None => return Ok(next.run(request).await),
    };

    match state.provider.check_rate(ctx.namespace()) {
        QuotaDecision::Allow { remaining } => {
            let mut response = next.run(request).await;
            let headers = response.headers_mut();
            attach_allow_headers(headers, remaining);
            Ok(response)
        }
        QuotaDecision::Deny {
            reason,
            retry_after,
        } => {
            let retry_after_s = retry_after.map(|d| d.as_secs().max(1));
            Err((
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(QuotaError::rate_limit_exceeded(reason, retry_after_s)),
            ))
        }
    }
}

/// Best-effort header emission. A malformed `HeaderValue` on this
/// path is almost impossible (we only format from numbers), but
/// `from_str` is fallible by type so we swallow the error instead
/// of 500-ing a request that otherwise succeeded — header absence
/// is always safer than header corruption.
fn attach_allow_headers(headers: &mut HeaderMap, remaining: Option<u64>) {
    if let Some(r) = remaining {
        if let Ok(v) = HeaderValue::from_str(&r.to_string()) {
            headers.insert("x-ratelimit-remaining", v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster::namespace::UserNamespace;
    use crate::cluster::quota::{LocalQuotaProvider, QuotaSnapshot, UsageDelta};
    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::extract::Extension;
    use axum::http::Request;
    use axum::routing::get;

    async fn hello() -> &'static str {
        "ok"
    }

    fn router_with_quota(provider: Arc<dyn QuotaProvider>) -> Router {
        Router::new()
            .route("/echo", get(hello))
            .layer(axum::middleware::from_fn_with_state(
                QuotaMiddlewareState::new(provider),
                quota_middleware_handler,
            ))
    }

    fn authed(ns: &UserNamespace) -> Request<Body> {
        let mut req = Request::builder()
            .uri("/echo")
            .body(Body::empty())
            .expect("build request");
        let ctx = UserContext::unrestricted(ns.clone(), "key-1");
        req.extensions_mut().insert(ctx);
        req
    }

    #[tokio::test]
    async fn unauthenticated_request_passes_through() {
        // `/echo` with no UserContext — the quota layer must not
        // even consult the provider, otherwise standalone mode
        // would see made-up namespaces.
        let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(Default::default());
        let app = router_with_quota(provider.clone());

        let response = tower::ServiceExt::oneshot(
            app,
            Request::builder().uri("/echo").body(Body::empty()).unwrap(),
        )
        .await
        .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        // Provider was never touched → snapshot for any ns is None.
        let ns = UserNamespace::new("alice").unwrap();
        assert!(provider.snapshot(&ns).is_none());
    }

    #[tokio::test]
    async fn authenticated_request_within_limit_gets_rate_header() {
        let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(Default::default());
        let app = router_with_quota(provider.clone());
        let ns = UserNamespace::new("alice").unwrap();

        let response = tower::ServiceExt::oneshot(app, authed(&ns)).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let remaining = response
            .headers()
            .get("x-ratelimit-remaining")
            .expect("Allow path must set X-RateLimit-Remaining");
        assert!(
            remaining.to_str().unwrap().parse::<u64>().unwrap() > 0,
            "remaining must be positive on a fresh tenant"
        );
    }

    #[tokio::test]
    async fn rate_limit_exceeded_returns_429() {
        // Very tight quota — 2 req/min — so we can blow through it
        // synchronously in one test without waiting on a real
        // window expiry.
        let defaults = crate::cluster::config::TenantDefaults {
            storage_mb: 1,
            requests_per_minute: 2,
            requests_per_hour: 10,
        };
        let provider: Arc<dyn QuotaProvider> = LocalQuotaProvider::new(defaults);
        let app = router_with_quota(provider.clone());
        let ns = UserNamespace::new("alice").unwrap();

        // Two allowed requests, then a denial.
        for _ in 0..2 {
            let r = tower::ServiceExt::oneshot(app.clone(), authed(&ns))
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::OK);
        }

        let denied = tower::ServiceExt::oneshot(app, authed(&ns)).await.unwrap();
        assert_eq!(denied.status(), StatusCode::TOO_MANY_REQUESTS);

        let body = to_bytes(denied.into_body(), 4096).await.unwrap();
        let err: QuotaError = serde_json::from_slice(&body).unwrap();
        assert_eq!(err.code, "RATE_LIMIT_EXCEEDED");
        assert!(
            err.message.contains("per-minute"),
            "reason: {}",
            err.message
        );
        assert!(err.retry_after_seconds.is_some());
    }
}
