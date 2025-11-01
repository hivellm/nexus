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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:15474".parse().unwrap(),
            data_dir: "./data".to_string(),
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

        Self { addr, data_dir }
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
}
