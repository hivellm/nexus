//! `hub_auth_middleware` end-to-end tests
//! (phase5_hub-integration §2).
//!
//! Spins up a minimal Axum router with the middleware layered on
//! top and exercises:
//!
//! - **Standalone mode**: when no `HubClient` is configured the
//!   middleware is a pass-through; requests without auth headers
//!   succeed and `UserContext` is absent from extensions.
//! - **Hub mode**: with `HubClient = Some(...)` requests must
//!   carry both `Authorization: Bearer <token>` and the
//!   gateway-set `X-Hivehub-User-Id`. Missing header → 401.
//!   Valid headers → 200, `UserContext` is in request extensions.
//! - **Malformed `X-Hivehub-User-Id`** is rejected with 401.

use axum::{
    Extension, Router,
    body::Body,
    http::{Request, StatusCode, header},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::get,
};
use nexus_server::hub::{HubClient, UserContext, hub_auth_middleware};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

async fn echo_user(ext: Option<Extension<UserContext>>) -> impl IntoResponse {
    match ext {
        Some(Extension(ctx)) => (StatusCode::OK, format!("authenticated:{}", ctx.user_id)),
        None => (StatusCode::OK, "anonymous".to_string()),
    }
}

fn build_app(hub: Option<HubClient>) -> Router {
    let state: Arc<Option<HubClient>> = Arc::new(hub);
    Router::new()
        .route("/whoami", get(echo_user))
        .layer(axum_middleware::from_fn_with_state(
            state,
            hub_auth_middleware,
        ))
}

#[tokio::test]
async fn standalone_mode_is_passthrough_for_anonymous_requests() {
    let app = build_app(None);
    let res = app
        .oneshot(
            Request::builder()
                .uri("/whoami")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1 << 16)
        .await
        .unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert_eq!(text, "anonymous");
}

#[tokio::test]
async fn hub_mode_rejects_request_without_bearer() {
    let hub = HubClient::new("test-key".to_string(), "http://localhost:12000".to_string()).unwrap();
    let app = build_app(Some(hub));
    let res = app
        .oneshot(
            Request::builder()
                .uri("/whoami")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn hub_mode_rejects_bearer_without_user_header() {
    let hub = HubClient::new("test-key".to_string(), "http://localhost:12000".to_string()).unwrap();
    let app = build_app(Some(hub));
    let res = app
        .oneshot(
            Request::builder()
                .uri("/whoami")
                .header(header::AUTHORIZATION, "Bearer abc")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn hub_mode_accepts_bearer_plus_gateway_user_header() {
    let hub = HubClient::new("test-key".to_string(), "http://localhost:12000".to_string()).unwrap();
    let app = build_app(Some(hub));
    let user = Uuid::new_v4();
    let res = app
        .oneshot(
            Request::builder()
                .uri("/whoami")
                .header(header::AUTHORIZATION, "Bearer abc")
                .header("X-Hivehub-User-Id", user.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body = axum::body::to_bytes(res.into_body(), 1 << 16)
        .await
        .unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert_eq!(text, format!("authenticated:{user}"));
}

#[tokio::test]
async fn hub_mode_rejects_malformed_user_id_header() {
    let hub = HubClient::new("test-key".to_string(), "http://localhost:12000".to_string()).unwrap();
    let app = build_app(Some(hub));
    let res = app
        .oneshot(
            Request::builder()
                .uri("/whoami")
                .header(header::AUTHORIZATION, "Bearer abc")
                .header("X-Hivehub-User-Id", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}
