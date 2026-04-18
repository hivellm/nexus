//! Configuration module unit tests

use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

// Re-create the Config struct for testing (since we can't import from binary crate)
#[derive(Debug, Default, serde::Serialize, serde::Deserialize, PartialEq)]
struct Config {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default)]
    pub profiles: HashMap<String, Profile>,
    #[serde(default)]
    pub default_profile: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
struct Profile {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl Config {
    fn load_from_path(path: &std::path::Path) -> anyhow::Result<Self> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    fn save_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    fn get_profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    fn set_profile(&mut self, name: &str, profile: Profile) {
        self.profiles.insert(name.to_string(), profile);
    }

    fn remove_profile(&mut self, name: &str) -> bool {
        self.profiles.remove(name).is_some()
    }

    fn list_profiles(&self) -> Vec<&String> {
        self.profiles.keys().collect()
    }
}

#[test]
fn test_config_default() {
    let config = Config::default();
    assert!(config.url.is_none());
    assert!(config.api_key.is_none());
    assert!(config.username.is_none());
    assert!(config.password.is_none());
    assert!(config.profiles.is_empty());
    assert!(config.default_profile.is_none());
}

#[test]
fn test_config_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let config = Config {
        url: Some("http://localhost:3000".to_string()),
        api_key: Some("test-key".to_string()),
        username: Some("admin".to_string()),
        password: Some("secret".to_string()),
        profiles: HashMap::new(),
        default_profile: None,
    };

    config.save_to_path(&config_path).unwrap();
    assert!(config_path.exists());

    let loaded = Config::load_from_path(&config_path).unwrap();
    assert_eq!(loaded.url, config.url);
    assert_eq!(loaded.api_key, config.api_key);
    assert_eq!(loaded.username, config.username);
    assert_eq!(loaded.password, config.password);
}

#[test]
fn test_config_load_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("nonexistent.toml");

    let config = Config::load_from_path(&config_path).unwrap();
    assert!(config.url.is_none());
}

#[test]
fn test_config_profiles() {
    let mut config = Config::default();

    let profile = Profile {
        url: "http://production:3000".to_string(),
        api_key: Some("prod-key".to_string()),
        username: Some("prod-user".to_string()),
        password: None,
    };

    config.set_profile("production", profile.clone());
    assert_eq!(config.profiles.len(), 1);

    let retrieved = config.get_profile("production").unwrap();
    assert_eq!(retrieved.url, profile.url);
    assert_eq!(retrieved.api_key, profile.api_key);

    assert!(config.get_profile("nonexistent").is_none());
}

#[test]
fn test_config_remove_profile() {
    let mut config = Config::default();

    let profile = Profile {
        url: "http://test:3000".to_string(),
        api_key: None,
        username: None,
        password: None,
    };

    config.set_profile("test", profile);
    assert_eq!(config.profiles.len(), 1);

    assert!(config.remove_profile("test"));
    assert_eq!(config.profiles.len(), 0);

    assert!(!config.remove_profile("nonexistent"));
}

#[test]
fn test_config_list_profiles() {
    let mut config = Config::default();

    let profile1 = Profile {
        url: "http://prod:3000".to_string(),
        api_key: None,
        username: None,
        password: None,
    };

    let profile2 = Profile {
        url: "http://dev:3000".to_string(),
        api_key: None,
        username: None,
        password: None,
    };

    config.set_profile("production", profile1);
    config.set_profile("development", profile2);

    let profiles = config.list_profiles();
    assert_eq!(profiles.len(), 2);
    assert!(profiles.iter().any(|p| *p == "production"));
    assert!(profiles.iter().any(|p| *p == "development"));
}

#[test]
fn test_config_serialization() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let mut config = Config {
        url: Some("http://localhost:3000".to_string()),
        api_key: None,
        username: Some("root".to_string()),
        password: None,
        profiles: HashMap::new(),
        default_profile: Some("production".to_string()),
    };

    let profile = Profile {
        url: "http://production:3000".to_string(),
        api_key: Some("prod-key".to_string()),
        username: None,
        password: None,
    };
    config.set_profile("production", profile);

    config.save_to_path(&config_path).unwrap();

    let content = fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("url = \"http://localhost:3000\""));
    assert!(content.contains("[profiles.production]"));
    assert!(content.contains("default_profile = \"production\""));
}

#[test]
fn test_config_toml_parsing() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("config.toml");

    let toml_content = r#"
url = "http://myserver:3000"
username = "myuser"

[profiles.local]
url = "http://localhost:3000"

[profiles.remote]
url = "http://remote:3000"
api_key = "remote-key"
"#;

    fs::write(&config_path, toml_content).unwrap();

    let config = Config::load_from_path(&config_path).unwrap();
    assert_eq!(config.url, Some("http://myserver:3000".to_string()));
    assert_eq!(config.username, Some("myuser".to_string()));
    assert_eq!(config.profiles.len(), 2);

    let local = config.get_profile("local").unwrap();
    assert_eq!(local.url, "http://localhost:3000");
    assert!(local.api_key.is_none());

    let remote = config.get_profile("remote").unwrap();
    assert_eq!(remote.url, "http://remote:3000");
    assert_eq!(remote.api_key, Some("remote-key".to_string()));
}

#[test]
fn test_config_validation() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("invalid.toml");

    // Invalid TOML should fail
    fs::write(&config_path, "this is not valid toml [[[").unwrap();

    let result = Config::load_from_path(&config_path);
    assert!(result.is_err());
}

#[test]
fn test_profile_default_values() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("minimal.toml");

    let toml_content = r#"
[profiles.minimal]
url = "http://minimal:3000"
"#;

    fs::write(&config_path, toml_content).unwrap();

    let config = Config::load_from_path(&config_path).unwrap();
    let profile = config.get_profile("minimal").unwrap();

    assert_eq!(profile.url, "http://minimal:3000");
    assert!(profile.api_key.is_none());
    assert!(profile.username.is_none());
    assert!(profile.password.is_none());
}
