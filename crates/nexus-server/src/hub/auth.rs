//! Hub access-key authentication middleware
//! (phase5_hub-integration §2).
//!
//! ## Trust model
//!
//! The Hub control plane is the authoritative source for user
//! access keys. Today the SDK exposes management endpoints
//! (`generate` / `list` / `get(id)` / `revoke`) but does not yet
//! expose a "validate this raw token → user_id" call. While that
//! gap is open, Nexus follows the Hub's documented gateway pattern:
//!
//! - The Hub fronts user traffic and verifies the
//!   `Authorization: Bearer <access-key>` header.
//! - On success the Hub forwards the request to Nexus with two
//!   trusted headers:
//!     * `X-Hivehub-User-Id: <uuid>` — the user the key resolves to.
//!     * `X-Hivehub-Access-Key-Id: <uuid>` — the access-key id (so
//!       audit logs can attribute the call to a specific key).
//! - Nexus extracts those headers into a [`UserContext`] and lets
//!   the rest of the request pipeline scope its work to the user.
//!
//! When `HubClient` is `None` (single-tenant standalone mode), the
//! middleware is a no-op — no authentication header is required and
//! no `UserContext` is attached. Existing standalone deployments
//! keep working unchanged.
//!
//! When the Hub SDK gains a `validate_access_key` call, the
//! [`extract_user_context`] helper swaps from header-trust to a real
//! SDK round-trip without touching the rest of the wire-up.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use super::HubClient;

/// Per-request authenticated user context. Inserted into the Axum
/// request extensions by [`hub_auth_middleware`]; handlers extract
/// it via `Extension<UserContext>`.
#[derive(Debug, Clone, Serialize)]
pub struct UserContext {
    /// The Hub user id this request was authenticated as.
    pub user_id: Uuid,
    /// The Hub access-key id the user presented (when available).
    /// Populated from the `X-Hivehub-Access-Key-Id` header that the
    /// Hub gateway sets after it verifies the bearer token. `None`
    /// when the gateway omitted the header (e.g. service-to-service
    /// calls that authenticate with a different mechanism).
    pub access_key_id: Option<Uuid>,
}

/// Header constants shared with the Hub gateway.
pub mod headers {
    pub const X_USER_ID: &str = "X-Hivehub-User-Id";
    pub const X_ACCESS_KEY_ID: &str = "X-Hivehub-Access-Key-Id";
}

/// Standard JSON error body returned by the middleware.
#[derive(Debug, Serialize)]
struct AuthError {
    error: &'static str,
    detail: String,
}

fn unauthorized(detail: impl Into<String>) -> Response {
    let body = AuthError {
        error: "unauthorized",
        detail: detail.into(),
    };
    (StatusCode::UNAUTHORIZED, axum::Json(body)).into_response()
}

/// Pull the user context out of the trusted gateway headers. Returns
/// `Ok(Some(ctx))` on success, `Ok(None)` when no auth headers were
/// present (caller decides if that's an error), or `Err(reason)` on
/// malformed values.
pub fn extract_user_context(headers: &HeaderMap) -> Result<Option<UserContext>, &'static str> {
    let Some(raw_user) = headers.get(headers::X_USER_ID) else {
        return Ok(None);
    };
    let user_str = raw_user
        .to_str()
        .map_err(|_| "X-Hivehub-User-Id contains non-ASCII bytes")?;
    let user_id =
        Uuid::parse_str(user_str.trim()).map_err(|_| "X-Hivehub-User-Id is not a UUID")?;

    let access_key_id = match headers.get(headers::X_ACCESS_KEY_ID) {
        None => None,
        Some(v) => {
            let s = v
                .to_str()
                .map_err(|_| "X-Hivehub-Access-Key-Id contains non-ASCII bytes")?;
            Some(Uuid::parse_str(s.trim()).map_err(|_| "X-Hivehub-Access-Key-Id is not a UUID")?)
        }
    };

    Ok(Some(UserContext {
        user_id,
        access_key_id,
    }))
}

/// Middleware factory parameterised by the optional Hub client.
///
/// - `Some(client)` — Hub mode is active. The middleware requires
///   `X-Hivehub-User-Id` and rejects requests that lack a bearer
///   token + the trusted gateway header.
/// - `None` — standalone mode. The middleware is a no-op so existing
///   single-tenant deployments keep their behaviour.
pub async fn hub_auth_middleware(
    State(hub): State<Arc<Option<HubClient>>>,
    mut request: Request,
    next: Next,
) -> Response {
    if hub.is_none() {
        return next.run(request).await;
    }

    // The bearer token itself isn't validated by Nexus today (the
    // gateway already verified it). We still require its presence so
    // requests bypassing the gateway are rejected.
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if !auth_header.starts_with("Bearer ") {
        return unauthorized("Missing or malformed Authorization: Bearer <token>");
    }

    match extract_user_context(request.headers()) {
        Ok(Some(ctx)) => {
            request.extensions_mut().insert(ctx);
            next.run(request).await
        }
        Ok(None) => {
            unauthorized("Hub mode is enabled but X-Hivehub-User-Id was not set by the gateway")
        }
        Err(detail) => unauthorized(detail),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn header_map(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (k, v) in pairs {
            let name: axum::http::HeaderName = k.parse().expect("valid header name");
            map.insert(name, HeaderValue::from_str(v).expect("valid header value"));
        }
        map
    }

    #[test]
    fn extract_user_context_returns_none_when_header_missing() {
        let headers = HeaderMap::new();
        let ctx = extract_user_context(&headers).expect("no error when header is absent");
        assert!(ctx.is_none());
    }

    #[test]
    fn extract_user_context_pulls_user_id_only() {
        let user = Uuid::new_v4();
        let headers = header_map(&[(headers::X_USER_ID, &user.to_string())]);
        let ctx = extract_user_context(&headers).unwrap().unwrap();
        assert_eq!(ctx.user_id, user);
        assert_eq!(ctx.access_key_id, None);
    }

    #[test]
    fn extract_user_context_pulls_user_and_access_key() {
        let user = Uuid::new_v4();
        let key = Uuid::new_v4();
        let headers = header_map(&[
            (headers::X_USER_ID, &user.to_string()),
            (headers::X_ACCESS_KEY_ID, &key.to_string()),
        ]);
        let ctx = extract_user_context(&headers).unwrap().unwrap();
        assert_eq!(ctx.user_id, user);
        assert_eq!(ctx.access_key_id, Some(key));
    }

    #[test]
    fn extract_user_context_rejects_non_uuid_user_id() {
        let headers = header_map(&[(headers::X_USER_ID, "not-a-uuid")]);
        let err = extract_user_context(&headers).unwrap_err();
        assert!(err.contains("not a UUID"));
    }

    #[test]
    fn extract_user_context_rejects_non_uuid_access_key() {
        let user = Uuid::new_v4();
        let headers = header_map(&[
            (headers::X_USER_ID, &user.to_string()),
            (headers::X_ACCESS_KEY_ID, "garbage"),
        ]);
        let err = extract_user_context(&headers).unwrap_err();
        assert!(err.contains("Access-Key-Id"));
    }
}
