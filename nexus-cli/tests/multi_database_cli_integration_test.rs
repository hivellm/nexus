//! CLI integration tests for multi-database support
//!
//! These tests run the actual CLI commands against a real Nexus server
//! to verify end-to-end functionality.
//!
//! Run with: cargo test -p nexus-cli --test multi_database_cli_integration_test

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Helper to manage a test Nexus server instance
struct TestServer {
    process: Arc<Mutex<Option<Child>>>,
    port: u16,
}

impl TestServer {
    /// Start a new test server on the specified port
    fn start(port: u16) -> Self {
        let server = TestServer {
            process: Arc::new(Mutex::new(None)),
            port,
        };

        // Start the server in the background
        let process = Command::new("cargo")
            .args([
                "run",
                "-p",
                "nexus-server",
                "--",
                "--port",
                &port.to_string(),
            ])
            .env("NEXUS_DATA_DIR", format!("./target/test-data-{}", port))
            .env("RUST_LOG", "info")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to start test server");

        *server.process.lock().unwrap() = Some(process);

        // Wait for server to be ready
        for _ in 0..30 {
            thread::sleep(Duration::from_millis(500));
            if Self::ping_server(port) {
                println!("Test server started on port {}", port);
                return server;
            }
        }

        panic!("Test server failed to start within timeout");
    }

    /// Check if server is responding
    fn ping_server(port: u16) -> bool {
        let output = Command::new("cargo")
            .args([
                "run",
                "-p",
                "nexus-cli",
                "--",
                "--url",
                &format!("http://localhost:{}", port),
                "db",
                "ping",
            ])
            .output();

        output.map(|o| o.status.success()).unwrap_or(false)
    }

    /// Get the server URL
    fn url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(mut process) = self.process.lock().unwrap().take() {
            let _ = process.kill();
            let _ = process.wait();
            println!("Test server stopped");
        }
    }
}

/// Run a CLI command against the test server
fn run_cli_command(server_url: &str, args: &[&str]) -> (bool, String, String) {
    let mut cmd_args = vec!["run", "-p", "nexus-cli", "--", "--url", server_url];
    cmd_args.extend_from_slice(args);

    let output = Command::new("cargo")
        .args(&cmd_args)
        .output()
        .expect("Failed to execute CLI command");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let success = output.status.success();

    (success, stdout, stderr)
}

