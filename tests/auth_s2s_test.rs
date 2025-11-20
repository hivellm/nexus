//! End-to-end (S2S) tests for Authentication & User Management via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Usage:
//!   cargo test --features s2s --test auth_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test auth_s2s_test

#![cfg(feature = "s2s")]

use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
struct CreateUserRequest {
    username: String,
    password: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UserResponse {
    id: String,
    username: String,
    email: Option<String>,
    roles: Vec<String>,
    permissions: Vec<String>,
    is_active: bool,
    is_root: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct UsersResponse {
    users: Vec<UserResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct UpdatePermissionsRequest {
    permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PermissionsResponse {
    username: String,
    permissions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Get server URL from environment or use default
fn get_server_url() -> String {
    std::env::var("NEXUS_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:15474".to_string())
}

/// Check if server is available
async fn check_server_available(url: &str) -> bool {
    let client = reqwest::Client::new();
    client
        .get(&format!("{}/health", url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tokio::test]
async fn test_auth_s2s() {
    let server_url = get_server_url();

    // Check if server is available
    if !check_server_available(&server_url).await {
        eprintln!("ERROR: Server not available at {}", server_url);
        eprintln!("Please start the server first: cargo run --release --bin nexus-server");
        std::process::exit(1);
    }

    println!("Server is available at {}", server_url);
    println!("==========================================");
    println!("Authentication & User Management S2S Tests");
    println!("==========================================");
    println!();

    let client = reqwest::Client::new();
    let mut passed = 0;
    let mut failed = 0;

    // Test User CRUD Operations via REST API
    println!("--- User CRUD Operations (REST API) Tests ---");

    // POST /auth/users - Create user
    let create_request = CreateUserRequest {
        username: "testuser_s2s_rest".to_string(),
        password: Some("testpass123".to_string()),
        email: Some("testuser@example.com".to_string()),
    };

    match client
        .post(&format!("{}/auth/users", server_url))
        .json(&create_request)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(user) = response.json::<UserResponse>().await {
                    assert_eq!(user.username, "testuser_s2s_rest");
                    assert_eq!(user.email, Some("testuser@example.com".to_string()));
                    assert!(user.is_active);
                    println!("✓ POST /auth/users: PASSED");
                    passed += 1;
                } else {
                    println!("✗ POST /auth/users: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                println!("✗ POST /auth/users: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ POST /auth/users: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // GET /auth/users - List users
    match client.get(&format!("{}/auth/users", server_url)).send().await {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(users_response) = response.json::<UsersResponse>().await {
                    assert!(users_response.users.len() > 0);
                    assert!(users_response
                        .users
                        .iter()
                        .any(|u| u.username == "testuser_s2s_rest"));
                    println!("✓ GET /auth/users: PASSED");
                    passed += 1;
                } else {
                    println!("✗ GET /auth/users: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                println!("✗ GET /auth/users: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ GET /auth/users: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // GET /auth/users/{username} - Get specific user
    match client
        .get(&format!("{}/auth/users/testuser_s2s_rest", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(user) = response.json::<UserResponse>().await {
                    assert_eq!(user.username, "testuser_s2s_rest");
                    println!("✓ GET /auth/users/{{username}}: PASSED");
                    passed += 1;
                } else {
                    println!("✗ GET /auth/users/{{username}}: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                println!("✗ GET /auth/users/{{username}}: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ GET /auth/users/{{username}}: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test Permission Management via REST API
    println!();
    println!("--- Permission Management (REST API) Tests ---");

    // POST /auth/users/{username}/permissions - Grant permissions
    let grant_request = UpdatePermissionsRequest {
        permissions: vec!["READ".to_string(), "WRITE".to_string()],
    };

    match client
        .post(&format!("{}/auth/users/testuser_s2s_rest/permissions", server_url))
        .json(&grant_request)
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ POST /auth/users/{{username}}/permissions: PASSED");
                passed += 1;
            } else {
                println!(
                    "✗ POST /auth/users/{{username}}/permissions: FAILED - Status: {}",
                    response.status()
                );
                failed += 1;
            }
        }
        Err(e) => {
            println!(
                "✗ POST /auth/users/{{username}}/permissions: FAILED - Request error: {}",
                e
            );
            failed += 1;
        }
    }

    // GET /auth/users/{username}/permissions - Get user permissions
    match client
        .get(&format!("{}/auth/users/testuser_s2s_rest/permissions", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(perms) = response.json::<PermissionsResponse>().await {
                    assert!(perms.permissions.contains(&"READ".to_string()));
                    assert!(perms.permissions.contains(&"WRITE".to_string()));
                    println!("✓ GET /auth/users/{{username}}/permissions: PASSED");
                    passed += 1;
                } else {
                    println!("✗ GET /auth/users/{{username}}/permissions: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                println!(
                    "✗ GET /auth/users/{{username}}/permissions: FAILED - Status: {}",
                    response.status()
                );
                failed += 1;
            }
        }
        Err(e) => {
            println!(
                "✗ GET /auth/users/{{username}}/permissions: FAILED - Request error: {}",
                e
            );
            failed += 1;
        }
    }

    // DELETE /auth/users/{username}/permissions/{permission} - Revoke permission
    match client
        .delete(&format!(
            "{}/auth/users/testuser_s2s_rest/permissions/READ",
            server_url
        ))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ DELETE /auth/users/{{username}}/permissions/{{permission}}: PASSED");
                passed += 1;
            } else {
                println!(
                    "✗ DELETE /auth/users/{{username}}/permissions/{{permission}}: FAILED - Status: {}",
                    response.status()
                );
                failed += 1;
            }
        }
        Err(e) => {
            println!(
                "✗ DELETE /auth/users/{{username}}/permissions/{{permission}}: FAILED - Request error: {}",
                e
            );
            failed += 1;
        }
    }

    // DELETE /auth/users/{username} - Delete user
    match client
        .delete(&format!("{}/auth/users/testuser_s2s_rest", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                println!("✓ DELETE /auth/users/{{username}}: PASSED");
                passed += 1;
            } else {
                println!("✗ DELETE /auth/users/{{username}}: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ DELETE /auth/users/{{username}}: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test REST Endpoint Protection
    println!();
    println!("--- REST Endpoint Protection Tests ---");

    // Test protected endpoint without authentication
    match client
        .post(&format!("{}/cypher", server_url))
        .json(&json!({
            "query": "MATCH (n) RETURN n LIMIT 1"
        }))
        .send()
        .await
    {
        Ok(response) => {
            if response.status() == 401 {
                println!("✓ Protected endpoint returns 401 without auth: PASSED");
                passed += 1;
            } else {
                println!(
                    "✗ Protected endpoint without auth: FAILED - Expected 401, got {}",
                    response.status()
                );
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ Protected endpoint without auth: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test protected endpoint with invalid API key
    match client
        .post(&format!("{}/cypher", server_url))
        .header("Authorization", "Bearer nx_invalid_key_12345")
        .json(&json!({
            "query": "MATCH (n) RETURN n LIMIT 1"
        }))
        .send()
        .await
    {
        Ok(response) => {
            if response.status() == 401 {
                println!("✓ Protected endpoint returns 401 with invalid key: PASSED");
                passed += 1;
            } else {
                println!(
                    "✗ Protected endpoint with invalid key: FAILED - Expected 401, got {}",
                    response.status()
                );
                failed += 1;
            }
        }
        Err(e) => {
            println!("✗ Protected endpoint with invalid key: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test rate limiting headers (if rate limiting is enabled)
    // This test assumes a valid API key exists
    // In a real scenario, you would create an API key first and use it
    println!();
    println!("--- Rate Limiting Headers Tests ---");
    println!("Note: Rate limiting headers test requires a valid API key");
    println!("This test is skipped if no valid key is available");

    // Summary
    println!();
    println!("==========================================");
    println!("Test Summary");
    println!("==========================================");
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);
    println!("Total:  {}", passed + failed);
    println!();

    if failed > 0 {
        eprintln!("Some tests failed!");
        std::process::exit(1);
    }
}

