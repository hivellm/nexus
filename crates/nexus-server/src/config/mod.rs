//! Server configuration.
//!
//! Split from a single `config.rs` into a directory module: this file is
//! the facade — the `Config` struct and its `Default` impl stay here,
//! `Config`'s other inherent methods (`from_env`, `security_preflight`,
//! ...) are implemented across the submodules below via additional `impl
//! Config { .. }` blocks (legal in Rust: multiple `impl` blocks for one
//! type may live in different files of the same crate). Every path that
//! used to resolve through `nexus_server::config::*` before the split
//! still resolves identically after it via the `pub use` re-exports
//! below.

use crate::middleware::RateLimitConfig;
use std::net::SocketAddr;

mod encryption;
mod loader;
mod settings;
mod yaml;

#[cfg(test)]
mod tests;

pub use encryption::{
    EncryptionConfig, EncryptionInventorySummary, EncryptionSource, enforce_data_dir_invariants,
    fingerprint_master_key, resolve_encryption_config,
};
pub use settings::{AuthConfig, MultiDatabaseConfig, Resp3Config, RootUserConfig, RpcConfig};
pub use yaml::YamlOverrides;

/// Literal default root password shipped by [`RootUserConfig::default`].
/// Lives in one place so the boot-time preflight ([`Config::security_preflight`])
/// can compare against it without duplicating the string.
const DEFAULT_ROOT_PASSWORD: &str = "root";

/// Server configuration
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Config {
    /// Server bind address
    pub addr: SocketAddr,
    /// Data directory
    pub data_dir: String,
    /// Maximum request body size in bytes. Enforced via Axum's
    /// `DefaultBodyLimit` layer to keep bulk ingest payloads from
    /// monopolising memory.
    pub max_body_size_bytes: usize,
    /// Maximum wall-clock duration allowed for a single HTTP request,
    /// enforced via `tower_http::timeout::TimeoutLayer`. Covers the
    /// HTTP/slow-connection vector (a client that never finishes sending
    /// or reading); it does not cancel CPU-bound work already in flight
    /// inside the Cypher executor (see H4 follow-up in the server-hardening
    /// report).
    pub request_timeout_secs: u64,
    /// Per-IP rate limiting configuration for the global token-bucket
    /// limiter that fronts every HTTP route. `NEXUS_RATE_LIMIT_*` env
    /// vars override the defaults (enabled, 100 req/60s + 20 burst,
    /// loopback exempt) — see [`RateLimitConfig`] for the full knob
    /// list.
    pub rate_limit: RateLimitConfig,
    /// CORS allow-list (M4). Empty (the default) grants no cross-origin
    /// access — a browser on another origin cannot read API responses.
    /// Populate via `NEXUS_CORS_ALLOWED_ORIGINS` (comma-separated origins)
    /// for deployments that intentionally serve cross-origin clients.
    pub cors_allowed_origins: Vec<String>,
    /// Additional `Host` header values accepted by the `/mcp` endpoint's
    /// DNS-rebinding guard (rmcp's `StreamableHttpServerConfig::
    /// allowed_hosts`), beyond rmcp's own built-in default of
    /// `localhost`, `127.0.0.1`, `::1`. Empty (the default) keeps that
    /// built-in list untouched — deliberately NOT pre-populated with the
    /// loopback values here, because passing an empty-but-explicit list
    /// to `with_allowed_hosts` would disable every host instead of
    /// falling through to rmcp's secure default. Populate via
    /// `NEXUS_MCP_ALLOWED_HOSTS` (comma-separated hostnames or
    /// `host:port` authorities) for deployments reached by a hostname or
    /// IP other than localhost. See `create_mcp_router` in `main.rs` and
    /// `docs/specs/api-protocols.md` § MCP Integration.
    pub mcp_allowed_hosts: Vec<String>,
    /// Escape hatch that disables the `/mcp` Host-header allow-list
    /// entirely (`StreamableHttpServerConfig::disable_allowed_hosts()`),
    /// removing DNS-rebinding protection so any `Host` header is
    /// accepted. `false` by default. Set
    /// `NEXUS_MCP_ALLOWED_HOSTS_DISABLE=true` to opt in — NOT recommended
    /// for public deployments; prefer `mcp_allowed_hosts` /
    /// `NEXUS_MCP_ALLOWED_HOSTS` instead.
    pub mcp_allowed_hosts_disable: bool,
    /// Engine-side tunables (page cache, etc.) propagated from YAML.
    pub engine: nexus_core::EngineConfig,
    /// Root user configuration
    pub root_user: RootUserConfig,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Multi-database configuration
    pub multi_database: MultiDatabaseConfig,
    /// RESP3 listener configuration (additive to the HTTP port).
    pub resp3: Resp3Config,
    /// Native binary RPC listener configuration (additive to the HTTP port).
    pub rpc: RpcConfig,
    /// Cluster-mode configuration. Disabled by default; when enabled,
    /// every endpoint requires authentication and each authenticated
    /// request is scoped to the tenant namespace derived from its API
    /// key's `user_id`. See `nexus_core::cluster::ClusterConfig`.
    pub cluster: nexus_core::cluster::ClusterConfig,
    /// Encryption-at-rest configuration. The full stack is gated
    /// behind `enabled = true` AND a valid [`KeyProvider`] resolved
    /// at boot; storage-layer wiring lands in
    /// `phase8_encryption-at-rest-storage-hooks` and friends. The
    /// CLI surface ships now so operators can validate their key
    /// configuration before the storage hooks land.
    ///
    /// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
    pub encryption: EncryptionConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:15474".parse().unwrap(),
            data_dir: "./data".to_string(),
            // 16 MiB — generous for single Cypher statements and small bulk
            // ingest payloads, but bounded so a single oversized POST cannot
            // exhaust the server's allocator.
            max_body_size_bytes: 16 * 1024 * 1024,
            request_timeout_secs: 30,
            rate_limit: RateLimitConfig::default(),
            cors_allowed_origins: Vec::new(),
            mcp_allowed_hosts: Vec::new(),
            mcp_allowed_hosts_disable: false,
            engine: nexus_core::EngineConfig::default(),
            root_user: RootUserConfig::default(),
            auth: AuthConfig::default(),
            multi_database: MultiDatabaseConfig::default(),
            resp3: Resp3Config::default(),
            rpc: RpcConfig::default(),
            cluster: nexus_core::cluster::ClusterConfig::default(),
            encryption: EncryptionConfig::default(),
        }
    }
}
