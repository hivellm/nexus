//! Server configuration

use std::net::SocketAddr;

/// Server configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Server bind address
    pub addr: SocketAddr,
    /// Data directory
    pub data_dir: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            addr: "127.0.0.1:7474".parse().unwrap(),
            data_dir: "./data".to_string(),
        }
    }
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let addr = std::env::var("NEXUS_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:7474".to_string())
            .parse()
            .expect("Invalid NEXUS_ADDR");

        let data_dir = std::env::var("NEXUS_DATA_DIR").unwrap_or_else(|_| "./data".to_string());

        Self { addr, data_dir }
    }
}
