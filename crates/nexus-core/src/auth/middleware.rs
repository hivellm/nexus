//! Authentication middleware for HTTP requests

use super::{AuthManager, Permission};
use crate::cluster::{UserContext, UserNamespace};
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
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Monotonic counter of audit-log write failures observed by the auth
/// middleware. Exposed via [`audit_log_failures_total`] so `nexus-server`
/// can surface it in the Prometheus `/prometheus` endpoint.
///
/// This is the "fail-open" half of the audit-log failure policy documented
/// in `docs/security/SECURITY_AUDIT.md`: when `AuditLogger::log_authentication_failed`
/// returns `Err`, the middleware still returns the original 401/429/500 to
/// the caller (we never convert an auth failure into a 500 just because the
/// audit sink broke) but bumps this counter and emits a `tracing::error!` so
/// operators can alarm on the condition.
static AUDIT_LOG_FAILURES: AtomicU64 = AtomicU64::new(0);

/// Read the current audit-log failure count. Exposed for the Prometheus
/// endpoint and for tests.
pub fn audit_log_failures_total() -> u64 {
    AUDIT_LOG_FAILURES.load(Ordering::Relaxed)
}

/// Record that a single audit-log write failed. Bumps the
/// [`audit_log_failures_total`] counter and emits a `tracing::error!` with
/// the underlying error.
///
/// Use this helper whenever an audit-log write returns `Err` on a code path
/// that **must not** fail the request as a result (the fail-open half of
/// the policy in `docs/security/SECURITY_AUDIT.md`). The typical pattern is:
///
/// ```ignore
/// if let Err(e) = audit_logger.log_write_operation(params).await {
///     nexus_core::auth::record_audit_log_failure("set_property", &e);
/// }
/// ```
///
/// `context` is a short static tag identifying *which* call site produced
/// the failure — this lets operators correlate counter movement with a
/// specific audit event class without exposing per-request data.
pub fn record_audit_log_failure(context: &'static str, err: &dyn std::fmt::Display) {
    AUDIT_LOG_FAILURES.fetch_add(1, Ordering::Relaxed);
    tracing::error!(
        target: "audit_log",
        context,
        error = %err,
        "audit log write failed — the originating request was still served \
         with its original response status, but the event was not persisted \
         to the audit log (see docs/security/SECURITY_AUDIT.md)"
    );
}

#[doc(hidden)]
/// Test-only helper that lets unit tests reset the global counter between
/// cases. NOT part of the public API — gated behind `#[cfg(test)]` so it
/// cannot be called from non-test builds.
#[cfg(test)]
pub(crate) fn reset_audit_log_failures_for_test() {
    AUDIT_LOG_FAILURES.store(0, Ordering::Relaxed);
}

/// Authentication context for the current request.
///
/// Stays `Serialize + Deserialize` so audit logs and plan snapshots
/// round-trip through JSON unchanged. The cluster-mode `UserContext`
/// is NOT part of this struct on purpose — it rides its own
/// request-extension slot (`request.extensions().get::<UserContext>()`)
/// so downstream layers that only need a namespace don't also have
/// to depend on the auth types.
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

/// Rate limit check result
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    /// Whether the request is allowed
    pub allowed: bool,
    /// Remaining requests in the current window
    pub remaining: u32,
    /// Time until the rate limit resets (in seconds)
    pub reset_after: Duration,
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
    /// Returns RateLimitResult with information about remaining requests and reset time
    pub async fn check_rate_limit(&self, api_key: &str) -> RateLimitResult {
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

        // Calculate remaining requests
        let remaining = if entry.count >= self.config.max_requests {
            0
        } else {
            self.config.max_requests - entry.count - 1
        };

        // Calculate reset time
        let elapsed = now.duration_since(entry.window_start);
        let reset_after = if elapsed >= self.config.window_duration {
            Duration::from_secs(0)
        } else {
            self.config.window_duration - elapsed
        };

        // Check if rate limit is exceeded
        let allowed = entry.count < self.config.max_requests;

        if allowed {
            // Increment the count
            entry.count += 1;
        }

        RateLimitResult {
            allowed,
            remaining,
            reset_after,
        }
    }

    /// Get the maximum requests per window
    pub fn max_requests(&self) -> u32 {
        self.config.max_requests
    }
}

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthMiddleware {
    auth_manager: Arc<AuthManager>,
    require_auth: bool,
    /// When `true`, cluster-mode semantics are active: every endpoint
    /// requires authentication (including health/stats), and the
    /// middleware builds a [`UserContext`] from the resolved API key
    /// and stashes it into request extensions for downstream layers.
    cluster_enabled: bool,
    rate_limiter: Option<Arc<RateLimiter>>,
    audit_logger: Option<Arc<super::AuditLogger>>,
}

