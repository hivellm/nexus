//! Server configuration

use serde::Deserialize;
use std::net::SocketAddr;
use std::path::Path;

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

/// Encryption-at-rest configuration. Resolved from
/// `NEXUS_ENCRYPT_AT_REST` + `NEXUS_DATA_KEY` / `NEXUS_KEY_FILE` at
/// server boot.
#[derive(Debug, Clone, Default)]
pub struct EncryptionConfig {
    /// Master switch. `false` keeps the storage layer in plaintext
    /// (the pre-phase-8 behaviour); `true` requires a valid
    /// [`KeyProvider`] source to be configured. Storage hooks gate
    /// on this flag once they land.
    ///
    /// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
    pub enabled: bool,
    /// Where the master key is sourced from. `None` when
    /// `enabled = false` or when boot resolution failed and the
    /// server intentionally started without a key (we never silently
    /// fall through — a hard fail is the default).
    pub source: Option<EncryptionSource>,
    /// SHA-256 fingerprint of the resolved master key (32 bytes,
    /// hex-encoded). Safe to log; safe to ship over the
    /// `/admin/encryption/status` endpoint. Lets operators verify
    /// two servers are using the same key without exposing the key
    /// itself.
    pub fingerprint: Option<String>,
}

/// Tag identifying which [`KeyProvider`] backed the resolved master
/// key. Wider than `KeyProviderError` because the operator might
/// want to know *which* env var or *which* path was used.
///
/// [`KeyProvider`]: nexus_core::storage::crypto::KeyProvider
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EncryptionSource {
    /// Master key sourced from an environment variable.
    Env { name: String },
    /// Master key sourced from a file on disk.
    File { path: String },
}

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
            password: "root".to_string(),
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
        }
    }
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

/// Resolve the master key from the configured source and compute a
/// SHA-256 fingerprint. The fingerprint is safe to log — it's a
/// hash of the key, not the key itself; two servers booting with
/// the same master will report the same fingerprint, two servers
/// with different masters will report different ones.
///
/// Returns `Ok(None)` when `enabled = false` (no key resolution
/// attempted) and `Err` when resolution itself failed (bad path,
/// wrong format, missing env var). The server's boot path treats
/// the latter as fatal — silently falling through would let an
/// operator who *thought* they had encryption running ship to
/// production without it.
pub fn resolve_encryption_config() -> anyhow::Result<EncryptionConfig> {
    let enabled = std::env::var("NEXUS_ENCRYPT_AT_REST")
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE" | "yes"))
        .unwrap_or(false);
    if !enabled {
        return Ok(EncryptionConfig::default());
    }

    use nexus_core::storage::crypto::{
        EnvKeyProvider, FileKeyProvider, KeyProvider, MASTER_KEY_LEN,
    };

    let (source, key) = if let Ok(path) = std::env::var("NEXUS_KEY_FILE") {
        let p = FileKeyProvider::from_path(&path)
            .map_err(|e| anyhow::anyhow!("failed to load NEXUS_KEY_FILE={path}: {e}"))?;
        let k = p
            .master_key()
            .map_err(|e| anyhow::anyhow!("failed to read master key from {path}: {e}"))?;
        (EncryptionSource::File { path: path.clone() }, *k)
    } else {
        let p = EnvKeyProvider::from_default_env()
            .map_err(|e| anyhow::anyhow!("failed to load NEXUS_DATA_KEY env var: {e}"))?;
        let k = p
            .master_key()
            .map_err(|e| anyhow::anyhow!("failed to read NEXUS_DATA_KEY: {e}"))?;
        (
            EncryptionSource::Env {
                name: "NEXUS_DATA_KEY".to_string(),
            },
            *k,
        )
    };
    debug_assert_eq!(key.len(), MASTER_KEY_LEN);

    Ok(EncryptionConfig {
        enabled: true,
        source: Some(source),
        fingerprint: Some(fingerprint_master_key(&key)),
    })
}

