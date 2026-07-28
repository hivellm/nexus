//! Unit tests for the `config` module facade.
//!
//! Extracted from `config.rs` as a pure mechanical move — every test
//! below is byte-identical to the original.

// Tests build a `Config::default()` then set the few fields under test —
// clearer than spelling out every unrelated field via struct-update.
#![allow(clippy::field_reassign_with_default)]

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

// ------------------------------------------------------------------
// H1 + M2 — boot-time security preflight
// ------------------------------------------------------------------

#[test]
fn security_preflight_rejects_public_bind_with_auth_disabled() {
    let mut config = Config::default();
    config.addr = "0.0.0.0:15474".parse().unwrap();
    config.auth.enabled = false;
    config.auth.required_for_public = true;

    let err = config
        .security_preflight()
        .expect_err("public bind + auth disabled must be rejected");
    assert!(err.contains("not loopback"), "got {err}");
}

#[test]
fn security_preflight_allows_loopback_bind_with_auth_disabled() {
    let mut config = Config::default();
    config.addr = "127.0.0.1:15474".parse().unwrap();
    config.auth.enabled = false;
    config.auth.required_for_public = true;

    config
        .security_preflight()
        .expect("loopback bind must be allowed even with auth disabled");
}

#[test]
fn security_preflight_allows_public_bind_when_operator_opts_out() {
    let mut config = Config::default();
    config.addr = "0.0.0.0:15474".parse().unwrap();
    config.auth.enabled = false;
    config.auth.required_for_public = false;

    config
        .security_preflight()
        .expect("operator opt-out via required_for_public=false must be honored");
}

#[test]
fn security_preflight_rejects_default_root_password_when_auth_enabled() {
    let mut config = Config::default();
    config.addr = "127.0.0.1:15474".parse().unwrap();
    config.auth.enabled = true;
    config.root_user.enabled = true;
    config.root_user.password = "root".to_string();

    let err = config
        .security_preflight()
        .expect_err("default root password with auth enabled must be rejected");
    assert!(err.contains("default"), "got {err}");
}

#[test]
fn security_preflight_allows_custom_root_password_when_auth_enabled() {
    let mut config = Config::default();
    config.addr = "127.0.0.1:15474".parse().unwrap();
    config.auth.enabled = true;
    config.root_user.enabled = true;
    config.root_user.password = "a-strong-unique-secret".to_string();

    config
        .security_preflight()
        .expect("custom root password with auth enabled must be allowed");
}

#[test]
fn security_preflight_allows_default_root_password_when_root_user_disabled() {
    let mut config = Config::default();
    config.addr = "127.0.0.1:15474".parse().unwrap();
    config.auth.enabled = true;
    config.root_user.enabled = false;
    config.root_user.password = "root".to_string();

    config
        .security_preflight()
        .expect("disabled root user must not trip the default-password guard");
}
