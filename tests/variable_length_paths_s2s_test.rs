//! End-to-end (S2S) tests for Variable-Length Paths via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Usage:
//!   cargo test --features s2s --test variable_length_paths_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test variable_length_paths_s2s_test

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
                tracing::info!("{}: PASSED ({} rows)", test_name, response.rows.len());
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

/// Test helper that expects error
async fn test_query_error(
    client: &reqwest::Client,
    url: &str,
    test_name: &str,
    query: &str,
) -> bool {
    match execute_query(client, url, query).await {
        Ok(response) => {
            if response.error.is_some() && !response.error.as_ref().unwrap().is_empty() {
                tracing::info!("{}: PASSED (expected error)", test_name);
                true
            } else {
                tracing::info!("{}: FAILED - Expected error but got success", test_name);
                false
            }
        }
        Err(_) => {
            tracing::info!("{}: PASSED (expected error)", test_name);
            true
        }
    }
}

#[tokio::test]
async fn test_variable_length_paths_zero_or_more() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Find all paths with zero or more hops
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*]->(b) RETURN a.name AS start, b.name AS end, length(r) AS hops";
    test_query_success(&client, &url, "Zero or more hops (*)", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_one_or_more() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Find all paths with one or more hops
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS+]->(b) RETURN a.name AS start, b.name AS end";
    test_query_success(&client, &url, "One or more hops (+)", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_exact_length() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Find paths with exactly 2 hops
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*2]->(b) RETURN a.name AS start, b.name AS end";
    test_query_success(&client, &url, "Exact length (*2)", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_range() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (d:Person {name: 'David'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
        CREATE (c)-[:KNOWS]->(d)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Find paths with 1 to 3 hops
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*1..3]->(b) RETURN a.name AS start, b.name AS end";
    test_query_success(&client, &url, "Range (1..3)", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_zero_or_one() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (a)-[:KNOWS]->(b)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Find paths with zero or one hop
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS?]->(b) RETURN a.name AS start, b.name AS end";
    test_query_success(&client, &url, "Zero or one hop (?)", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_with_relationship_variable() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Return relationship variable in path
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*]->(b) RETURN a.name AS start, b.name AS end, r AS relationships";
    test_query_success(&client, &url, "With relationship variable", query).await;
}

#[tokio::test]
async fn test_variable_length_paths_no_path_found() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create isolated nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: No path exists
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*]->(b:Person {name: 'Bob'}) RETURN a.name AS start, b.name AS end";
    let response = execute_query(&client, &url, query).await.unwrap();
    
    // Should return empty result, not error
    assert!(response.error.is_none() || response.error.as_ref().unwrap().is_empty());
    tracing::info!("No path found: PASSED ({} rows)", response.rows.len());
}

#[tokio::test]
async fn test_variable_length_paths_bidirectional() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create bidirectional relationships
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: Bidirectional path search
    let query = "MATCH (a:Person {name: 'Alice'})-[r:KNOWS*]-(b) RETURN a.name AS start, b.name AS end";
    test_query_success(&client, &url, "Bidirectional paths", query).await;
}

#[tokio::test]
async fn test_shortest_path_function() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create a chain of nodes
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (d:Person {name: 'David'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
        CREATE (a)-[:KNOWS]->(d)
        CREATE (d)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: shortestPath function (using pattern comprehension syntax)
    // Note: This requires pattern comprehension support in shortestPath()
    let query = "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) RETURN shortestPath([(a)-[*]->(c)]) AS path";
    test_query_success(&client, &url, "shortestPath function", query).await;
}

#[tokio::test]
async fn test_all_shortest_paths_function() {
    let url = get_server_url();
    let client = reqwest::Client::new();

    if !wait_for_server(&url, 5).await {
        etracing::info!("Server not available, skipping test");
        return;
    }

    // Setup: Create multiple paths of same length
    let setup_query = r#"
        CREATE (a:Person {name: 'Alice'})
        CREATE (b:Person {name: 'Bob'})
        CREATE (c:Person {name: 'Charlie'})
        CREATE (d:Person {name: 'David'})
        CREATE (a)-[:KNOWS]->(b)
        CREATE (b)-[:KNOWS]->(c)
        CREATE (a)-[:KNOWS]->(d)
        CREATE (d)-[:KNOWS]->(c)
    "#;
    let _ = execute_query(&client, &url, setup_query).await;

    // Test: allShortestPaths function
    let query = "MATCH (a:Person {name: 'Alice'}), (c:Person {name: 'Charlie'}) RETURN allShortestPaths([(a)-[*]->(c)]) AS paths";
    test_query_success(&client, &url, "allShortestPaths function", query).await;
}

