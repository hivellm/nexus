//! Authentication middleware wrapper for Nexus Server

use crate::NexusServer;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use nexus_core::auth::{AuthError, middleware::AuthMiddleware};
use std::sync::Arc;

/// Create authentication middleware from NexusServer
pub fn create_auth_middleware(server: Arc<NexusServer>, require_auth: bool) -> AuthMiddleware {
    AuthMiddleware::with_audit_logger(
        server.auth_manager.clone(),
        require_auth,
        server.audit_logger.clone(),
    )
}

/// Authentication middleware handler for Axum
pub async fn auth_middleware_handler(
    State(auth_middleware): State<AuthMiddleware>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    nexus_core::auth::middleware::auth_middleware_handler(
        axum::extract::State(auth_middleware),
        request,
        next,
    )
    .await
}

/// Helper function to check if a route requires authentication
pub fn route_requires_auth(path: &str) -> bool {
    // Public endpoints that don't require authentication
    let public_paths = [
        "/",
        "/health",
        "/metrics",
        "/openapi.json",
        // Auth endpoints themselves might need special handling
        // For now, we'll require auth for /auth/* endpoints too
    ];

    !public_paths.contains(&path)
}
