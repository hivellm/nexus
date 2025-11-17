//! End-to-end (S2S) tests for Advanced Cypher Features via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Tests: CASE, FOREACH, EXISTS, Map Projections, List Comprehensions, Pattern Comprehensions
//!
//! Usage:
//!   cargo test --features s2s --test advanced_features_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test advanced_features_s2s_test

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
                println!("✅ {}: PASSED", test_name);
                true
            } else {
                println!("❌ {}: FAILED - Error: {:?}", test_name, response.error);
                false
            }
        }
        Err(e) => {
            println!("❌ {}: FAILED - Request error: {}", test_name, e);
            false
        }
    }
}

#[tokio::test]
async fn test_advanced_features_s2s() {
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
    let setup_query = r#"
CREATE 
  (alice:Person {name: 'Alice', age: 30, scores: [85, 90, 78, 92], city: 'New York'}),
  (bob:Person {name: 'Bob', age: 25, scores: [70, 75, 80], city: 'Boston'}),
  (charlie:Person {name: 'Charlie', age: 35, scores: [95, 88, 91], city: 'New York'}),
  (diana:Person {name: 'Diana', age: 28, scores: [82, 87, 90], city: 'Boston'}),
  (alice)-[:KNOWS {since: 2020}]->(bob),
  (alice)-[:KNOWS {since: 2018}]->(charlie),
  (bob)-[:KNOWS {since: 2021}]->(diana),
  (charlie)-[:KNOWS {since: 2019}]->(diana),
  (alice)-[:WORKS_AT {role: 'Engineer'}]->(:Company {name: 'TechCorp'}),
  (bob)-[:WORKS_AT {role: 'Manager'}]->(:Company {name: 'TechCorp'}),
  (charlie)-[:WORKS_AT {role: 'Director'}]->(:Company {name: 'BigCorp'})
"#;

    if test_query_success(&client, &server_url, "Setup: Create complex graph", setup_query).await {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // CASE Expressions Tests
    println!("=== CASE Expressions - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "CASE: Nested CASE expressions",
        r#"
MATCH (p:Person)
RETURN 
  p.name,
  CASE 
    WHEN p.age < 25 THEN 'Junior'
    WHEN p.age < 30 THEN 
      CASE 
        WHEN p.city = 'New York' THEN 'Mid-Level NYC'
        ELSE 'Mid-Level'
      END
    WHEN p.age < 35 THEN 'Senior'
    ELSE 'Executive'
  END AS category
LIMIT 5
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "CASE: Generic CASE with property matching",
        r#"
MATCH (p:Person)
RETURN 
  p.name,
  CASE p.city
    WHEN 'New York' THEN 'NYC'
    WHEN 'Boston' THEN 'BOS'
    ELSE 'Other'
  END AS city_code
LIMIT 5
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // FOREACH Tests
    println!("=== FOREACH - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "FOREACH: SET property for multiple items",
        r#"
MATCH (p:Person)
FOREACH (x IN [1, 2, 3] |
  SET p.processed = true
)
RETURN COUNT(p) AS processed_count
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // EXISTS Tests
    println!("=== EXISTS - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "EXISTS: Pattern with WHERE clause",
        r#"
MATCH (p:Person)
WHERE EXISTS {
  (p)-[:KNOWS]->(friend:Person)
  WHERE friend.age > p.age
}
RETURN p.name, p.age
LIMIT 5
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }

    if test_query_success(
        &client,
        &server_url,
        "EXISTS: Pattern with node property conditions",
        r#"
MATCH (p:Person)
WHERE EXISTS {
  (p)-[:WORKS_AT]->(c:Company)
  WHERE c.name = 'TechCorp'
}
RETURN p.name, p.age
LIMIT 5
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // Map Projections Tests
    println!("=== Map Projections - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "Map Projection: Virtual keys with CASE",
        r#"
MATCH (p:Person)
RETURN p {
  .name,
  .age,
  isSenior: CASE WHEN p.age >= 30 THEN true ELSE false END
} AS person_info
LIMIT 3
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // List Comprehensions Tests
    println!("=== List Comprehensions - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "List Comprehension: Filter and transform",
        r#"
MATCH (p:Person)
RETURN p.name, [score IN p.scores WHERE score > 80 | score * 1.1] AS high_scores
LIMIT 3
"#,
    )
    .await
    {
        passed += 1;
    } else {
        failed += 1;
    }
    println!();

    // Pattern Comprehensions Tests
    println!("=== Pattern Comprehensions - Complex Scenarios ===");
    if test_query_success(
        &client,
        &server_url,
        "Pattern Comprehension: Extract relationships",
        r#"
MATCH (p:Person)
RETURN p.name, [(p)-[r:KNOWS]->(friend:Person) | friend.name] AS friends
LIMIT 3
"#,
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
    println!("Passed: {}", passed);
    println!("Failed: {}", failed);
    println!("Total: {}", passed + failed);
    println!();

    if failed == 0 {
        println!("✅ ALL TESTS PASSED!");
    } else {
        println!("❌ SOME TESTS FAILED");
        std::process::exit(1);
    }
}

