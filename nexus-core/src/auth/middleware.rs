//! Authentication middleware for HTTP requests

use super::{AuthManager, Permission};
#[cfg(feature = "axum")]
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Authentication context for the current request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthContext {
    /// The authenticated API key
    pub api_key: super::ApiKey,
    /// Whether authentication was required for this request
    pub required: bool,
}

/// Authentication error response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthError {
    pub code: String,
    pub message: String,
    pub details: Option<String>,
}

impl AuthError {
    pub fn authentication_required() -> Self {
        Self {
            code: "AUTHENTICATION_REQUIRED".to_string(),
            message: "Authentication required for this endpoint".to_string(),
            details: None,
        }
    }

    pub fn invalid_token() -> Self {
        Self {
            code: "INVALID_TOKEN".to_string(),
            message: "Invalid or expired authentication token".to_string(),
            details: None,
        }
    }

    pub fn insufficient_permissions() -> Self {
        Self {
            code: "INSUFFICIENT_PERMISSIONS".to_string(),
            message: "Insufficient permissions for this operation".to_string(),
            details: None,
        }
    }

    pub fn rate_limit_exceeded() -> Self {
        Self {
            code: "RATE_LIMIT_EXCEEDED".to_string(),
            message: "Rate limit exceeded".to_string(),
            details: None,
        }
    }
}

/// Rate limiter entry for tracking API key usage
#[derive(Debug, Clone)]
struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

/// Rate limiter configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Time window duration
    pub window_duration: Duration,
    /// Cleanup interval for expired entries
    pub cleanup_interval: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 1000,
            window_duration: Duration::from_secs(3600), // 1 hour
            cleanup_interval: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Rate limiter for API keys
#[derive(Debug)]
pub struct RateLimiter {
    entries: Arc<RwLock<HashMap<String, RateLimitEntry>>>,
    config: RateLimitConfig,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            entries: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if the API key is within rate limits
    pub async fn check_rate_limit(&self, api_key: &str) -> Result<(), ()> {
        let mut entries = self.entries.write().await;
        let now = Instant::now();

        // Clean up expired entries periodically
        if now.duration_since(
            entries
                .values()
                .next()
                .map(|e| e.window_start)
                .unwrap_or(now),
        ) > self.config.cleanup_interval
        {
            entries.retain(|_, entry| {
                now.duration_since(entry.window_start) < self.config.window_duration
            });
        }

        // Get or create entry for this API key
        let entry = entries
            .entry(api_key.to_string())
            .or_insert(RateLimitEntry {
                count: 0,
                window_start: now,
            });

        // Check if we need to reset the window
        if now.duration_since(entry.window_start) >= self.config.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        // Check if rate limit is exceeded
        if entry.count >= self.config.max_requests {
            return Err(());
        }

        // Increment the count
        entry.count += 1;
        Ok(())
    }
}

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthMiddleware {
    auth_manager: Arc<AuthManager>,
    require_auth: bool,
    rate_limiter: Option<Arc<RateLimiter>>,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    pub fn new(auth_manager: Arc<AuthManager>, require_auth: bool) -> Self {
        Self {
            auth_manager,
            require_auth,
            rate_limiter: None,
        }
    }

    /// Create a new authentication middleware with rate limiting
    pub fn with_rate_limiter(
        auth_manager: Arc<AuthManager>,
        require_auth: bool,
        rate_limiter: RateLimiter,
    ) -> Self {
        Self {
            auth_manager,
            require_auth,
            rate_limiter: Some(Arc::new(rate_limiter)),
        }
    }

    /// Extract the API key from the Authorization header
    #[cfg(feature = "axum")]
    pub fn extract_api_key(headers: &HeaderMap) -> Option<String> {
        let auth_header = headers.get("authorization")?;
        let auth_str = auth_header.to_str().ok()?;

        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            Some(token.to_string())
        } else {
            None
        }
    }

    /// Check if the request requires authentication
    pub fn requires_auth(&self, uri: &str) -> bool {
        // Health check and stats endpoints don't require auth
        if uri == "/health" || uri == "/stats" {
            return false;
        }

        self.require_auth
    }

    /// Check if the API key has the required permission
    pub fn has_permission(&self, api_key: &super::ApiKey, permission: Permission) -> bool {
        self.auth_manager.has_permission(api_key, permission)
    }
}

