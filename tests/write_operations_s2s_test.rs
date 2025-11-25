//! End-to-end (S2S) tests for Cypher write operations via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Usage:
//!   cargo test --features s2s --test write_operations_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test write_operations_s2s_test

#![cfg(feature = "s2s")]

use serde::{Deserialize, Serialize};
use tracing;

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

/// Wait for server to be available
async fn wait_for_server(url: &str, max_attempts: u32) -> bool {
    let client = reqwest::Client::new();
    for i in 1..=max_attempts {
        if client
            .get(&format!("{}/health", url))
            .send()
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            return true;
        }
        if i < max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
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
        .post(&format!("{}/cypher", url))
        .json(&request)
        .send()
        .await?;

    response.json().await
}

/// Test helper that expects success
async fn test_query_success(
    client: &reqwest::Client,
    url: &str,
    test_name: &str,
    query: &str,
) -> bool {
    match execute_query(client, url, query).await {
        Ok(response) => {
            if response.error.is_none() || response.error.as_ref().unwrap().is_empty() {
                tracing::info!("{}: PASSED", test_name);
                true
            } else {
                tracing::info!("{}: FAILED - Error: {:?}", test_name, response.error);
                false
            }
        }
        Err(e) => {
            tracing::info!("{}: FAILED - Request error: {}", test_name, e);
            false
        }
    }
}

#[tokio::test]
async fn test_write_operations_s2s() {
    let server_url = get_server_url();

    // Wait for server to be available
    tracing::info!("Aguardando servidor iniciar...");
    if !wait_for_server(&server_url, 10).await {
        tracing::info!("Servidor não iniciou após 10 tentativas");
        tracing::info!("Please start the server first: cargo run --release --bin nexus-server");
        std::process::exit(1);
    }
    tracing::info!("Servidor está pronto!");
    tracing::info!("");

    let client = reqwest::Client::new();
    let mut passed = 0;
    let mut failed = 0;

    // Clean database first
    tracing::info!("🧹 Limpando banco de dados...");
    if test_query_success(&client, &server_url, "Clean database", "MATCH (n) DETACH DELETE n").await {
        passed += 1;
    } else {
        failed += 1;
    }
    tracing::info!("");

    // Test 1: MERGE creates node when missing
    if test_query_success(
        &client,
        &server_url,
        "MERGE creates node with ON CREATE",
        "MERGE (n:Person {email: 'alice@example.com'}) ON CREATE SET n.created = true RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 2: MERGE reuses existing node
    if test_query_success(
        &client,
        &server_url,
        "MERGE matches existing node with ON MATCH",
        "MERGE (n:Person {email: 'alice@example.com'}) ON MATCH SET n.last_seen = '2025-11-11' RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 3: Verify no duplicates
    if test_query_success(
        &client,
        &server_url,
        "Verify no duplicate nodes",
        "MATCH (n:Person) RETURN count(n) AS total",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 4: SET updates properties
    if test_query_success(
        &client,
        &server_url,
        "SET updates node properties",
        "MATCH (n:Person {email: 'alice@example.com'}) SET n.age = 31, n.city = 'NYC' RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 5: SET adds label
    if test_query_success(
        &client,
        &server_url,
        "SET adds label to node",
        "MATCH (n:Person {email: 'alice@example.com'}) SET n:Employee RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 6: REMOVE property
    if test_query_success(
        &client,
        &server_url,
        "REMOVE property",
        "MATCH (n:Person {email: 'alice@example.com'}) REMOVE n.age RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 7: REMOVE label
    if test_query_success(
        &client,
        &server_url,
        "REMOVE label",
        "MATCH (n:Person {email: 'alice@example.com'}) REMOVE n:Employee RETURN n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 8: DELETE node
    if test_query_success(
        &client,
        &server_url,
        "DELETE node",
        "MATCH (n:Person {email: 'alice@example.com'}) DELETE n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 9: Verify deletion
    if test_query_success(
        &client,
        &server_url,
        "Verify deletion",
        "MATCH (n:Person {email: 'alice@example.com'}) RETURN count(n) AS total",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    // Test 10: DETACH DELETE with relationships
    // First create node with relationship
    test_query_success(
        &client,
        &server_url,
        "Setup: Create node with relationship",
        "CREATE (a:Person {email: 'bob@example.com'})-[:KNOWS]->(b:Person {email: 'charlie@example.com'})",
    )
    .await;

    if test_query_success(
        &client,
        &server_url,
        "DETACH DELETE node with relationships",
        "MATCH (n:Person {email: 'bob@example.com'}) DETACH DELETE n",
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    tracing::info!("");
    tracing::info!("==========================================");
    tracing::info!("Test Summary");
    tracing::info!("==========================================");
    tracing::info!("Passed: {}", passed);
    tracing::info!("Failed: {}", failed);
    tracing::info!("Total: {}", passed + failed);
    tracing::info!("");

    if failed == 0 {
        tracing::info!("ALL TESTS PASSED!");
    } else {
        tracing::info!("SOME TESTS FAILED");
        std::process::exit(1);
    }
}

