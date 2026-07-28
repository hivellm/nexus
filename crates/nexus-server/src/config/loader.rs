//! `Config::from_env` (the full env-var / YAML / auth-file resolution
//! pipeline) plus the small getter/builder/preflight methods.
//!
//! Extracted from `config.rs` as a pure mechanical move — every method
//! below is byte-identical to the original.

use std::net::SocketAddr;
use std::time::Duration;

use crate::middleware::RateLimitConfig;

use super::{
    AuthConfig, Config, DEFAULT_ROOT_PASSWORD, MultiDatabaseConfig, Resp3Config, RootUserConfig,
    RpcConfig, resolve_encryption_config,
};

impl Config {
    /// Load configuration from environment variables and config file
    /// Priority: Environment variables > YAML file > auth.toml > defaults
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        // YAML first so env vars always win.
        let yaml_path =
            std::env::var("NEXUS_CONFIG_PATH").unwrap_or_else(|_| "config.yml".to_string());
        let yaml = Self::from_yaml_file(&yaml_path).unwrap_or_default();

        let addr: SocketAddr = std::env::var("NEXUS_ADDR")
            .ok()
            .or(yaml.addr)
            .unwrap_or_else(|| "127.0.0.1:15474".to_string())
            .parse()
            .expect("Invalid NEXUS_ADDR");

        let data_dir = std::env::var("NEXUS_DATA_DIR")
            .ok()
            .or(yaml.data_dir)
            .unwrap_or_else(|| "./data".to_string());

        // Max body size: NEXUS_MAX_BODY_SIZE_MB > yaml.server.max_body_size_mb > 16 MiB.
        let max_body_size_bytes = std::env::var("NEXUS_MAX_BODY_SIZE_MB")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .or(yaml.max_body_size_mb)
            .map(|mb| mb * 1024 * 1024)
            .unwrap_or(16 * 1024 * 1024);

        // Engine config. Start from defaults and let YAML override.
        let mut engine = nexus_core::EngineConfig::default();
        if let Some(cap) = yaml.page_cache_capacity {
            engine.page_cache_capacity = cap;
        }

        // Try to load from config file first (will be overridden by env vars)
        let (mut root_user, mut auth) = Self::from_auth_file("config")
            .unwrap_or_else(|| (RootUserConfig::default(), AuthConfig::default()));

        // Load root user configuration from environment (overrides config file)
        if let Ok(root_username) = std::env::var("NEXUS_ROOT_USERNAME") {
            root_user.username = root_username;
        }

        // Support Docker secrets: try NEXUS_ROOT_PASSWORD_FILE first, then NEXUS_ROOT_PASSWORD
        if let Ok(password_file) = std::env::var("NEXUS_ROOT_PASSWORD_FILE") {
            root_user.password = std::fs::read_to_string(&password_file)
                .unwrap_or_else(|_| root_user.password.clone())
                .trim()
                .to_string();
        } else if let Ok(password) = std::env::var("NEXUS_ROOT_PASSWORD") {
            root_user.password = password;
        }

        if let Ok(root_enabled) = std::env::var("NEXUS_ROOT_ENABLED") {
            root_user.enabled = root_enabled.parse::<bool>().unwrap_or(root_user.enabled);
        }

        if let Ok(disable_after_setup) = std::env::var("NEXUS_DISABLE_ROOT_AFTER_SETUP") {
            root_user.disable_after_setup = disable_after_setup
                .parse::<bool>()
                .unwrap_or(root_user.disable_after_setup);
        }

        // Load auth configuration from environment (overrides config file)
        if let Ok(auth_enabled) = std::env::var("NEXUS_AUTH_ENABLED") {
            auth.enabled = auth_enabled.parse::<bool>().unwrap_or(auth.enabled);
        }

        if let Ok(require_health_auth) = std::env::var("NEXUS_REQUIRE_HEALTH_AUTH") {
            auth.require_health_auth = require_health_auth
                .parse::<bool>()
                .unwrap_or(auth.require_health_auth);
        }

        if let Ok(v) = std::env::var("NEXUS_AUTH_REQUIRED_FOR_PUBLIC") {
            auth.required_for_public = v.parse::<bool>().unwrap_or(auth.required_for_public);
        }

