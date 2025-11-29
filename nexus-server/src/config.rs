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
    /// Root user configuration
    pub root_user: RootUserConfig,
    /// Authentication configuration
    pub auth: AuthConfig,
    /// Multi-database configuration
    pub multi_database: MultiDatabaseConfig,
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
            root_user: RootUserConfig::default(),
            auth: AuthConfig::default(),
            multi_database: MultiDatabaseConfig::default(),
        }
    }
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

    /// Load configuration from environment variables and config file
    /// Priority: Environment variables > config file > defaults
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        let addr = std::env::var("NEXUS_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:15474".to_string())
            .parse()
            .expect("Invalid NEXUS_ADDR");

        let data_dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());

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

        Self {
            addr,
            data_dir,
            root_user,
            auth,
            multi_database: MultiDatabaseConfig::default(),
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
}
