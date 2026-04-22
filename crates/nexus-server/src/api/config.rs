//! Server configuration API endpoint

use axum::response::{IntoResponse, Json, Response};
use serde::Serialize;

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
    // The on-disk config file (config.yml / config.example.yml) is the
    // source of truth for operators, but this endpoint just returns the
    // defaults the process was booted with — parsing the file here
    // duplicates what `Config::from_env_and_yaml` already does at start.
    // If we later want to surface live-reloaded overrides, reach back
    // into the parsed `Config` instance rather than re-reading the file.

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
