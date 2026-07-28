//! MCP StreamableHTTP router construction.
//!
//! [`create_mcp_router`] was extracted from `main.rs` as a pure mechanical
//! move — no behavior or signature change. `pub(super)` mirrors the
//! original reachability: a crate-root-private item, visible to `main.rs`
//! and to `main.rs`'s nested `#[cfg(test)] mod tests`, both of which call
//! it.

use std::sync::Arc;

use axum::middleware as axum_middleware;
use axum::routing::any;
use axum::{Router, extract::Request, middleware::Next};

use nexus_server::NexusServer;
use nexus_server::middleware::{create_auth_middleware, mcp_auth_middleware_handler};

/// Create MCP router with StreamableHTTP transport.
///
/// `mcp_allowed_hosts` / `mcp_allowed_hosts_disable` configure rmcp's
/// DNS-rebinding `Host` allow-list (see `config::Config::mcp_allowed_hosts`
/// for the env var / default semantics): `disable` wins outright
/// (`disable_allowed_hosts()`); otherwise a non-empty list replaces rmcp's
/// built-in default via `with_allowed_hosts`; an empty list keeps
/// `StreamableHttpServerConfig::default()` exactly as before this option
/// existed.
pub(super) async fn create_mcp_router(
    nexus_server: Arc<NexusServer>,
    cluster_enabled: bool,
    mcp_allowed_hosts: Vec<String>,
    mcp_allowed_hosts_disable: bool,
) -> anyhow::Result<Router<Arc<NexusServer>>> {
    use hyper::service::Service;
    use hyper_util::service::TowerToHyperService;
    use rmcp::transport::streamable_http_server::StreamableHttpServerConfig;
    use rmcp::transport::streamable_http_server::StreamableHttpService;
    use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;

    // Create MCP service handler
    let server = nexus_server.clone();

    let streamable_config = if mcp_allowed_hosts_disable {
        StreamableHttpServerConfig::default().disable_allowed_hosts()
    } else if !mcp_allowed_hosts.is_empty() {
        StreamableHttpServerConfig::default().with_allowed_hosts(mcp_allowed_hosts)
    } else {
        StreamableHttpServerConfig::default()
    };

    // Create StreamableHTTP service
    let streamable_service = StreamableHttpService::new(
        move || Ok(crate::api::streaming::NexusMcpService::new(server.clone())),
        LocalSessionManager::default().into(),
        streamable_config,
    );

    // Convert to axum service and create router
    let hyper_service = TowerToHyperService::new(streamable_service);

    // Create router with the MCP endpoint
    let mut router = Router::new()
        .route(
            "/",
            any(move |req: Request| {
                let service = hyper_service.clone();
                async move {
                    // Forward request to hyper service
                    match service.call(req).await {
                        Ok(response) => Ok(response),
                        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
                    }
                }
            }),
        )
        .with_state(nexus_server.clone());

    // Apply MCP authentication middleware if authentication is enabled
    if nexus_server.auth_manager.config().enabled || cluster_enabled {
        let auth_middleware = create_auth_middleware(
            nexus_server.clone(),
            true, // Require authentication for MCP
            true, // MCP has no /stats route; keep the gated default
            cluster_enabled,
        );

        router = router.layer(axum_middleware::from_fn_with_state(
            auth_middleware,
            |state: axum::extract::State<nexus_core::auth::middleware::AuthMiddleware>,
             request: Request,
             next: Next| async move {
                mcp_auth_middleware_handler(state, request, next).await
            },
        ));
    }

    Ok(router)
}
