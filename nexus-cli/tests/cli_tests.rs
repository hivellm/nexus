//! CLI unit and integration tests

use std::process::Command;

/// Test that CLI binary exists and shows help
#[test]
fn test_cli_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nexus"));
    assert!(stdout.contains("Command-line interface"));
}

/// Test that CLI shows version
#[test]
fn test_cli_version() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "--version"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("nexus"));
}

/// Test that subcommand help works
#[test]
fn test_query_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "query", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("query") || stdout.contains("Query") || stdout.contains("Cypher"));
}

/// Test db subcommand help
#[test]
fn test_db_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "db", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("db") || stdout.contains("Database"));
}

/// Test user subcommand help
#[test]
fn test_user_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "user", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("user") || stdout.contains("User"));
}

/// Test key subcommand help
#[test]
fn test_key_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "key", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("key") || stdout.contains("Key") || stdout.contains("API"));
}

/// Test schema subcommand help
#[test]
fn test_schema_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "schema", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("schema") || stdout.contains("Schema"));
}

/// Test admin subcommand help
#[test]
fn test_admin_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "admin", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("admin") || stdout.contains("Admin"));
}

/// Test config subcommand help
#[test]
fn test_config_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "config", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("config") || stdout.contains("Config"));
}

/// Test data subcommand help
#[test]
fn test_data_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "data", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("data")
            || stdout.contains("Data")
            || stdout.contains("import")
            || stdout.contains("export")
    );
}

/// Test that invalid command returns error
#[test]
fn test_invalid_command() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "invalid-command"])
        .output()
        .expect("Failed to execute command");

    // Should fail with non-zero exit code
    assert!(!output.status.success());
}

/// Test exit codes
#[test]
fn test_exit_code_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "--help"])
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());
}

/// Test query pagination options
#[test]
fn test_query_pagination_options() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "query", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--limit"));
    assert!(stdout.contains("--skip"));
}

/// Test query filter option
#[test]
fn test_query_filter_option() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "query", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--filter"));
}

/// Test query sort option
#[test]
fn test_query_sort_option() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "query", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--sort"));
}

/// Test query history option
#[test]
fn test_query_history_option() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "query", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--history"));
}

/// Test key create rate limit option
#[test]
fn test_key_rate_limit_option() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "key", "create", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--rate-limit"));
}

/// Test key create expires option
#[test]
fn test_key_expires_option() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "key", "create", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--expires"));
}

/// Test data backup subcommand
#[test]
fn test_data_backup_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "data", "backup", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backup") || stdout.contains("Backup"));
    assert!(stdout.contains("--compress"));
}

/// Test data restore subcommand
#[test]
fn test_data_restore_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "data", "restore", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("restore") || stdout.contains("Restore"));
}

/// Test data backups subcommand
#[test]
fn test_data_backups_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "data", "backups", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("backups") || stdout.contains("list"));
}

/// Test user update subcommand
#[test]
fn test_user_update_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "user", "update", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("update") || stdout.contains("Update"));
}

/// Test user passwd subcommand
#[test]
fn test_user_passwd_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "user", "passwd", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("passwd") || stdout.contains("password"));
}

/// Test key rotate subcommand
#[test]
fn test_key_rotate_help() {
    let output = Command::new("cargo")
        .args(["run", "-p", "nexus-cli", "--", "key", "rotate", "--help"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("rotate") || stdout.contains("Rotate"));
}

/// Test schema labels create subcommand
#[test]
fn test_schema_labels_create_help() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "nexus-cli",
            "--",
            "schema",
            "labels",
            "create",
            "--help",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("create") || stdout.contains("Create"));
}

/// Test schema indexes create subcommand
#[test]
fn test_schema_indexes_create_help() {
    let output = Command::new("cargo")
        .args([
            "run",
            "-p",
            "nexus-cli",
            "--",
            "schema",
            "indexes",
            "create",
            "--help",
        ])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--label"));
    assert!(stdout.contains("--property"));
}
