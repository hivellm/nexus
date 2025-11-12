//! MCP Authentication Middleware
//!
//! Validates API keys for MCP requests before forwarding to the MCP service.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use nexus_core::auth::middleware::AuthMiddleware;

/// MCP authentication middleware handler
/// Validates API key from Authorization header or X-API-Key header
pub async fn mcp_auth_middleware_handler(
    State(auth_middleware): State<AuthMiddleware>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<serde_json::Value>)> {
    // Extract API key from headers
    let api_key = AuthMiddleware::extract_api_key(request.headers());

    // If no API key provided, return 401
    let api_key = match api_key {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": {
                        "code": "AUTHENTICATION_REQUIRED",
                        "message": "MCP requests require authentication. Provide API key via Authorization: Bearer <key> or X-API-Key header."
                    }
                })),
            ));
        }
    };

    // Verify API key
    let auth_manager = auth_middleware.auth_manager();
    match auth_manager.verify_api_key(&api_key) {
        Ok(Some(verified_key)) => {
            // Check if API key is revoked or expired
            if verified_key.is_revoked {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": {
                            "code": "INVALID_TOKEN",
                            "message": "API key has been revoked"
                        }
                    })),
                ));
            }

            // Check if API key has expired
            if let Some(expires_at) = verified_key.expires_at {
                if expires_at < chrono::Utc::now() {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        axum::Json(serde_json::json!({
                            "error": {
                                "code": "INVALID_TOKEN",
                                "message": "API key has expired"
                            }
                        })),
                    ));
                }
            }

            // Check if API key has Admin permission (MCP operations require Admin or explicit MCP permission)
            // For now, we'll allow Admin permission. MCP-specific permission can be added later if needed.
            let has_permission = verified_key
                .permissions
                .contains(&nexus_core::auth::Permission::Admin)
                || verified_key
                    .permissions
                    .contains(&nexus_core::auth::Permission::Read); // Allow read-only MCP for now

            if !has_permission {
                return Err((
                    StatusCode::FORBIDDEN,
                    axum::Json(serde_json::json!({
                        "error": {
                            "code": "INSUFFICIENT_PERMISSIONS",
                            "message": "User does not have MCP permission"
                        }
                    })),
                ));
            }

            // Store API key info in request extensions for use in handlers
            request.extensions_mut().insert(verified_key);

            // Continue with the request
            Ok(next.run(request).await)
        }
        Ok(None) | Err(_) => {
            // Invalid API key
            Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": {
                        "code": "INVALID_TOKEN",
                        "message": "Invalid or expired API key"
                    }
                })),
            ))
        }
    }
}