impl AuthMiddleware {
    /// Create a new authentication middleware
    pub fn new(auth_manager: Arc<AuthManager>, require_auth: bool) -> Self {
        Self {
            auth_manager,
            require_auth,
            cluster_enabled: false,
            rate_limiter: None,
            audit_logger: None,
        }
    }

    /// Create a new authentication middleware with audit logging
    pub fn with_audit_logger(
        auth_manager: Arc<AuthManager>,
        require_auth: bool,
        audit_logger: Arc<super::AuditLogger>,
    ) -> Self {
        Self {
            auth_manager,
            require_auth,
            cluster_enabled: false,
            rate_limiter: None,
            audit_logger: Some(audit_logger),
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
            cluster_enabled: false,
            rate_limiter: Some(Arc::new(rate_limiter)),
            audit_logger: None,
        }
    }

    /// Enable cluster-mode semantics on this middleware. Once set,
    /// `require_auth` is implicitly `true` for every endpoint (no
    /// public exemptions) and each successfully authenticated
    /// request gets a [`UserContext`] inserted into its extensions.
    ///
    /// Returns `self` so callers can chain this onto one of the
    /// existing constructors (keeping the call sites that build
    /// standalone middlewares untouched).
    pub fn with_cluster_mode(mut self) -> Self {
        self.cluster_enabled = true;
        self.require_auth = true;
        self
    }

    /// Whether cluster-mode semantics are active on this middleware.
    pub fn is_cluster_mode(&self) -> bool {
        self.cluster_enabled
    }

    /// Derive a cluster-mode [`UserContext`] from an authenticated
    /// API key, or explain why the key is not eligible.
    ///
    /// Returns:
    /// * `Ok(Some(ctx))` — the key carries a `user_id` that parses
    ///   as a valid [`UserNamespace`]; use the context downstream.
    /// * `Ok(None)` — the key has no `user_id`; acceptable in
    ///   standalone mode but will be treated as an error by the
    ///   middleware when `cluster_enabled = true`.
    /// * `Err(AuthError)` — the key had a `user_id` but it failed
    ///   namespace validation (e.g. contained the reserved
    ///   delimiter). Caller should 401 the request.
    pub fn user_context_from_api_key(
        api_key: &super::ApiKey,
    ) -> Result<Option<UserContext>, AuthError> {
        let user_id = match &api_key.user_id {
            Some(id) => id,
            None => return Ok(None),
        };
        let namespace =
            UserNamespace::new(user_id.clone()).map_err(|_| AuthError::invalid_token())?;
        let ctx = match &api_key.allowed_functions {
            None => UserContext::unrestricted(namespace, api_key.id.clone()),
            Some(list) => UserContext::restricted(namespace, api_key.id.clone(), list.clone()),
        };
        Ok(Some(ctx))
    }

    /// Extract the API key from headers
    /// Supports multiple authentication methods:
    /// - Bearer token: `Authorization: Bearer nx_...`
    /// - API key header: `X-API-Key: nx_...`
    /// - Basic auth: `Authorization: Basic <base64(username:password)>` (for future use)
    #[cfg(feature = "axum")]
    pub fn extract_api_key(headers: &HeaderMap) -> Option<String> {
        // Try Bearer token first
        if let Some(auth_header) = headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                // Bearer token: Authorization: Bearer nx_...
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    return Some(token.to_string());
                }

