//! CORS layer construction (M4 — server hardening).

use tower_http::cors::CorsLayer;

/// Build the CORS layer from the configured allow-list.
///
/// An empty allow-list (the default) grants NO cross-origin access: the layer
/// adds no `Access-Control-Allow-Origin` header, so a browser on another origin
/// cannot read API responses. A non-empty list allows exactly those origins.
///
/// This replaces the previous `CorsLayer::permissive()`, which reflected any
/// `Origin` and let any website read API responses cross-origin (dangerous when
/// combined with an auth-disabled default deployment).
pub fn build_cors_layer(allowed_origins: &[String]) -> CorsLayer {
    if allowed_origins.is_empty() {
        return CorsLayer::new();
    }
    let origins: Vec<axum::http::HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse::<axum::http::HeaderValue>().ok())
        .collect();
    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any)
}