/// SHA-256 fingerprint of the master key — first 16 hex digits of
/// the digest, prefixed with `nexus:` for log readability. Short
/// enough to fit in a log line, long enough that birthday-collisions
/// against a hostile observer are negligible (`2^32` keys).
pub fn fingerprint_master_key(key: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"nexus-master-key-fingerprint-v1");
    hasher.update(key);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(22);
    out.push_str("nexus:");
    for byte in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod encryption_tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_for_the_same_key() {
        let k = [0x42u8; 32];
        let a = fingerprint_master_key(&k);
        let b = fingerprint_master_key(&k);
        assert_eq!(a, b);
        assert!(a.starts_with("nexus:"));
        assert_eq!(a.len(), "nexus:".len() + 16);
    }

    #[test]
    fn fingerprint_changes_with_the_key() {
        let a = fingerprint_master_key(&[0u8; 32]);
        let b = fingerprint_master_key(&[1u8; 32]);
        assert_ne!(a, b);
    }

    #[test]
    fn fingerprint_does_not_leak_key_bytes() {
        let k = [0xAAu8; 32];
        let fp = fingerprint_master_key(&k);
        // Naive check: the literal hex of the key must not appear
        // in the fingerprint output.
        assert!(!fp.contains("aaaaaa"), "fingerprint leaked key bytes");
    }

    #[test]
    fn resolve_disabled_returns_default() {
        // Use a unique env var so parallel tests don't collide.
        unsafe { std::env::remove_var("NEXUS_ENCRYPT_AT_REST") };
        let cfg = resolve_encryption_config().expect("resolve");
        assert!(!cfg.enabled);
        assert!(cfg.source.is_none());
        assert!(cfg.fingerprint.is_none());
    }

    #[test]
    fn resolve_with_env_key_records_source_and_fingerprint() {
        unsafe { std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true") };
        unsafe { std::env::set_var("NEXUS_DATA_KEY", "a".repeat(64)) };
        unsafe { std::env::remove_var("NEXUS_KEY_FILE") };
        let cfg = resolve_encryption_config().expect("resolve");
        assert!(cfg.enabled);
        assert_eq!(
            cfg.source,
            Some(EncryptionSource::Env {
                name: "NEXUS_DATA_KEY".into()
            })
        );
        assert!(cfg.fingerprint.unwrap().starts_with("nexus:"));
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
    }

    #[test]
    fn resolve_rejects_bad_key_format() {
        unsafe { std::env::set_var("NEXUS_ENCRYPT_AT_REST", "true") };
        // 8-byte key — invalid; expect 32 raw or 64 hex.
        unsafe { std::env::set_var("NEXUS_DATA_KEY", "shortkey") };
        unsafe { std::env::remove_var("NEXUS_KEY_FILE") };
        let err = resolve_encryption_config().unwrap_err();
        assert!(err.to_string().contains("NEXUS_DATA_KEY"));
        unsafe {
            std::env::remove_var("NEXUS_ENCRYPT_AT_REST");
            std::env::remove_var("NEXUS_DATA_KEY");
        }
    }
}

// Intermediate structs for parsing config.yml (a subset of its schema).
// Everything is optional so partial configs work and the YAML file can
// evolve without breaking deployments.

