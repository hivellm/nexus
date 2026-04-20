//! V2 Cluster Management REST API.
//!
//! Implements the endpoints described in the cluster-api spec:
//!
//! * `GET  /cluster/status` — snapshot of cluster metadata + health.
//! * `POST /cluster/add_node` — register a new node.
//! * `POST /cluster/remove_node` — remove a node, optionally draining.
//! * `POST /cluster/rebalance` — kick the rebalancer.
//! * `GET  /cluster/shards/{id}` — per-shard detail.
//!
//! All mutating endpoints:
//!
//! * Require the caller to hold the `Admin` permission.
//! * Return `307 Temporary Redirect` when this node is not the
//!   metadata-group leader, pointing at the current leader's
//!   `/cluster/...` URL.
//! * Return `503 Service Unavailable` when sharding is disabled at
//!   this node.

use std::sync::Arc;

use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header::LOCATION};
use axum::response::{IntoResponse, Json, Response};
use nexus_core::auth::Permission;
use nexus_core::auth::middleware::AuthContext;
use nexus_core::sharding::controller::{
    AddNodeRequest, ClusterController, ControllerError, RemoveNodeRequest,
};
use nexus_core::sharding::metadata::ShardId;

use crate::NexusServer;

/// Response body for any failed cluster-API request.
fn err_json(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": message.into() }))).into_response()
}

/// Pull the controller off the server or 503 if sharding is disabled.
async fn require_controller(server: &NexusServer) -> Result<Arc<ClusterController>, Response> {
    let guard = server.cluster_controller.read().await;
    guard.clone().ok_or_else(|| {
        err_json(
            StatusCode::SERVICE_UNAVAILABLE,
            "sharding is not enabled on this node",
        )
    })
}

/// Gate admin-only endpoints behind the `Admin` permission.
async fn require_admin(server: &NexusServer, auth: &Option<AuthContext>) -> Result<(), Response> {
    // When auth is disabled the server already refuses public binds
    // without it; the auth middleware is the one that populates
    // `auth_context`. A `None` here means the endpoint was reached on
    // a no-auth build, in which case we deny — admin operations must
    // be authenticated even on localhost.
    let ctx = auth
        .as_ref()
        .ok_or_else(|| err_json(StatusCode::UNAUTHORIZED, "authentication required"))?;
    let user_id = ctx
        .api_key
        .user_id
        .clone()
        .ok_or_else(|| err_json(StatusCode::FORBIDDEN, "API key has no associated user"))?;
    let rbac = server.rbac.read().await;
    if !rbac.user_has_permission(&user_id, &Permission::Admin) {
        return Err(err_json(StatusCode::FORBIDDEN, "Admin permission required"));
    }
    Ok(())
}

/// Translate a [`ControllerError::NotMetadataLeader`] into a `307
/// Temporary Redirect` if we have a hint, or `503` if we don't.
fn redirect_or_unavailable(
    controller: &ClusterController,
    err: ControllerError,
    endpoint: &str,
) -> Response {
    match err {
        ControllerError::NotMetadataLeader { leader_hint } => {
            let meta = controller.meta();
            let addr = leader_hint
                .as_ref()
                .and_then(|id| meta.nodes.get(id).map(|info| info.addr.to_string()));
            match addr {
                Some(a) => {
                    let mut headers = HeaderMap::new();
                    // Best-effort URL: we don't know the scheme (http
                    // vs https) from inside the handler, assume http.
                    // Clients redirected should pin scheme at the
                    // load balancer anyway.
                    let url = format!("http://{a}/cluster/{endpoint}");
                    if let Ok(v) = HeaderValue::from_str(&url) {
                        headers.insert(LOCATION, v);
                    }
                    (
                        StatusCode::TEMPORARY_REDIRECT,
                        headers,
                        Json(serde_json::json!({
                            "error": "not metadata leader",
                            "leader_hint": leader_hint.as_ref().map(|n| n.as_str()),
                            "leader_addr": a,
                        })),
                    )
                        .into_response()
                }
                None => err_json(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "no metadata leader known; retry shortly",
                ),
            }
        }
        ControllerError::Meta(e) => err_json(StatusCode::CONFLICT, e.to_string()),
        ControllerError::DrainTimeout { reason } => {
            err_json(StatusCode::CONFLICT, format!("drain pending: {reason}"))
        }
        ControllerError::UnknownShard(s) => {
            err_json(StatusCode::NOT_FOUND, format!("unknown shard {s}"))
        }
    }
}

/// `GET /cluster/status` — read-only; permitted for any authenticated
/// caller (not gated behind Admin, mirroring `/health`).
pub async fn get_status(State(server): State<Arc<NexusServer>>) -> Response {
    let controller = match require_controller(&server).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let status = controller.status();
    (StatusCode::OK, Json(status)).into_response()
}

/// `POST /cluster/add_node` — admin-only, leader-only.
pub async fn add_node(
    State(server): State<Arc<NexusServer>>,
    Extension(auth): Extension<Option<AuthContext>>,
    Json(req): Json<AddNodeRequest>,
) -> Response {
    let controller = match require_controller(&server).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_admin(&server, &auth).await {
        return resp;
    }
    match controller.add_node(req) {
        Ok(generation) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "generation": generation,
            })),
        )
            .into_response(),
        Err(e) => redirect_or_unavailable(&controller, e, "add_node"),
    }
}

/// `POST /cluster/remove_node` — admin-only, leader-only.
pub async fn remove_node(
    State(server): State<Arc<NexusServer>>,
    Extension(auth): Extension<Option<AuthContext>>,
    Json(req): Json<RemoveNodeRequest>,
) -> Response {
    let controller = match require_controller(&server).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_admin(&server, &auth).await {
        return resp;
    }
    match controller.remove_node(req) {
        Ok(generation) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "generation": generation,
            })),
        )
            .into_response(),
        Err(e) => redirect_or_unavailable(&controller, e, "remove_node"),
    }
}

/// `POST /cluster/rebalance` — admin-only, leader-only.
pub async fn rebalance(
    State(server): State<Arc<NexusServer>>,
    Extension(auth): Extension<Option<AuthContext>>,
) -> Response {
    let controller = match require_controller(&server).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    if let Err(resp) = require_admin(&server, &auth).await {
        return resp;
    }
    match controller.rebalance() {
        Ok(moves) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "moves_applied": moves,
            })),
        )
            .into_response(),
        Err(e) => redirect_or_unavailable(&controller, e, "rebalance"),
    }
}

/// `GET /cluster/shards/{id}` — per-shard detail.
pub async fn get_shard(State(server): State<Arc<NexusServer>>, Path(id): Path<u32>) -> Response {
    let controller = match require_controller(&server).await {
        Ok(c) => c,
        Err(resp) => return resp,
    };
    let status = controller.status();
    let shard_id = ShardId::new(id);
    let shard = status.shards.iter().find(|s| s.shard_id == shard_id);
    match shard {
        Some(s) => (StatusCode::OK, Json(s)).into_response(),
        None => err_json(StatusCode::NOT_FOUND, format!("unknown shard {shard_id}")),
    }
}
