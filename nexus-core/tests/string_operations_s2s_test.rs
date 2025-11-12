//! End-to-end (S2S) tests for Cypher string operations via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Usage:
//!   cargo test --features s2s --test string_operations_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test string_operations_s2s_test

#![cfg(feature = "s2s")]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct CypherRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CypherResponse {
    columns: Vec<String>,
    rows: Vec<serde_json::Value>,
    execution_time_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Get server URL from environment or use default
fn get_server_url() -> String {
    std::env::var("NEXUS_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:15474".to_string())
}

/// Check if server is available
async fn check_server_available(url: &str) -> bool {
    let client = reqwest::Client::new();
    client
        .get(format!("{}/health", url))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Wait for server to be available
async fn wait_for_server(url: &str, max_attempts: u32) -> bool {
    for i in 1..=max_attempts {
        if check_server_available(url).await {
            return true;
        }
        if i < max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
    false
}

/// Execute a Cypher query via HTTP API
async fn execute_query(
    client: &reqwest::Client,
    url: &str,
    query: &str,
) -> Result<CypherResponse, reqwest::Error> {
    let request = CypherRequest {
        query: query.to_string(),
        params: None,
    };

    let response = client
        .post(format!("{}/cypher", url))
        .json(&request)
        .send()
        .await?;

    response.json().await
}

/// Test helper that checks row count
async fn test_query_count(
    client: &reqwest::Client,
    url: &str,
    test_name: &str,
    query: &str,
    expected_count: usize,
) -> bool {
    match execute_query(client, url, query).await {
        Ok(response) => {
            if let Some(error) = response.error {
                println!("✗ {}: FAILED - Error: {}", test_name, error);
                false
            } else {
                let actual_count = response.rows.len();
                if actual_count == expected_count {
                    println!("✓ {}: PASSED", test_name);
                    true
                } else {
                    println!(
                        "✗ {}: FAILED - Expected {} rows, got {}",
                        test_name, expected_count, actual_count
                    );
                    false
                }
            }
        }
        Err(e) => {
            println!("✗ {}: FAILED - Request error: {}", test_name, e);
            false
        }
    }
}

#[tokio::test]
async fn test_string_operations_s2s() {
    let server_url = get_server_url();

    // Wait for server to be available
    println!("Waiting for server at {}...", server_url);
    if !wait_for_server(&server_url, 30).await {
        eprintln!("ERROR: Server not available at {}", server_url);
        eprintln!("Please start the server first: cargo run --release --bin nexus-server");
        std::process::exit(1);
    }
    println!("✅ Server is ready");
    println!();

    let client = reqwest::Client::new();
    let mut passed = 0;
    let mut failed = 0;

    // Setup test data
    println!("=== Setting up test data ===");
    let setup_query = "CREATE (n1:Person {name: 'Alice Smith', email: 'alice@example.com', bio: 'Software engineer'}),
                       (n2:Person {name: 'Bob Johnson', email: 'bob@other.com', bio: 'Marketing specialist'}),
                       (n3:Person {name: 'Charlie Brown', email: 'charlie@example.com', phone: '123-456-7890'})
                       RETURN count(n1) AS total";
    if execute_query(&client, &server_url, setup_query)
        .await
        .is_ok()
    {
        println!("✅ Test data created");
    } else {
        println!("⚠️  Failed to create test data (may already exist)");
    }
    println!();

    // STARTS WITH tests
    println!("=== Testing STARTS WITH ===");
    if test_query_count(
        &client,
        &server_url,
        "STARTS WITH: Basic match",
        "MATCH (n:Person) WHERE n.name STARTS WITH 'Alice' RETURN n.name AS name",
        1,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "STARTS WITH: No match",
        "MATCH (n:Person) WHERE n.name STARTS WITH 'Zebra' RETURN n.name AS name",
        0,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "STARTS WITH: Case sensitive",
        "MATCH (n:Person) WHERE n.name STARTS WITH 'alice' RETURN n.name AS name",
        0,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // ENDS WITH tests
    println!("=== Testing ENDS WITH ===");
    if test_query_count(
        &client,
        &server_url,
        "ENDS WITH: Basic match",
        "MATCH (n:Person) WHERE n.email ENDS WITH '@example.com' RETURN n.email AS email",
        2,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "ENDS WITH: No match",
        "MATCH (n:Person) WHERE n.email ENDS WITH '@nonexistent.com' RETURN n.email AS email",
        0,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // CONTAINS tests
    println!("=== Testing CONTAINS ===");
    if test_query_count(
        &client,
        &server_url,
        "CONTAINS: Basic match",
        "MATCH (n:Person) WHERE n.bio CONTAINS 'engineer' RETURN n.bio AS bio",
        1,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "CONTAINS: No match",
        "MATCH (n:Person) WHERE n.bio CONTAINS 'doctor' RETURN n.bio AS bio",
        0,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "CONTAINS: Email contains @",
        "MATCH (n:Person) WHERE n.email CONTAINS '@' RETURN n.email AS email",
        3,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // Regex tests
    println!("=== Testing Regex (=~) ===");
    if test_query_count(
        &client,
        &server_url,
        "Regex: Email pattern",
        "MATCH (n:Person) WHERE n.email =~ '.*@example\\\\.com' RETURN n.email AS email",
        2,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "Regex: Phone pattern",
        "MATCH (n:Person) WHERE n.phone =~ '\\\\d{3}-\\\\d{3}-\\\\d{4}' RETURN n.phone AS phone",
        1,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "Regex: No match",
        "MATCH (n:Person) WHERE n.email =~ '.*@nonexistent\\\\.com' RETURN n.email AS email",
        0,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // Combined operators tests
    println!("=== Testing Combined Operators ===");
    if test_query_count(
        &client,
        &server_url,
        "Combined: STARTS WITH AND ENDS WITH",
        "MATCH (n:Person) WHERE n.email STARTS WITH 'alice' AND n.email ENDS WITH '.com' RETURN n.email AS email",
        1,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_count(
        &client,
        &server_url,
        "Combined: Multiple conditions",
        "MATCH (n:Person) WHERE n.email STARTS WITH 'alice' AND n.email ENDS WITH '.com' AND n.email CONTAINS '@' RETURN n.email AS email",
        1,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // Summary
    println!("==========================================");
    println!("Test Summary");
    println!("==========================================");
    println!("Total Tests: {}", passed + failed);
    println!("Passed: {}", passed);
    if failed > 0 {
        println!("Failed: {}", failed);
    } else {
        println!("Failed: 0");
    }
    println!();

    if failed == 0 {
        println!("✅ ALL TESTS PASSED!");
    } else {
        println!("❌ SOME TESTS FAILED");
        std::process::exit(1);
    }
}
