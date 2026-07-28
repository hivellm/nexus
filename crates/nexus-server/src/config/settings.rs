//! Per-listener and per-feature settings structs: RESP3, native binary
//! RPC, multi-database, authentication, and root-user configuration.
//!
//! Extracted from `config.rs` as a pure mechanical move — every type and
//! its `Default` impl below is byte-identical to the original.

use serde::Deserialize;
use std::net::SocketAddr;

use super::DEFAULT_ROOT_PASSWORD;

/// Configuration for the optional RESP3 TCP listener. Disabled or enabled
/// per deployment via the `[resp3]` section of `config.yml` or the
/// corresponding `NEXUS_RESP3_*` env vars. The listener is additive: HTTP,
/// MCP, UMICP, etc. keep running regardless of this flag.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Resp3Config {
    /// Whether the RESP3 listener is spawned at all.
    pub enabled: bool,
    /// Bind address (host:port). Defaults to `127.0.0.1:15476` — loopback
    /// on purpose, so a plaintext debugging port is never exposed to the
    /// internet by default.
    pub addr: SocketAddr,
    /// Whether the listener requires `AUTH` / `HELLO AUTH` before running
    /// any non-pre-auth command. Inherits from the top-level
    /// `auth.enabled` by default so flipping authentication on/off for the
    /// whole server flips it for RESP3 too.
    pub require_auth: bool,
}

impl Default for Resp3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            addr: "127.0.0.1:15476".parse().unwrap(),
            require_auth: true,
        }
    }
}

/// Configuration for the native binary RPC listener (Phase 1 of
/// `phase1_nexus-rpc-binary-protocol`). Enabled by default — it is the
/// preferred transport for first-party SDKs. Operators who don't want the
/// extra port set `enabled = false` in the `[rpc]` section of `config.yml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RpcConfig {
    /// Whether the RPC listener is spawned at all.
    pub enabled: bool,
    /// Bind address (host:port). Default is `0.0.0.0:15475` so the SDK can
    /// reach it on LAN; keep behind a firewall or flip this to loopback
    /// for local-only deployments.
    pub addr: SocketAddr,
    /// Whether the listener requires `AUTH` before running any non-pre-auth
    /// command. Inherits from `auth.enabled` in `main.rs`.
    pub require_auth: bool,
    /// Maximum encoded body size of a single frame, in bytes. Defaults to
    /// 64 MiB — matches `nexus_protocol::rpc::DEFAULT_MAX_FRAME_BYTES`.
    pub max_frame_bytes: usize,
    /// Cap on the number of in-flight requests per connection. Excess
    /// requests wait on a per-connection semaphore.
    pub max_in_flight_per_conn: usize,
    /// Milliseconds above which a completed command logs at WARN. 2 ms is
    /// 2x the target point-read latency; tune per deployment.
    pub slow_threshold_ms: u64,
}

impl Default for RpcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            addr: "0.0.0.0:15475".parse().unwrap(),
            require_auth: true,
            max_frame_bytes: 64 * 1024 * 1024,
            max_in_flight_per_conn: 1024,
            slow_threshold_ms: 2,
        }
    }
}

/// Multi-database configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct MultiDatabaseConfig {
    /// Whether multi-database support is enabled
    pub enabled: bool,
    /// Default database name
    pub default_database: String,
    /// Directory for database storage
    pub databases_dir: String,
    /// Maximum number of databases allowed
    pub max_databases: usize,
    /// Auto-create default database on startup
    pub auto_create_default: bool,
}

impl Default for MultiDatabaseConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_database: "neo4j".to_string(),
            databases_dir: "./data/databases".to_string(),
            max_databases: 100,
            auto_create_default: true,
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Whether authentication is required for public binding (0.0.0.0)
    pub required_for_public: bool,
    /// Whether /health endpoint requires authentication
    pub require_health_auth: bool,
    /// Whether /stats requires authentication when auth is enabled (M3).
    /// Default true: with auth enabled, node/relationship/storage stats sit
    /// behind the same auth boundary as the rest of the API (closing the
    /// historical `/stats` info-leak). Set `NEXUS_REQUIRE_STATS_AUTH=false`
    /// to keep `/stats` public for operators who rely on scraping it.
    pub require_stats_auth: bool,
}

/// Root user configuration
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RootUserConfig {
    /// Root username
    pub username: String,
    /// Root password (plaintext, will be hashed)
    pub password: String,
    /// Whether root user is enabled
    pub enabled: bool,
    /// Whether to disable root after first admin user is created
    pub disable_after_setup: bool,
}

impl Default for RootUserConfig {
    fn default() -> Self {
        Self {
            username: "root".to_string(),
            password: DEFAULT_ROOT_PASSWORD.to_string(),
            enabled: true,
            disable_after_setup: false,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false, // Disabled by default for development
            required_for_public: true,
            require_health_auth: false,
            require_stats_auth: true,
        }
    }
}