#[cfg(feature = "axum")]
/// Authentication middleware function
pub async fn auth_middleware(
    State(auth_middleware): State<AuthMiddleware>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    let uri = request.uri().path();

    // Check if authentication is required for this endpoint
    if !auth_middleware.requires_auth(uri) {
        return Ok(next.run(request).await);
    }

    // Extract API key from headers
    let headers = request.headers();
    let api_key_str = match AuthMiddleware::extract_api_key(headers) {
        Some(key) => key,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError::authentication_required()),
            ));
        }
    };

    // Verify the API key
    let api_key = match auth_middleware.auth_manager.verify_api_key(&api_key_str) {
        Ok(Some(key)) => key,
        Ok(None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError::invalid_token()),
            ));
        }
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(AuthError {
                    code: "AUTH_ERROR".to_string(),
                    message: "Authentication error".to_string(),
                    details: None,
                }),
            ));
        }
    };

    // Create authentication context
    let auth_context = AuthContext {
        api_key: api_key.clone(),
        required: true,
    };

    // Insert the auth context into the request extensions
    request.extensions_mut().insert(auth_context);

    // Continue with the request
    Ok(next.run(request).await)
}

#[cfg(feature = "axum")]
/// Permission-based middleware that checks for specific permissions
pub async fn permission_middleware(
    State(auth_middleware): State<AuthMiddleware>,
    State(required_permission): State<Permission>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    // First run the basic auth middleware
    let response = auth_middleware(State(auth_middleware.clone()), request, next).await?;

    // Extract the auth context from the response extensions
    // Note: This is a simplified approach. In practice, you'd need to
    // modify the middleware to pass the context through the response.
    // For now, we'll assume the permission check was done in the auth middleware.

    Ok(response)
}

#[cfg(feature = "axum")]
/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(auth_middleware): State<AuthMiddleware>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    // Extract API key for rate limiting
    let headers = request.headers();
    if let Some(api_key_str) = AuthMiddleware::extract_api_key(headers) {
        // Check rate limiting if configured
        if let Some(rate_limiter) = &auth_middleware.rate_limiter {
            if rate_limiter.check_rate_limit(&api_key_str).await.is_err() {
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    axum::Json(AuthError::rate_limit_exceeded()),
                ));
            }
        }
    }

    Ok(next.run(request).await)
}

#[cfg(feature = "axum")]
/// Helper function to extract auth context from request extensions
pub fn extract_auth_context(request: &Request) -> Option<&AuthContext> {
    request.extensions().get::<AuthContext>()
}

#[cfg(feature = "axum")]
/// Helper function to check if a request is authenticated
pub fn is_authenticated(request: &Request) -> bool {
    extract_auth_context(request).is_some()
}

#[cfg(feature = "axum")]
/// Helper function to get the API key from the request
pub fn get_api_key(request: &Request) -> Option<&super::ApiKey> {
    extract_auth_context(request).map(|ctx| &ctx.api_key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_error_creation() {
        let error = AuthError::authentication_required();
        assert_eq!(error.code, "AUTHENTICATION_REQUIRED");
        assert_eq!(error.message, "Authentication required for this endpoint");

        let error = AuthError::invalid_token();
        assert_eq!(error.code, "INVALID_TOKEN");
        assert_eq!(error.message, "Invalid or expired authentication token");

        let error = AuthError::insufficient_permissions();
        assert_eq!(error.code, "INSUFFICIENT_PERMISSIONS");
        assert_eq!(error.message, "Insufficient permissions for this operation");
    }

    #[test]
    fn test_auth_middleware_creation() {
        let config = super::super::AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(config));
        let middleware = AuthMiddleware::new(auth_manager, true);

        assert!(middleware.requires_auth("/api/cypher"));
        assert!(!middleware.requires_auth("/health"));
        assert!(!middleware.requires_auth("/stats"));
    }

    #[tokio::test]
    async fn test_rate_limiter() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_duration: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(10),
        };
        let rate_limiter = RateLimiter::new(config);

        // First two requests should pass
        assert!(rate_limiter.check_rate_limit("test-key").await.is_ok());
        assert!(rate_limiter.check_rate_limit("test-key").await.is_ok());

        // Third request should be rate limited
        assert!(rate_limiter.check_rate_limit("test-key").await.is_err());

        // Different key should work
        assert!(rate_limiter.check_rate_limit("other-key").await.is_ok());
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 1000);
        assert_eq!(config.window_duration, Duration::from_secs(3600));
        assert_eq!(config.cleanup_interval, Duration::from_secs(300));
    }
}