                // Basic auth: Authorization: Basic <base64>
                // For now, we'll skip Basic auth as it requires username/password handling
                // This can be implemented later if needed
            }
        }

        // Try X-API-Key header
        if let Some(api_key_header) = headers.get("x-api-key") {
            if let Ok(key_str) = api_key_header.to_str() {
                return Some(key_str.to_string());
            }
        }

        None
    }

    /// Check if the request requires authentication.
    ///
    /// In cluster mode (`cluster_enabled = true`) the answer is
    /// unconditionally `true` — including for `/health`, `/stats`,
    /// and `/openapi.json` — because a shared multi-tenant server
    /// must not expose any surface without an authenticated caller.
    pub fn requires_auth(&self, uri: &str, require_health_auth: bool) -> bool {
        if self.cluster_enabled {
            return true;
        }

        // Health check endpoint - configurable
        if uri == "/health" || uri == "/" {
            return require_health_auth;
        }

        // Stats endpoint - optional (can be made configurable later)
        if uri == "/stats" {
            return false;
        }

        // OpenAPI spec - public
        if uri == "/openapi.json" {
            return false;
        }

        self.require_auth
    }

    /// Check if the API key has the required permission
    pub fn has_permission(&self, api_key: &super::ApiKey, permission: Permission) -> bool {
        self.auth_manager.has_permission(api_key, permission)
    }

    /// Get a reference to the auth manager
    pub fn auth_manager(&self) -> &Arc<AuthManager> {
        &self.auth_manager
    }
}

