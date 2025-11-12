//! Server configuration

use std::net::SocketAddr;

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
}

/// Root user configuration
#[derive(Debug, Clone)]
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

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:15474".parse().unwrap(),
            data_dir: "./data".to_string(),
            root_user: RootUserConfig::default(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        let addr = std::env::var("NEXUS_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:15474".to_string())
            .parse()
            .expect("Invalid NEXUS_ADDR");

        let data_dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());

        // Load root user configuration from environment
        let root_username =
            std::env::var("NEXUS_ROOT_USERNAME").unwrap_or_else(|_| "root".to_string());

        // Support Docker secrets: try NEXUS_ROOT_PASSWORD_FILE first, then NEXUS_ROOT_PASSWORD
        let root_password = if let Ok(password_file) = std::env::var("NEXUS_ROOT_PASSWORD_FILE") {
            std::fs::read_to_string(&password_file)
                .unwrap_or_else(|_| "root".to_string())
                .trim()
                .to_string()
        } else {
            std::env::var("NEXUS_ROOT_PASSWORD").unwrap_or_else(|_| "root".to_string())
        };

        let root_enabled = std::env::var("NEXUS_ROOT_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .parse::<bool>()
            .unwrap_or(true);

        let disable_after_setup = std::env::var("NEXUS_DISABLE_ROOT_AFTER_SETUP")
            .unwrap_or_else(|_| "false".to_string())
            .parse::<bool>()
            .unwrap_or(false);

        let root_user = RootUserConfig {
            username: root_username,
            password: root_password,
            enabled: root_enabled,
            disable_after_setup,
        };

        Self {
            addr,
            data_dir,
            root_user,
        }
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
}