/// Values that YAML parsing can contribute. All optional; env vars still
/// win and unset fields fall back to compiled defaults.
#[derive(Debug, Default, Clone)]
pub struct YamlOverrides {
    /// `server.addr`
    pub addr: Option<String>,
    /// `server.max_body_size_mb`
    pub max_body_size_mb: Option<usize>,
    /// `storage.data_dir`
    pub data_dir: Option<String>,
    /// `storage.page_cache.capacity`
    pub page_cache_capacity: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct YamlRootConfig {
    server: YamlServerSection,
    storage: YamlStorageSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct YamlServerSection {
    addr: Option<String>,
    max_body_size_mb: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct YamlStorageSection {
    data_dir: Option<String>,
    page_cache: YamlPageCacheSection,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct YamlPageCacheSection {
    capacity: Option<usize>,
}

/// Authentication configuration file structure
#[derive(Debug, Deserialize)]
struct AuthConfigFile {
    #[serde(default)]
    root_user: RootUserConfig,
    #[serde(default)]
    auth: AuthConfig,
}

impl Config {
    /// Load authentication configuration from `config/auth.toml` file
    /// Returns None if file doesn't exist or can't be parsed
    pub fn from_auth_file(config_dir: impl AsRef<Path>) -> Option<(RootUserConfig, AuthConfig)> {
        let config_path = config_dir.as_ref().join("auth.toml");

        if !config_path.exists() {
            tracing::debug!("Auth config file not found: {:?}", config_path);
            return None;
        }

        match std::fs::read_to_string(&config_path) {
            Ok(content) => match toml::from_str::<AuthConfigFile>(&content) {
                Ok(config) => {
                    tracing::info!("Loaded auth configuration from {:?}", config_path);
                    Some((config.root_user, config.auth))
                }
                Err(e) => {
                    tracing::warn!("Failed to parse auth config file {:?}: {}", config_path, e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read auth config file {:?}: {}", config_path, e);
                None
            }
        }
    }

    /// Parse a `config.yml`-style file, returning only the fields this
    /// binary wires up. Missing fields fall back to `None` and are
    /// substituted later by defaults or env vars.
    pub fn from_yaml_file(path: impl AsRef<Path>) -> Option<YamlOverrides> {
        let path = path.as_ref();
        if !path.exists() {
            tracing::debug!("YAML config file not found: {:?}", path);
            return None;
        }
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_yaml::from_str::<YamlRootConfig>(&content) {
                Ok(parsed) => {
                    tracing::info!("Loaded YAML configuration from {:?}", path);
                    Some(YamlOverrides {
                        addr: parsed.server.addr,
                        max_body_size_mb: parsed.server.max_body_size_mb,
                        data_dir: parsed.storage.data_dir,
                        page_cache_capacity: parsed.storage.page_cache.capacity,
                    })
                }
                Err(e) => {
                    tracing::warn!("Failed to parse YAML config {:?}: {}", path, e);
                    None
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read YAML config {:?}: {}", path, e);
                None
            }
        }
    }

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

        Self {
            addr,
            data_dir,
            max_body_size_bytes,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.data_dir, "./data");
    }

    #[test]
    fn test_config_getters() {
        let config = Config::default();
        assert_eq!(config.addr(), &config.addr);
        assert_eq!(config.data_dir(), "./data");
    }

    #[test]
    fn test_config_with_data_dir() {
        let config = Config::default().with_data_dir("/custom/data");
        assert_eq!(config.data_dir, "/custom/data");
    }

    #[test]
    fn test_config_with_addr() {
        let new_addr = "192.168.1.100:8080".parse().unwrap();
        let config = Config::default().with_addr(new_addr);
        assert_eq!(config.addr, new_addr);
    }

    #[test]
    fn test_config_chaining() {
        let new_addr = "10.0.0.1:9000".parse().unwrap();
        let config = Config::default()
            .with_data_dir("/tmp/nexus")
            .with_addr(new_addr);

        assert_eq!(config.data_dir, "/tmp/nexus");
        assert_eq!(config.addr, new_addr);
    }

    #[test]
    #[ignore = "Environment variable tests can have race conditions when run in parallel"]
    fn test_config_from_env_default() {
        // Clear environment variables to test defaults
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        let config = Config::from_env();
        assert_eq!(config.addr.port(), 15474);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
        assert_eq!(config.data_dir, "./data");

        // Clean up
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }
    }

    #[test]
    #[ignore = "Environment variable tests can have race conditions when run in parallel"]
    fn test_config_from_env_custom() {
        // Clean up any existing environment variables first
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        // Set custom environment variables
        unsafe {
            std::env::set_var("NEXUS_ADDR", "192.168.1.50:3000");
            std::env::set_var("NEXUS_DATA_DIR", "/var/lib/nexus");
        }

        let config = Config::from_env();
        assert_eq!(config.addr.port(), 3000);
        assert_eq!(config.addr.ip(), IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50)));
        assert_eq!(config.data_dir, "/var/lib/nexus");

        // Clean up
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }
    }

    #[test]
    #[ignore = "Environment variable tests can have race conditions when run in parallel"]
    fn test_config_from_env_partial() {
        // Clean up any existing environment variables first
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }

        // Set only one environment variable
        unsafe {
            std::env::set_var("NEXUS_DATA_DIR", "/custom/data");
            std::env::remove_var("NEXUS_ADDR");
        }

        let config = Config::from_env();
        assert_eq!(config.addr.port(), 15474); // Default
        assert_eq!(config.data_dir, "/custom/data"); // From env

        // Clean up
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
            std::env::remove_var("NEXUS_DATA_DIR");
        }
    }

    #[test]
    #[should_panic(expected = "Invalid NEXUS_ADDR")]
    fn test_config_from_env_invalid_addr() {
        unsafe {
            std::env::set_var("NEXUS_ADDR", "invalid-address");
        }

        let _config = Config::from_env();

        // Clean up
        unsafe {
            std::env::remove_var("NEXUS_ADDR");
        }
    }

    #[test]
    fn test_config_clone() {
        let config1 = Config::default();
        let config2 = config1.clone();

        assert_eq!(config1.addr, config2.addr);
        assert_eq!(config1.data_dir, config2.data_dir);
    }

    #[test]
    fn test_config_debug() {
        let config = Config::default();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("127.0.0.1:15474"));
        assert!(debug_str.contains("./data"));
    }

    #[test]
    fn test_root_user_config_default() {
        let root_config = RootUserConfig::default();
        assert_eq!(root_config.username, "root");
        assert_eq!(root_config.password, "root");
        assert!(root_config.enabled);
        assert!(!root_config.disable_after_setup);
    }

    #[test]
    fn test_config_with_root_user() {
        let config = Config::default();
        assert_eq!(config.root_user.username, "root");
        assert_eq!(config.root_user.password, "root");
        assert!(config.root_user.enabled);
    }

    #[test]
    fn test_from_auth_file_not_found() {
        // Test when file doesn't exist
        let result = Config::from_auth_file("/nonexistent/path");
        assert!(result.is_none());
    }

    #[test]
    fn test_from_auth_file_valid() {
        use nexus_core::testing::TestContext;

        let ctx = TestContext::new();
        let config_dir = ctx.path();
        let config_file = config_dir.join("auth.toml");

        // Create a valid config file
        std::fs::write(
            &config_file,
            r#"
[root_user]
username = "admin"
password = "secret123"
enabled = false
disable_after_setup = true

[auth]
enabled = true
required_for_public = false
require_health_auth = true
"#,
        )
        .unwrap();

        let result = Config::from_auth_file(config_dir);
        assert!(result.is_some());

        let (root_user, auth) = result.unwrap();
        assert_eq!(root_user.username, "admin");
        assert_eq!(root_user.password, "secret123");
        assert!(!root_user.enabled);
        assert!(root_user.disable_after_setup);
        assert!(auth.enabled);
        assert!(!auth.required_for_public);
        assert!(auth.require_health_auth);
    }

    #[test]
    fn test_from_auth_file_invalid_toml() {
        use nexus_core::testing::TestContext;

        let ctx = TestContext::new();
        let config_dir = ctx.path();
        let config_file = config_dir.join("auth.toml");

        // Create an invalid TOML file
        std::fs::write(&config_file, "invalid toml content [").unwrap();

        let result = Config::from_auth_file(config_dir);
        assert!(result.is_none());
    }

    #[test]
    fn test_from_auth_file_partial_config() {
        use nexus_core::testing::TestContext;

        let ctx = TestContext::new();
        let config_dir = ctx.path();
        let config_file = config_dir.join("auth.toml");

        // Create a config file with only root_user section
        std::fs::write(
            &config_file,
            r#"
[root_user]
username = "custom_root"
password = "custom_pass"
"#,
        )
        .unwrap();

        let result = Config::from_auth_file(config_dir);
        assert!(result.is_some());

        let (root_user, auth) = result.unwrap();
        assert_eq!(root_user.username, "custom_root");
        assert_eq!(root_user.password, "custom_pass");
        // Auth should use defaults
        assert!(!auth.enabled);
    }

    #[test]
    fn test_from_yaml_file_parses_subset() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.yml");
        std::fs::write(
            &path,
            r#"
server:
  addr: "0.0.0.0:9999"
  max_body_size_mb: 7
  workers: 4           # unrelated field — must be ignored without error
storage:
  data_dir: "/custom/data"
  page_cache:
    capacity: 2048
    eviction_policy: "clock"
"#,
        )
        .unwrap();

        let overrides = Config::from_yaml_file(&path).expect("yaml should parse");
        assert_eq!(overrides.addr.as_deref(), Some("0.0.0.0:9999"));
        assert_eq!(overrides.max_body_size_mb, Some(7));
        assert_eq!(overrides.data_dir.as_deref(), Some("/custom/data"));
        assert_eq!(overrides.page_cache_capacity, Some(2048));
    }

    #[test]
    fn test_from_yaml_file_missing_returns_none() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("does-not-exist.yml");
        assert!(Config::from_yaml_file(&path).is_none());
    }

    #[test]
    fn test_from_yaml_file_partial_ok() {
        // Only page_cache.capacity set — everything else should be None.
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("partial.yml");
        std::fs::write(&path, "storage:\n  page_cache:\n    capacity: 500\n").unwrap();

        let overrides = Config::from_yaml_file(&path).expect("yaml should parse");
        assert_eq!(overrides.addr, None);
        assert_eq!(overrides.max_body_size_mb, None);
        assert_eq!(overrides.data_dir, None);
        assert_eq!(overrides.page_cache_capacity, Some(500));
    }
}