        if let Ok(v) = std::env::var("NEXUS_REQUIRE_STATS_AUTH") {
            auth.require_stats_auth = v.parse::<bool>().unwrap_or(auth.require_stats_auth);
        }

        // RESP3: disabled by default; `NEXUS_RESP3_ENABLED=true` opts in,
        // `NEXUS_RESP3_ADDR` overrides the bind address, and auth requirement
        // mirrors the top-level auth flag unless overridden.
        let resp3_enabled = std::env::var("NEXUS_RESP3_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);
        let resp3_addr: SocketAddr = std::env::var("NEXUS_RESP3_ADDR")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or_else(|| "127.0.0.1:15476".parse().unwrap());
        let resp3_require_auth = std::env::var("NEXUS_RESP3_REQUIRE_AUTH")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(auth.enabled);

        // RPC: enabled by default (the preferred SDK transport). Env vars
        // follow the same shape as `NEXUS_RESP3_*` for operator parity.
        let rpc_defaults = RpcConfig::default();
        let rpc_enabled = std::env::var("NEXUS_RPC_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(rpc_defaults.enabled);
        let rpc_addr: SocketAddr = std::env::var("NEXUS_RPC_ADDR")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(rpc_defaults.addr);
        let rpc_require_auth = std::env::var("NEXUS_RPC_REQUIRE_AUTH")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(auth.enabled);
        let rpc_max_frame_bytes = std::env::var("NEXUS_RPC_MAX_FRAME_BYTES")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(rpc_defaults.max_frame_bytes);
        let rpc_max_in_flight = std::env::var("NEXUS_RPC_MAX_IN_FLIGHT")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(rpc_defaults.max_in_flight_per_conn);
        let rpc_slow_threshold_ms = std::env::var("NEXUS_RPC_SLOW_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(rpc_defaults.slow_threshold_ms);

        // H4: per-request wall-clock timeout (default 30s).
        let request_timeout_secs = std::env::var("NEXUS_REQUEST_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(30);

        // Per-IP rate limiting (H2, hardened to fix the /ingest
        // connection-reset bug): enabled by default with the original
        // 100 req/60s + 20 burst budget, but loopback clients are now
        // exempt by default so local bulk loads (e.g. LDBC ingest)
        // never trip it. `NEXUS_RATE_LIMIT_*` overrides every knob;
        // an absent or unparseable var falls back to `RateLimitConfig::
        // default()`.
        let rate_limit_defaults = RateLimitConfig::default();
        let rate_limit_enabled = std::env::var("NEXUS_RATE_LIMIT_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(rate_limit_defaults.enabled);
        let rate_limit_max_requests = std::env::var("NEXUS_RATE_LIMIT_MAX_REQUESTS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(rate_limit_defaults.max_requests);
        let rate_limit_window = std::env::var("NEXUS_RATE_LIMIT_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(rate_limit_defaults.window_duration);
        let rate_limit_burst = std::env::var("NEXUS_RATE_LIMIT_BURST")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(rate_limit_defaults.burst_capacity);
        let rate_limit_exempt_loopback = std::env::var("NEXUS_RATE_LIMIT_EXEMPT_LOOPBACK")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(rate_limit_defaults.exempt_loopback);
        let rate_limit = RateLimitConfig {
            enabled: rate_limit_enabled,
            max_requests: rate_limit_max_requests,
            window_duration: rate_limit_window,
            burst_capacity: rate_limit_burst,
            exempt_loopback: rate_limit_exempt_loopback,
        };

        // M4: CORS allow-list, comma-separated origins. Empty (default) means
        // no cross-origin access is granted.
        let cors_allowed_origins = std::env::var("NEXUS_CORS_ALLOWED_ORIGINS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();

        // MCP `/mcp` DNS-rebinding Host allow-list. Empty (default) keeps
        // rmcp's own secure default (localhost/127.0.0.1/::1) untouched;
        // NEXUS_MCP_ALLOWED_HOSTS (comma-separated) extends it for network
        // deployments, and NEXUS_MCP_ALLOWED_HOSTS_DISABLE removes the
        // check entirely (not recommended for public deployments).
        let mcp_allowed_hosts = std::env::var("NEXUS_MCP_ALLOWED_HOSTS")
            .ok()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<String>>()
            })
            .unwrap_or_default();
        let mcp_allowed_hosts_disable = std::env::var("NEXUS_MCP_ALLOWED_HOSTS_DISABLE")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        Self {
            addr,
            data_dir,
            max_body_size_bytes,
            request_timeout_secs,
            rate_limit,
            cors_allowed_origins,
            mcp_allowed_hosts,
            mcp_allowed_hosts_disable,
            engine,
            root_user,
            auth,
            multi_database: MultiDatabaseConfig::default(),
            resp3: Resp3Config {
                enabled: resp3_enabled,
                addr: resp3_addr,
                require_auth: resp3_require_auth,
            },
            rpc: RpcConfig {
                enabled: rpc_enabled,
                addr: rpc_addr,
                require_auth: rpc_require_auth,
                max_frame_bytes: rpc_max_frame_bytes,
                max_in_flight_per_conn: rpc_max_in_flight,
                slow_threshold_ms: rpc_slow_threshold_ms,
            },
            // Cluster mode is env-var-opt-in to keep existing
            // deployments untouched. `NEXUS_CLUSTER_ENABLED=true`
            // flips the master switch; everything else inherits
            // `ClusterConfig::default()` (sensible tenant quotas).
            cluster: if std::env::var("NEXUS_CLUSTER_ENABLED")
                .ok()
                .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
                .unwrap_or(false)
            {
                nexus_core::cluster::ClusterConfig::enabled_with_defaults()
            } else {
                nexus_core::cluster::ClusterConfig::default()
            },
            // Encryption-at-rest. Resolved separately so a bad key
            // surfaces as a hard fail at boot (`expect`) rather than
            // silently disabling encryption — an operator who set
            // NEXUS_ENCRYPT_AT_REST=true and got a typo'd key file
            // path must NOT see the server start in plaintext mode.
            encryption: resolve_encryption_config().expect(
                "ERR_ENCRYPTION_BOOT: failed to resolve master key — \
                 set NEXUS_ENCRYPT_AT_REST=false to start in plaintext, \
                 or fix NEXUS_DATA_KEY / NEXUS_KEY_FILE",
            ),
        }
    }

    /// Get MCP API key from environment variable
    pub fn mcp_api_key() -> Option<String> {
        std::env::var("NEXUS_MCP_API_KEY").ok()
    }

    /// Get the bind address
    #[allow(dead_code)]
    pub fn addr(&self) -> &SocketAddr {
        &self.addr
    }

    /// Get the data directory
    #[allow(dead_code)]
    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// Set a new data directory
    #[allow(dead_code)]
    pub fn with_data_dir(mut self, data_dir: impl Into<String>) -> Self {
        self.data_dir = data_dir.into();
        self
    }

    /// Set a new bind address
    #[allow(dead_code)]
    pub fn with_addr(mut self, addr: SocketAddr) -> Self {
        self.addr = addr;
        self
    }

    /// Boot-time security preflight. Returns `Err(operator-facing message)` if
    /// the current configuration is unsafe to serve. Called at startup before
    /// binding; a hard failure is intentional (these are unsafe defaults).
    pub fn security_preflight(&self) -> Result<(), String> {
        // H1: refuse a non-loopback bind with auth disabled, unless the
        // operator deliberately opts out via required_for_public = false.
        if !self.auth.enabled && self.auth.required_for_public && !self.addr.ip().is_loopback() {
            return Err(format!(
                "refusing to start: bind address {} is not loopback but authentication is \
                 disabled. Enable auth (NEXUS_AUTH_ENABLED=true), or to serve an open instance \
                 deliberately set NEXUS_AUTH_REQUIRED_FOR_PUBLIC=false.",
                self.addr
            ));
        }
        // M2: refuse the literal default root password when auth is enabled and
        // the root account is active.
        if self.auth.enabled
            && self.root_user.enabled
            && self.root_user.password == DEFAULT_ROOT_PASSWORD
        {
            return Err(
                "refusing to start: authentication is enabled but the root password is still the \
                 default. Set NEXUS_ROOT_PASSWORD (or NEXUS_ROOT_PASSWORD_FILE) to a strong secret."
                    .to_string(),
            );
        }
        Ok(())
    }
}
