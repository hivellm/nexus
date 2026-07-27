//! Integration tests for server-hardening request-pipeline layers
//! (`phase0_fix-server-secure-defaults-and-dos`): H4 request timeout and M4
//! CORS allow-list. Both are exercised via `tower::ServiceExt::oneshot` against
//! a minimal router that applies the same layer the server wires in `main.rs`.

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use axum::routing::get;
use std::time::Duration;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// M4 — CORS allow-list (replaces the old `CorsLayer::permissive()`)
// ---------------------------------------------------------------------------

fn cors_app(origins: &[String]) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(nexus_server::middleware::build_cors_layer(origins))
}

#[tokio::test]
async fn cors_empty_allowlist_does_not_grant_arbitrary_origin() {
    let resp = cors_app(&[])
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "an empty CORS allow-list must not grant cross-origin access"
    );
}

#[tokio::test]
async fn cors_allowlisted_origin_is_permitted_others_are_not() {
    let origins = vec!["https://good.example".to_string()];

    let ok = cors_app(&origins)
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header("origin", "https://good.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        ok.headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("https://good.example"),
        "an allow-listed origin must be granted cross-origin access"
    );

    let denied = cors_app(&origins)
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header("origin", "https://evil.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        denied
            .headers()
            .get("access-control-allow-origin")
            .and_then(|v| v.to_str().ok()),
        Some("https://evil.example"),
        "an origin not on the allow-list must not be echoed back"
    );
}

// ---------------------------------------------------------------------------
// H4 — request timeout
// ---------------------------------------------------------------------------

fn timeout_app(timeout: Duration) -> Router {
    Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                "done"
            }),
        )
        .route("/fast", get(|| async { "ok" }))
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
}

#[tokio::test]
async fn timeout_aborts_a_slow_request() {
    let resp = timeout_app(Duration::from_millis(50))
        .oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::REQUEST_TIMEOUT,
        "a request exceeding the timeout must be aborted with 408"
    );
}

#[tokio::test]
async fn timeout_does_not_affect_a_fast_request() {
    let resp = timeout_app(Duration::from_secs(5))
        .oneshot(Request::builder().uri("/fast").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