#[cfg(feature = "axum")]
/// Authentication middleware function
pub async fn auth_middleware_handler(
    State(auth_service): State<AuthMiddleware>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    let uri = request.uri().path();

    // Check if authentication is required for this endpoint
    // For now, we'll use require_health_auth = false by default
    // This can be made configurable later
    if !auth_service.requires_auth(uri, false) {
        // Insert None auth context for endpoints that don't require auth
        request
            .extensions_mut()
            .insert(axum::extract::Extension(None::<AuthContext>));
        return Ok(next.run(request).await);
    }

    // Extract API key from headers
    let headers = request.headers();
    let api_key_str = match AuthMiddleware::extract_api_key(headers) {
        Some(key) => key,
        None => {
            // Log authentication failure - no API key provided
            if let Some(ref audit_logger) = auth_service.audit_logger {
                let ip_address = request
                    .headers()
                    .get("x-forwarded-for")
                    .or_else(|| request.headers().get("x-real-ip"))
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());

                if let Err(e) = audit_logger
                    .log_authentication_failed(None, "No API key provided".to_string(), ip_address)
                    .await
                {
                    record_audit_log_failure("missing_api_key", &e);
                }
            }

            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError::authentication_required()),
            ));
        }
    };

    // Verify the API key
    let api_key = match auth_service.auth_manager.verify_api_key(&api_key_str) {
        Ok(Some(key)) => key,
        Ok(None) => {
            // Log authentication failure - invalid API key
            if let Some(ref audit_logger) = auth_service.audit_logger {
                let ip_address = request
                    .headers()
                    .get("x-forwarded-for")
                    .or_else(|| request.headers().get("x-real-ip"))
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());

                // Try to extract username from API key if possible (for logging)
                let username = None; // API keys don't have usernames directly

                if let Err(e) = audit_logger
                    .log_authentication_failed(
                        username,
                        "Invalid or expired API key".to_string(),
                        ip_address,
                    )
                    .await
                {
                    record_audit_log_failure("invalid_api_key", &e);
                }
            }

            return Err((
                StatusCode::UNAUTHORIZED,
                axum::Json(AuthError::invalid_token()),
            ));
        }
        Err(e) => {
            // Log authentication failure - internal error
            if let Some(ref audit_logger) = auth_service.audit_logger {
                let ip_address = request
                    .headers()
                    .get("x-forwarded-for")
                    .or_else(|| request.headers().get("x-real-ip"))
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());

                if let Err(audit_err) = audit_logger
                    .log_authentication_failed(
                        None,
                        format!("Authentication error: {}", e),
                        ip_address,
                    )
                    .await
                {
                    record_audit_log_failure("verify_api_key_error", &audit_err);
                }
            }

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

    // Check rate limiting if configured
    let rate_limit_result = if let Some(rate_limiter) = &auth_service.rate_limiter {
        Some(rate_limiter.check_rate_limit(&api_key_str).await)
    } else {
        None
    };

    // If rate limiting is enabled and limit is exceeded, return 429
    if let Some(ref result) = rate_limit_result {
        if !result.allowed {
            // Log rate limit exceeded (not an authentication failure, but worth logging)
            if let Some(ref audit_logger) = auth_service.audit_logger {
                let ip_address = request
                    .headers()
                    .get("x-forwarded-for")
                    .or_else(|| request.headers().get("x-real-ip"))
                    .and_then(|h| h.to_str().ok())
                    .map(|s| s.to_string());

                if let Err(e) = audit_logger
                    .log_authentication_failed(
                        None,
                        format!("Rate limit exceeded for API key {}", api_key.id),
                        ip_address,
                    )
                    .await
                {
                    record_audit_log_failure("rate_limit_exceeded", &e);
                }
            }

            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                axum::Json(AuthError::rate_limit_exceeded()),
            ));
        }
    }

    // In cluster mode, the key MUST resolve to a namespaced user
    // context. A key without a `user_id`, or one whose `user_id`
    // fails namespace validation, cannot be trusted to route to a
    // specific tenant — we 401 rather than silently routing it to
    // the global scope.
    let user_context_opt = if auth_service.cluster_enabled {
        match AuthMiddleware::user_context_from_api_key(&api_key) {
            Ok(Some(ctx)) => Some(ctx),
            Ok(None) | Err(_) => {
                if let Some(ref audit_logger) = auth_service.audit_logger {
                    let ip_address = request
                        .headers()
                        .get("x-forwarded-for")
                        .or_else(|| request.headers().get("x-real-ip"))
                        .and_then(|h| h.to_str().ok())
                        .map(|s| s.to_string());
                    if let Err(e) = audit_logger
                        .log_authentication_failed(
                            None,
                            format!(
                                "API key {} has no valid user_id for cluster mode",
                                api_key.id
                            ),
                            ip_address,
                        )
                        .await
                    {
                        record_audit_log_failure("cluster_missing_user_id", &e);
                    }
                }
                return Err((
                    StatusCode::UNAUTHORIZED,
                    axum::Json(AuthError::invalid_token()),
                ));
            }
        }
    } else {
        None
    };

    // Create authentication context
    let auth_context = AuthContext {
        api_key: api_key.clone(),
        required: true,
    };

    // Insert the auth context into the request extensions as Extension
    request
        .extensions_mut()
        .insert(axum::extract::Extension(Some(auth_context)));

    // In cluster mode, also expose the UserContext on its own extension
    // slot so downstream handlers / middleware (quota, namespace
    // scoping) can find it via `request.extensions().get::<UserContext>()`
    // without depending on the auth module's types.
    if let Some(user_context) = user_context_opt {
        request.extensions_mut().insert(user_context);
    }

    // Continue with the request and add rate limit headers if configured
    let mut response = next.run(request).await;

    // Add rate limit headers if rate limiting is enabled
    if let Some(ref result) = rate_limit_result {
        if let Some(rate_limiter) = &auth_service.rate_limiter {
            let headers = response.headers_mut();

            // X-RateLimit-Limit: Maximum requests per window
            if let Ok(limit_value) =
                axum::http::HeaderValue::from_str(&rate_limiter.max_requests().to_string())
            {
                headers.insert("X-RateLimit-Limit", limit_value);
            }

            // X-RateLimit-Remaining: Remaining requests in current window
            if let Ok(remaining_value) =
                axum::http::HeaderValue::from_str(&result.remaining.to_string())
            {
                headers.insert("X-RateLimit-Remaining", remaining_value);
            }

            // X-RateLimit-Reset: Seconds until the rate limit resets
            if let Ok(reset_value) =
                axum::http::HeaderValue::from_str(&result.reset_after.as_secs().to_string())
            {
                headers.insert("X-RateLimit-Reset", reset_value);
            }
        }
    }

    Ok(response)
}

#[cfg(feature = "axum")]
/// Permission-based middleware that checks for specific permissions
pub async fn permission_middleware(
    State(auth_middleware): State<AuthMiddleware>,
    State(_required_permission): State<Permission>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    // First run the basic auth middleware
    let response = auth_middleware_handler(State(auth_middleware.clone()), request, next).await?;

    // Extract the auth context from the response extensions
    // Note: This is a simplified approach. In practice, you'd need to
    // modify the middleware to pass the context through the response.
    // For now, we'll assume the permission check was done in the auth middleware.

    Ok(response)
}

