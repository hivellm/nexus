//! Server configuration API endpoint

use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;

/// Server configuration response
#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
}

/// Server configuration
#[derive(Debug, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: String,
    pub log_level: String,
}

/// Database configuration
#[derive(Debug, Serialize)]
pub struct DatabaseConfig {
    pub data_dir: String,
    pub cache_size_mb: usize,
}

/// Authentication configuration
#[derive(Debug, Serialize)]
pub struct AuthConfig {
    pub enabled: bool,
    pub provider: String,
}

/// Get server configuration
pub async fn get_config() -> Response {
    // Try to read from config file
    let config_paths = vec![
        PathBuf::from("./config.yml"),
        PathBuf::from("./config.example.yml"),
    ];

    let mut config_data = serde_json::json!({});

    for config_path in config_paths {
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                // Try to parse YAML (simplified - would need yaml parsing library)
                // For now, return a basic structure
                break;
            }
        }
    }

    // Return default configuration
    let config = ConfigResponse {
        server: ServerConfig {
            host: "localhost".to_string(),
            port: 15474,
            data_dir: "./data".to_string(),
            log_level: "info".to_string(),
        },
        database: DatabaseConfig {
            data_dir: "./data".to_string(),
            cache_size_mb: 512,
        },
        auth: AuthConfig {
            enabled: false,
            provider: "none".to_string(),
        },
    };

    Json(config).into_response()
}