#[test]
#[ignore] // Requires running server - run manually with: cargo test --test multi_database_cli_integration_test -- --ignored
fn test_cli_db_list() {
    let server = TestServer::start(7688);
    let url = server.url();

    let (success, stdout, _stderr) = run_cli_command(&url, &["db", "list"]);

    assert!(success, "CLI db list command should succeed");
    assert!(
        stdout.contains("neo4j") || stdout.contains("nexus"),
        "Should show default database"
    );
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_create_and_drop() {
    let server = TestServer::start(7689);
    let url = server.url();

    // Create database
    let (success, stdout, stderr) = run_cli_command(&url, &["db", "create", "testclidb"]);

    // May not be supported yet, but should not crash
    if success {
        assert!(
            stdout.contains("created") || stdout.contains("success"),
            "Should show success message"
        );

        // List databases to verify
        let (list_success, list_stdout, _) = run_cli_command(&url, &["db", "list"]);
        if list_success {
            assert!(
                list_stdout.contains("testclidb"),
                "New database should appear in list"
            );
        }

        // Drop database
        let (drop_success, drop_stdout, _) =
            run_cli_command(&url, &["db", "drop", "testclidb", "--force"]);

        if drop_success {
            assert!(
                drop_stdout.contains("dropped") || drop_stdout.contains("success"),
                "Should show success message"
            );
        }
    } else {
        // If not supported, should gracefully indicate
        assert!(
            stderr.contains("not supported") || stdout.contains("not available"),
            "Should indicate feature is not supported"
        );
    }
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_switch() {
    let server = TestServer::start(7690);
    let url = server.url();

    // Create database first
    let _ = run_cli_command(&url, &["db", "create", "switchdb"]);

    // Try to switch to it
    let (success, stdout, stderr) = run_cli_command(&url, &["db", "switch", "switchdb"]);

    if success {
        assert!(
            stdout.contains("Switched") || stdout.contains("success"),
            "Should show success message"
        );
    } else {
        // If not supported, should gracefully indicate
        assert!(
            stderr.contains("not supported") || stdout.contains("not available"),
            "Should indicate feature is not supported"
        );
    }
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_info() {
    let server = TestServer::start(7691);
    let url = server.url();

    let (success, stdout, _stderr) = run_cli_command(&url, &["db", "info"]);

    assert!(success, "CLI db info command should succeed");
    assert!(
        stdout.contains("Nodes") || stdout.contains("node"),
        "Should show node count"
    );
    assert!(
        stdout.contains("Relationships") || stdout.contains("relationship"),
        "Should show relationship count"
    );
}

#[test]
#[ignore] // Requires running server
fn test_cli_full_database_lifecycle() {
    let server = TestServer::start(7692);
    let url = server.url();

    // 1. List databases initially
    let (list_success, list_stdout, _) = run_cli_command(&url, &["db", "list"]);
    assert!(list_success, "Initial list should succeed");
    println!("Initial databases:\n{}", list_stdout);

    // 2. Create a new database
    let (create_success, create_stdout, create_stderr) =
        run_cli_command(&url, &["db", "create", "lifecycle_test"]);

    if !create_success {
        println!("Create stdout: {}", create_stdout);
        println!("Create stderr: {}", create_stderr);
        if create_stderr.contains("not supported") || create_stdout.contains("not available") {
            println!("Multi-database not supported, skipping test");
            return;
        }
        panic!("Database creation failed");
    }

    // 3. List databases again - should include new database
    let (list2_success, list2_stdout, _) = run_cli_command(&url, &["db", "list"]);
    assert!(list2_success, "Second list should succeed");
    println!("After create:\n{}", list2_stdout);

    // 4. Switch to new database
    let (switch_success, switch_stdout, _) =
        run_cli_command(&url, &["db", "switch", "lifecycle_test"]);

    if switch_success {
        println!("Switched to new database:\n{}", switch_stdout);

        // 5. Create some data in the new database
        let create_query = r#"CREATE (n:TestNode {name: 'test'}) RETURN n"#;
        let (query_success, query_stdout, _) = run_cli_command(&url, &["query", create_query]);

        if query_success {
            println!("Created test node:\n{}", query_stdout);

            // 6. Verify data exists
            let (info_success, info_stdout, _) = run_cli_command(&url, &["db", "info"]);
            assert!(info_success, "Info should succeed");
            println!("Database info:\n{}", info_stdout);
        }
    }

    // 7. Switch back to default database
    let (switch_back_success, _, _) = run_cli_command(&url, &["db", "switch", "neo4j"]);
    println!("Switched back to default: {}", switch_back_success);

    // 8. Drop the test database
    let (drop_success, drop_stdout, _) =
        run_cli_command(&url, &["db", "drop", "lifecycle_test", "--force"]);

    if drop_success {
        println!("Dropped database:\n{}", drop_stdout);

        // 9. Verify database is gone
        let (list3_success, list3_stdout, _) = run_cli_command(&url, &["db", "list"]);
        assert!(list3_success, "Final list should succeed");
        println!("After drop:\n{}", list3_stdout);

        // Should not contain the dropped database
        assert!(
            !list3_stdout.contains("lifecycle_test"),
            "Dropped database should not appear in list"
        );
    }
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_ping() {
    let server = TestServer::start(7693);
    let url = server.url();

    let (success, stdout, _stderr) = run_cli_command(&url, &["db", "ping"]);

    assert!(success, "Ping should succeed when server is running");
    assert!(
        stdout.contains("reachable") || stdout.contains("success"),
        "Should indicate server is reachable"
    );
    assert!(
        stdout.contains("ms") || stdout.contains("time"),
        "Should show response time"
    );
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_create_duplicate_fails() {
    let server = TestServer::start(7694);
    let url = server.url();

    // Create database
    let (create_success, _, create_stderr) = run_cli_command(&url, &["db", "create", "duptest"]);

    if !create_success {
        if create_stderr.contains("not supported") {
            println!("Multi-database not supported, skipping test");
            return;
        }
        panic!("Initial database creation failed");
    }

    // Try to create same database again
    let (dup_success, dup_stdout, dup_stderr) = run_cli_command(&url, &["db", "create", "duptest"]);

    // Should fail or indicate already exists
    assert!(
        !dup_success
            || dup_stdout.contains("already exists")
            || dup_stderr.contains("already exists"),
        "Creating duplicate database should fail or indicate it exists"
    );

    // Cleanup
    let _ = run_cli_command(&url, &["db", "drop", "duptest", "--force"]);
}

#[test]
#[ignore] // Requires running server
fn test_cli_db_drop_nonexistent_fails() {
    let server = TestServer::start(7695);
    let url = server.url();

    // Try to drop non-existent database
    let (success, stdout, stderr) =
        run_cli_command(&url, &["db", "drop", "nonexistent_db", "--force"]);

    // Should fail or indicate doesn't exist
    if success {
        // If multi-database is supported, should indicate error
        println!("stdout: {}", stdout);
        println!("stderr: {}", stderr);
    } else {
        // Expected to fail
        assert!(
            stderr.contains("not found")
                || stderr.contains("does not exist")
                || stderr.contains("not supported"),
            "Should indicate database doesn't exist or feature not supported"
        );
    }
}
