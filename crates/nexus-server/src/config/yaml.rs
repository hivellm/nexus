//! `config.yml` / `auth.toml` parsing: intermediate serde structs plus
//! the `Config::from_auth_file` / `Config::from_yaml_file` loaders.
//!
//! Extracted from `config.rs` as a pure mechanical move — every struct
//! and method below is byte-identical to the original.

use serde::Deserialize;
use std::path::Path;

use super::{AuthConfig, Config, RootUserConfig};

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
}
