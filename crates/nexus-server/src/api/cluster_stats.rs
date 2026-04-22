//! Per-tenant quota / usage stats endpoint (cluster mode).
//!
//! Cluster-mode deployments need an easy way for tenants to check
//! their own usage against the limits the server is enforcing. The
//! `GET /cluster/stats/self` endpoint:
//!
//! 1. Requires authentication (as all cluster-mode endpoints do).
//!    The auth middleware will already have installed a
//!    `UserContext` in request extensions — the handler just reads it.
//! 2. Pulls the snapshot for the authenticated tenant from the
//!    quota provider via `NexusServer::tenant_quota_snapshot`.
//! 3. Returns a JSON body with storage / rate-limit usage +
//!    limits. No cross-tenant admin view here — that's Phase 4
//!    §14.4's "local stats only" constraint made explicit.
//!
//! Standalone-mode servers (no quota provider installed) return
//! `404 Not Found` with a stable error code so SDK clients can
//! tell "feature off" apart from "tenant unknown".

use axum::{
    Json,
    extract::State,
    http::{Request, StatusCode},
};
use nexus_core::auth::extract_user_context;
use nexus_core::cluster::QuotaSnapshot;
use serde::Serialize;
use std::sync::Arc;

use crate::NexusServer;

/// JSON body returned by `GET /cluster/stats/self`.
///
/// Flat shape — mirrors `QuotaSnapshot` but adds the tenant id so
/// clients that cache responses can key on it without re-reading
/// their own API key. The id is the raw namespace id (what the
/// API key carried as `user_id`), not the scoped catalog prefix.
#[derive(Debug, Serialize)]
pub struct TenantStatsResponse {
    pub tenant_id: String,
    pub storage_bytes_used: u64,
    pub storage_bytes_limit: u64,
    pub requests_this_minute: u32,
    pub requests_per_minute_limit: u32,
    pub requests_this_hour: u32,
    pub requests_per_hour_limit: u32,
}

/// Structured error response. `code` is the stable wire contract
/// (see `ClusterStatsErrorCode` constants); `message` is
/// informational.
#[derive(Debug, Serialize)]
pub struct ClusterStatsError {
    pub code: &'static str,
    pub message: &'static str,
}

pub const CODE_NOT_CLUSTER_MODE: &str = "CLUSTER_MODE_DISABLED";
pub const CODE_NO_TENANT_CONTEXT: &str = "NO_TENANT_CONTEXT";
pub const CODE_TENANT_UNKNOWN: &str = "TENANT_UNKNOWN";

impl From<QuotaSnapshot> for TenantStatsResponse {
    fn from(snap: QuotaSnapshot) -> Self {
        // tenant_id is filled in by the handler that has the
        // UserContext; From-impl here only covers the numeric side.
        Self {
            tenant_id: String::new(),
            storage_bytes_used: snap.storage_bytes_used,
            storage_bytes_limit: snap.storage_bytes_limit,
            requests_this_minute: snap.requests_this_minute,
            requests_per_minute_limit: snap.requests_per_minute_limit,
            requests_this_hour: snap.requests_this_hour,
            requests_per_hour_limit: snap.requests_per_hour_limit,
        }
    }
}

/// Handler for `GET /cluster/stats/self`.
///
/// Returns a snapshot of the authenticated tenant's current quota
/// usage. Stripe of 404s with stable codes so a smart client can
/// differentiate the three failure modes:
///
/// - Server is not in cluster mode (provider not installed).
/// - Request did not carry a tenant context (standalone-style
///   request under a cluster-mode server — shouldn't happen
///   because the auth middleware rejects earlier, but guard
///   anyway).
/// - The tenant is known to the system but has never touched the
///   server, so the provider has no snapshot yet.
pub async fn tenant_stats(
    State(server): State<Arc<NexusServer>>,
    request: Request<axum::body::Body>,
) -> Result<Json<TenantStatsResponse>, (StatusCode, Json<ClusterStatsError>)> {
    let ctx = extract_user_context(&request).ok_or((
        StatusCode::UNAUTHORIZED,
        Json(ClusterStatsError {
            code: CODE_NO_TENANT_CONTEXT,
            message: "request has no tenant context; cluster mode may be off or auth failed",
        }),
    ))?;

    // Check provider presence separately from tenant presence so
    // we can return different error codes.
    if server.quota_provider.read().await.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ClusterStatsError {
                code: CODE_NOT_CLUSTER_MODE,
                message: "server is running in standalone mode; quota stats are unavailable",
            }),
        ));
    }

    let snap = server.tenant_quota_snapshot(ctx.namespace()).await.ok_or((
        StatusCode::NOT_FOUND,
        Json(ClusterStatsError {
            code: CODE_TENANT_UNKNOWN,
            message: "no usage recorded for this tenant yet",
        }),
    ))?;

    let mut body: TenantStatsResponse = snap.into();
    body.tenant_id = ctx.namespace().as_id().to_string();
    Ok(Json(body))
}