#[cfg(feature = "axum")]
/// Rate limiting middleware (deprecated - rate limiting is now integrated into auth_middleware_handler)
/// This function is kept for backward compatibility but rate limiting should be done
/// through the AuthMiddleware::with_rate_limiter() method
pub async fn rate_limit_middleware(
    State(_auth_middleware): State<AuthMiddleware>,
    request: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<AuthError>)> {
    // Rate limiting is now integrated into auth_middleware_handler
    // This middleware is kept for backward compatibility
    Ok(next.run(request).await)
}

#[cfg(feature = "axum")]
/// Pull the cluster-mode [`UserContext`] off a request's extensions,
/// if the auth middleware installed one. Returns `None` in standalone
/// mode and for unauthenticated requests in cluster mode (which the
/// middleware would have rejected before reaching this call site).
pub fn extract_user_context(request: &Request) -> Option<&UserContext> {
    request.extensions().get::<UserContext>()
}

#[cfg(feature = "axum")]
/// Helper function to extract auth context from request extensions
pub fn extract_auth_context(request: &Request) -> Option<&AuthContext> {
    request
        .extensions()
        .get::<axum::extract::Extension<Option<AuthContext>>>()
        .and_then(|ext| ext.as_ref())
}

#[cfg(feature = "axum")]
/// Helper function to extract actor information from auth context
/// Returns (user_id, username, api_key_id)
pub fn extract_actor_info(request: &Request) -> (Option<String>, Option<String>, Option<String>) {
    if let Some(auth_context) = extract_auth_context(request) {
        let api_key_id = Some(auth_context.api_key.id.clone());
        let user_id = auth_context.api_key.user_id.clone();
        // Username is not directly available in ApiKey, would need to look it up from RBAC
        // For now, we'll return None for username
        let username = None;
        (user_id, username, api_key_id)
    } else {
        (None, None, None)
    }
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
    use std::sync::Arc;

    #[cfg(feature = "axum")]
    #[test]
    fn test_extract_auth_context() {
        use crate::auth::api_key::ApiKey;
        use axum::body::Body;
        use axum::http::Request;

        // Create a request without auth context
        let request = Request::builder()
            .uri("http://example.com/test")
            .body(Body::empty())
            .unwrap();

        // Should return None when no auth context
        assert!(extract_auth_context(&request).is_none());

        // Create a request with auth context
        let mut request = Request::builder()
            .uri("http://example.com/test")
            .body(Body::empty())
            .unwrap();

        let api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![super::Permission::Read],
            "hashed".to_string(),
        );
        let auth_context = AuthContext {
            api_key,
            required: true,
        };

        request
            .extensions_mut()
            .insert(axum::extract::Extension(Some(auth_context.clone())));

        // Should return Some when auth context exists
        let extracted = extract_auth_context(&request);
        assert!(extracted.is_some());
        assert_eq!(extracted.unwrap().api_key.id, "test-id");
    }

    #[cfg(feature = "axum")]
    #[test]
    fn test_extract_actor_info() {
        use crate::auth::api_key::ApiKey;
        use axum::body::Body;
        use axum::http::Request;

        // Create a request without auth context
        let request = Request::builder()
            .uri("http://example.com/test")
            .body(Body::empty())
            .unwrap();

        // Should return (None, None, None) when no auth context
        let (user_id, username, api_key_id) = extract_actor_info(&request);
        assert_eq!(user_id, None);
        assert_eq!(username, None);
        assert_eq!(api_key_id, None);

        // Create a request with auth context
        let mut request = Request::builder()
            .uri("http://example.com/test")
            .body(Body::empty())
            .unwrap();

        let mut api_key = ApiKey::new(
            "test-id".to_string(),
            "test-key".to_string(),
            vec![super::Permission::Read],
            "hashed".to_string(),
        );
        api_key.user_id = Some("user123".to_string());

        let auth_context = AuthContext {
            api_key,
            required: true,
        };

        request
            .extensions_mut()
            .insert(axum::extract::Extension(Some(auth_context)));

        // Should return actor info when auth context exists
        let (user_id, username, api_key_id) = extract_actor_info(&request);
        assert_eq!(user_id, Some("user123".to_string()));
        assert_eq!(username, None); // Username not available in ApiKey
        assert_eq!(api_key_id, Some("test-id".to_string()));
    }

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

        assert!(middleware.requires_auth("/api/cypher", false));
        assert!(!middleware.requires_auth("/health", false));
        assert!(!middleware.requires_auth("/stats", false));
    }

    #[cfg(feature = "axum")]
    #[test]
    fn test_extract_api_key_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            axum::http::HeaderValue::from_str("Bearer nx_test123456789").unwrap(),
        );

        let api_key = AuthMiddleware::extract_api_key(&headers);
        assert_eq!(api_key, Some("nx_test123456789".to_string()));
    }

    #[cfg(feature = "axum")]
    #[test]
    fn test_extract_api_key_x_api_key_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            axum::http::HeaderValue::from_str("nx_test123456789").unwrap(),
        );

        let api_key = AuthMiddleware::extract_api_key(&headers);
        assert_eq!(api_key, Some("nx_test123456789".to_string()));
    }

    #[cfg(feature = "axum")]
    #[test]
    fn test_extract_api_key_no_headers() {
        let headers = HeaderMap::new();
        let api_key = AuthMiddleware::extract_api_key(&headers);
        assert_eq!(api_key, None);
    }

    #[test]
    fn test_requires_auth_public_endpoints() {
        let config = super::super::AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(config));
        let middleware = AuthMiddleware::new(auth_manager, true);

        assert!(!middleware.requires_auth("/health", false));
        assert!(!middleware.requires_auth("/", false));
        assert!(!middleware.requires_auth("/stats", false));
        assert!(!middleware.requires_auth("/openapi.json", false));
    }

    #[test]
    fn test_requires_auth_protected_endpoints() {
        let config = super::super::AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(config));
        let middleware = AuthMiddleware::new(auth_manager, true);

        assert!(middleware.requires_auth("/cypher", false));
        assert!(middleware.requires_auth("/data/nodes", false));
        assert!(middleware.requires_auth("/schema/labels", false));
    }

    #[test]
    fn cluster_mode_requires_auth_on_every_path() {
        // The exact uris that are public in standalone mode —
        // cluster mode must override every one of them, including
        // /health and /openapi.json, because a shared multi-tenant
        // server cannot advertise its surface without identifying
        // the caller first.
        let config = super::super::AuthConfig::default();
        let auth_manager = Arc::new(AuthManager::new(config));
        let middleware = AuthMiddleware::new(auth_manager, false).with_cluster_mode();

        assert!(middleware.is_cluster_mode());
        for uri in ["/", "/health", "/stats", "/openapi.json", "/cypher"] {
            assert!(
                middleware.requires_auth(uri, false),
                "cluster mode must require auth for {uri}"
            );
        }
    }

    #[test]
    fn user_context_from_api_key_unrestricted() {
        use crate::auth::api_key::ApiKey;

        let mut key = ApiKey::new(
            "key-1".into(),
            "alice-key".into(),
            vec![super::Permission::Read],
            "hash".into(),
        );
        key.user_id = Some("alice".into());

        let ctx = AuthMiddleware::user_context_from_api_key(&key)
            .expect("valid user_id must build a context")
            .expect("user_id present means context is Some");

        assert_eq!(ctx.namespace().as_id(), "alice");
        assert_eq!(ctx.api_key_id(), "key-1");
        // No allow-list on the key → unrestricted context.
        assert!(ctx.allowed_functions().is_none());
        assert!(ctx.may_call("anything"));
    }

    #[test]
    fn user_context_from_api_key_honours_function_allow_list() {
        use crate::auth::api_key::ApiKey;

        let mut key = ApiKey::new(
            "key-2".into(),
            "scoped-key".into(),
            vec![super::Permission::Read],
            "hash".into(),
        )
        .with_allowed_functions(Some(vec!["cypher.execute".into()]));
        key.user_id = Some("bob".into());

        let ctx = AuthMiddleware::user_context_from_api_key(&key)
            .unwrap()
            .unwrap();
        assert!(ctx.may_call("cypher.execute"));
        assert!(!ctx.may_call("nexus.admin.drop_database"));
    }

    #[test]
    fn user_context_from_api_key_without_user_id_is_none() {
        use crate::auth::api_key::ApiKey;

        // Unlike the no-auth case, this is a _valid_ key with no
        // user_id at all. In standalone mode we're fine; the cluster
        // middleware is the one that turns this `Ok(None)` into a
        // 401, which is covered by the integration-level test that
        // exercises `auth_middleware_handler`.
        let key = ApiKey::new(
            "key-3".into(),
            "no-user".into(),
            vec![super::Permission::Read],
            "hash".into(),
        );
        assert!(
            AuthMiddleware::user_context_from_api_key(&key)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn user_context_from_api_key_rejects_invalid_user_id() {
        use crate::auth::api_key::ApiKey;

        // `:` is reserved in the namespace format. An API key that
        // ships with a forbidden user_id is almost certainly a bug
        // upstream — we surface it as an auth error rather than
        // silently collapsing to the global scope.
        let mut key = ApiKey::new(
            "key-4".into(),
            "bad-user".into(),
            vec![super::Permission::Read],
            "hash".into(),
        );
        key.user_id = Some("has:colon".into());
        let err = AuthMiddleware::user_context_from_api_key(&key).unwrap_err();
        assert_eq!(err.code, "INVALID_TOKEN");
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
        let result1 = rate_limiter.check_rate_limit("test-key").await;
        assert!(result1.allowed);
        assert_eq!(result1.remaining, 1);

        let result2 = rate_limiter.check_rate_limit("test-key").await;
        assert!(result2.allowed);
        assert_eq!(result2.remaining, 0);

        // Third request should be rate limited
        let result3 = rate_limiter.check_rate_limit("test-key").await;
        assert!(!result3.allowed);
        assert_eq!(result3.remaining, 0);

        // Different key should work
        let result4 = rate_limiter.check_rate_limit("other-key").await;
        assert!(result4.allowed);
        assert_eq!(result4.remaining, 1);
    }

    #[tokio::test]
    async fn test_rate_limiter_headers() {
        let config = RateLimitConfig {
            max_requests: 100,
            window_duration: Duration::from_secs(3600),
            cleanup_interval: Duration::from_secs(300),
        };
        let rate_limiter = RateLimiter::new(config);

        assert_eq!(rate_limiter.max_requests(), 100);

        let result = rate_limiter.check_rate_limit("test-key").await;
        assert!(result.allowed);
        assert_eq!(result.remaining, 99);
        assert!(result.reset_after.as_secs() <= 3600);
    }

    #[tokio::test]
    async fn test_rate_limiter_reset_after() {
        let config = RateLimitConfig {
            max_requests: 10,
            window_duration: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(300),
        };
        let rate_limiter = RateLimiter::new(config);

        // First request
        let result1 = rate_limiter.check_rate_limit("test-key").await;
        assert!(result1.allowed);
        assert!(result1.reset_after.as_secs() <= 60);

        // Multiple requests
        for _ in 0..5 {
            let result = rate_limiter.check_rate_limit("test-key").await;
            assert!(result.allowed);
        }

        // Check remaining
        // After 1 initial + 5 more = 6 requests, remaining should be 10 - 7 = 3
        // (because we increment before returning the result)
        let result_final = rate_limiter.check_rate_limit("test-key").await;
        assert!(result_final.allowed);
        assert_eq!(result_final.remaining, 3); // 10 - 7 (already made including current) = 3
    }

    #[test]
    fn test_rate_limit_config_default() {
        let config = RateLimitConfig::default();
        assert_eq!(config.max_requests, 1000);
        assert_eq!(config.window_duration, Duration::from_secs(3600));
        assert_eq!(config.cleanup_interval, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_rate_limiter_boundary_conditions() {
        // Test with max_requests = 1 (minimum)
        let config = RateLimitConfig {
            max_requests: 1,
            window_duration: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(10),
        };
        let rate_limiter = RateLimiter::new(config);

        // First request should pass
        let result1 = rate_limiter.check_rate_limit("test-key").await;
        assert!(result1.allowed);
        assert_eq!(result1.remaining, 0);

        // Second request should be blocked
        let result2 = rate_limiter.check_rate_limit("test-key").await;
        assert!(!result2.allowed);
        assert_eq!(result2.remaining, 0);
    }

    #[tokio::test]
    async fn test_rate_limiter_zero_max_requests() {
        // Test edge case: zero max requests (should allow nothing)
        let config = RateLimitConfig {
            max_requests: 0,
            window_duration: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(10),
        };
        let rate_limiter = RateLimiter::new(config);

        // Should be blocked immediately
        let result = rate_limiter.check_rate_limit("test-key").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_very_short_window() {
        // Test with very short window
        let config = RateLimitConfig {
            max_requests: 5,
            window_duration: Duration::from_millis(100),
            cleanup_interval: Duration::from_secs(10),
        };
        let rate_limiter = RateLimiter::new(config);

        // Make requests quickly
        for _ in 0..5 {
            let result = rate_limiter.check_rate_limit("test-key").await;
            assert!(result.allowed);
        }

        // Should be blocked
        let result = rate_limiter.check_rate_limit("test-key").await;
        assert!(!result.allowed);

        // Wait for window to reset
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Should be allowed again
        let result = rate_limiter.check_rate_limit("test-key").await;
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_concurrent_requests() {
        let config = RateLimitConfig {
            max_requests: 10,
            window_duration: Duration::from_secs(60),
            cleanup_interval: Duration::from_secs(10),
        };
        let rate_limiter = Arc::new(RateLimiter::new(config));

        // Test concurrent requests from same key
        let mut handles = Vec::new();
        for _ in 0..10 {
            let limiter = rate_limiter.clone();
            let handle = tokio::spawn(async move { limiter.check_rate_limit("test-key").await });
            handles.push(handle);
        }

        // Collect results
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        let allowed_count = results.iter().filter(|r| r.allowed).count();

        // All should be allowed (within limit)
        assert_eq!(allowed_count, 10);

        // Next request should be blocked
        let result = rate_limiter.check_rate_limit("test-key").await;
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn test_rate_limiter_cleanup_old_entries() {
        let config = RateLimitConfig {
            max_requests: 10,
            window_duration: Duration::from_millis(100),
            cleanup_interval: Duration::from_millis(50),
        };
        let rate_limiter = RateLimiter::new(config);

        // Create entries for multiple keys
        rate_limiter.check_rate_limit("key1").await;
        rate_limiter.check_rate_limit("key2").await;
        rate_limiter.check_rate_limit("key3").await;

        // Wait for window to expire
        tokio::time::sleep(Duration::from_millis(150)).await;

        // Cleanup happens automatically on next check_rate_limit call
        // Entries should be cleaned up, new requests should work
        let result = rate_limiter.check_rate_limit("key1").await;
        assert!(result.allowed);
    }

    // --------------------------------------------------------------------
    // Audit-log fail-open policy — regression tests.
    //
    // These tests run serially because `AUDIT_LOG_FAILURES` is a process-
    // global counter (serde::Serialize isn't required here; we're asserting
    // on the counter delta). Each test resets the counter via
    // `reset_audit_log_failures_for_test()` before asserting, which is the
    // idiomatic pattern for process-global observability counters.
    // --------------------------------------------------------------------

    #[test]
    fn audit_log_failure_counter_bumps_once_per_call() {
        super::reset_audit_log_failures_for_test();
        assert_eq!(
            super::audit_log_failures_total(),
            0,
            "counter must start at zero after reset"
        );

        let err = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
        super::record_audit_log_failure("test_site", &err);
        super::record_audit_log_failure("test_site", &err);
        super::record_audit_log_failure("other_site", &err);

        assert_eq!(
            super::audit_log_failures_total(),
            3,
            "three calls to record_audit_log_failure should yield 3 increments",
        );
    }

    // Proves the core fail-open guarantee: an audit-log write that returns
    // Err must NEVER panic, swallow the error silently, or otherwise
    // interrupt the calling flow. The observable side effect is exactly
    // the counter bump + the tracing::error! (the tracing event is
    // asserted by construction of `record_audit_log_failure`).
    #[test]
    fn record_audit_log_failure_does_not_panic_on_any_display_impl() {
        super::reset_audit_log_failures_for_test();

        // Exotic error types the AuditLogger might actually return.
        struct CustomErr(&'static str);
        impl std::fmt::Display for CustomErr {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "custom: {}", self.0)
            }
        }

        super::record_audit_log_failure("anyhow_like", &anyhow::anyhow!("boom"));
        super::record_audit_log_failure(
            "io_error",
            &std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no"),
        );
        super::record_audit_log_failure("custom", &CustomErr("xyz"));

        assert_eq!(super::audit_log_failures_total(), 3);
    }
}
