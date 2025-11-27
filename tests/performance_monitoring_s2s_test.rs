//! End-to-end (S2S) tests for Performance Monitoring via HTTP API
//!
//! These tests require the server to be running and are only executed when
//! the `s2s` feature is enabled.
//!
//! Tests:
//! - Query statistics collection
//! - Slow query logging
//! - Slow query analysis
//! - Plan cache statistics
//! - Memory and cache metrics tracking
//!
//! Usage:
//!   cargo test --features s2s --test performance_monitoring_s2s_test
//!
//! Or set NEXUS_SERVER_URL environment variable to specify server URL:
//!   NEXUS_SERVER_URL=http://localhost:15474 cargo test --features s2s --test performance_monitoring_s2s_test

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

#[derive(Debug, Serialize, Deserialize)]
struct QueryStatisticsResponse {
    statistics: QueryStatisticsSummary,
    patterns: Vec<QueryPatternStatsResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryStatisticsSummary {
    total_queries: u64,
    successful_queries: u64,
    failed_queries: u64,
    total_execution_time_ms: u64,
    average_execution_time_ms: u64,
    min_execution_time_ms: u64,
    max_execution_time_ms: u64,
    slow_query_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct QueryPatternStatsResponse {
    pattern: String,
    count: u64,
    avg_time_ms: f64,
    min_time_ms: u64,
    max_time_ms: u64,
    success_count: u64,
    failure_count: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlowQueriesResponse {
    queries: Vec<SlowQueryRecord>,
    count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlowQueryRecord {
    query: String,
    execution_time_ms: u64,
    timestamp: u64,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    rows_returned: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlowQueryAnalysisResponse {
    analyses: Vec<SlowQueryAnalysisItem>,
    total_patterns: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct SlowQueryAnalysisItem {
    pattern: String,
    occurrences: usize,
    avg_execution_time_ms: f64,
    total_execution_time_ms: u64,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PlanCacheStatisticsResponse {
    cached_plans: usize,
    max_size: usize,
    current_memory_bytes: usize,
    max_memory_bytes: usize,
    hit_rate: f64,
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
            .get(format!("{}/health", url))
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
) -> Result<CypherResponse, Box<dyn std::error::Error>> {
    let request = CypherRequest {
        query: query.to_string(),
        params: None,
    };

    let response = client
        .post(&format!("{}/cypher", url))
        .json(&request)
        .send()
        .await?;

    if response.status().is_success() {
        let cypher_response: CypherResponse = response.json().await?;
        Ok(cypher_response)
    } else {
        let error_text = response.text().await?;
        Err(format!("Query failed: {}", error_text).into())
    }
}

#[tokio::test]
async fn test_performance_monitoring_s2s() {
    let server_url = get_server_url();

    // Wait for server to be available
    if !wait_for_server(&server_url, 10).await {
        tracing::info!("ERROR: Server not available at {}", server_url);
        tracing::info!("Please start the server first: cargo run --release --bin nexus-server");
        std::process::exit(1);
    }

    tracing::info!("Server is available at {}", server_url);
    tracing::info!("==========================================");
    tracing::info!("Performance Monitoring S2S Tests");
    tracing::info!("==========================================");
    tracing::info!("");

    let client = reqwest::Client::new();
    let mut passed = 0;
    let mut failed = 0;

    // Test 1: Get initial query statistics
    tracing::info!("--- Test 1: Query Statistics Endpoint ---");
    match client
        .get(&format!("{}/performance/statistics", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(stats) = response.json::<QueryStatisticsResponse>().await {
                    tracing::info!("GET /performance/statistics: PASSED");
                    tracing::info!("  Total queries: {}", stats.statistics.total_queries);
                    tracing::info!("  Average time: {}ms", stats.statistics.average_execution_time_ms);
                    passed += 1;
                } else {
                    tracing::info!("GET /performance/statistics: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("GET /performance/statistics: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("GET /performance/statistics: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 2: Execute queries and verify statistics are collected
    tracing::info!("\n--- Test 2: Query Execution and Statistics Collection ---");
    let queries = vec![
        "MATCH (n) RETURN n LIMIT 10",
        "CREATE (n:Test {name: 'test1'}) RETURN n",
        "MATCH (n:Test) RETURN n",
    ];

    for query in &queries {
        match execute_query(&client, &server_url, query).await {
            Ok(_) => {
                tracing::info!("Query executed: {}", query);
            }
            Err(e) => {
                tracing::info!("Query failed: {} - Error: {}", query, e);
                failed += 1;
            }
        }
        // Small delay to ensure statistics are updated
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    // Verify statistics were updated
    match client
        .get(&format!("{}/performance/statistics", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(stats) = response.json::<QueryStatisticsResponse>().await {
                    if stats.statistics.total_queries >= queries.len() as u64 {
                        tracing::info!("Statistics collection: PASSED");
                        tracing::info!("  Total queries recorded: {}", stats.statistics.total_queries);
                        passed += 1;
                    } else {
                        tracing::info!("Statistics collection: FAILED - Expected at least {} queries, got {}", queries.len(), stats.statistics.total_queries);
                        failed += 1;
                    }
                } else {
                    tracing::info!("Statistics collection: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("Statistics collection: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("Statistics collection: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 3: Slow query logging
    tracing::info!("\n--- Test 3: Slow Query Logging ---");
    
    // Execute a slow query (simulated by a complex query)
    let slow_query = "MATCH (a)-[*1..3]-(b) RETURN a, b LIMIT 100";
    match execute_query(&client, &server_url, slow_query).await {
        Ok(_) => {
            tracing::info!("Slow query executed");
        }
        Err(_) => {
            // Query might fail, but that's ok for testing
            tracing::info!("  Slow query execution completed (may have failed)");
        }
    }

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    match client
        .get(&format!("{}/performance/slow-queries", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(slow_queries) = response.json::<SlowQueriesResponse>().await {
                    tracing::info!("GET /performance/slow-queries: PASSED");
                    tracing::info!("  Slow queries logged: {}", slow_queries.count);
                    passed += 1;
                } else {
                    tracing::info!("GET /performance/slow-queries: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("GET /performance/slow-queries: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("GET /performance/slow-queries: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 4: Slow query analysis
    tracing::info!("\n--- Test 4: Slow Query Analysis ---");
    match client
        .get(&format!("{}/performance/slow-queries/analysis", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(analysis) = response.json::<SlowQueryAnalysisResponse>().await {
                    tracing::info!("GET /performance/slow-queries/analysis: PASSED");
                    tracing::info!("  Patterns analyzed: {}", analysis.total_patterns);
                    for item in &analysis.analyses {
                        tracing::info!("  Pattern: {} ({} occurrences)", item.pattern, item.occurrences);
                        tracing::info!("    Avg time: {:.2}ms", item.avg_execution_time_ms);
                        tracing::info!("    Recommendations: {}", item.recommendations.len());
                    }
                    passed += 1;
                } else {
                    tracing::info!("GET /performance/slow-queries/analysis: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("GET /performance/slow-queries/analysis: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("GET /performance/slow-queries/analysis: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 5: Plan cache statistics
    tracing::info!("\n--- Test 5: Plan Cache Statistics ---");
    match client
        .get(&format!("{}/performance/plan-cache", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(cache_stats) = response.json::<PlanCacheStatisticsResponse>().await {
                    tracing::info!("GET /performance/plan-cache: PASSED");
                    tracing::info!("  Cached plans: {}", cache_stats.cached_plans);
                    tracing::info!("  Hit rate: {:.2}%", cache_stats.hit_rate * 100.0);
                    tracing::info!("  Memory usage: {} bytes", cache_stats.current_memory_bytes);
                    passed += 1;
                } else {
                    tracing::info!("GET /performance/plan-cache: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("GET /performance/plan-cache: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("GET /performance/plan-cache: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 6: Clear plan cache
    tracing::info!("\n--- Test 6: Clear Plan Cache ---");
    match client
        .post(&format!("{}/performance/plan-cache/clear", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                tracing::info!("POST /performance/plan-cache/clear: PASSED");
                passed += 1;
            } else {
                tracing::info!("POST /performance/plan-cache/clear: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("POST /performance/plan-cache/clear: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Verify cache was cleared
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    match client
        .get(&format!("{}/performance/plan-cache", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(cache_stats) = response.json::<PlanCacheStatisticsResponse>().await {
                    if cache_stats.cached_plans == 0 {
                        tracing::info!("Plan cache cleared: PASSED");
                        passed += 1;
                    } else {
                        tracing::info!("Plan cache cleared: FAILED - Expected 0 plans, got {}", cache_stats.cached_plans);
                        failed += 1;
                    }
                } else {
                    tracing::info!("Plan cache cleared: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("Plan cache cleared: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("Plan cache cleared: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 7: Verify metrics are collected during query execution
    tracing::info!("\n--- Test 7: Automatic Metrics Collection ---");
    
    // Get initial statistics
    let initial_total = match client
        .get(&format!("{}/performance/statistics", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                response.json::<QueryStatisticsResponse>().await
                    .map(|s| s.statistics.total_queries)
                    .unwrap_or(0)
            } else {
                0
            }
        }
        Err(_) => 0,
    };

    // Execute multiple queries
    for i in 0..5 {
        let query = format!("MATCH (n) RETURN n LIMIT {}", i + 1);
        let _ = execute_query(&client, &server_url, &query).await;
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }

    // Verify statistics increased
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    match client
        .get(&format!("{}/performance/statistics", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(stats) = response.json::<QueryStatisticsResponse>().await {
                    if stats.statistics.total_queries > initial_total {
                        tracing::info!("Automatic metrics collection: PASSED");
                        tracing::info!("  Queries before: {}, Queries after: {}", initial_total, stats.statistics.total_queries);
                        passed += 1;
                    } else {
                        tracing::info!("Automatic metrics collection: FAILED - Statistics did not increase");
                        failed += 1;
                    }
                } else {
                    tracing::info!("Automatic metrics collection: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("Automatic metrics collection: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("Automatic metrics collection: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Test 8: Pattern statistics
    tracing::info!("\n--- Test 8: Query Pattern Statistics ---");
    match client
        .get(&format!("{}/performance/statistics", server_url))
        .send()
        .await
    {
        Ok(response) => {
            if response.status().is_success() {
                if let Ok(stats) = response.json::<QueryStatisticsResponse>().await {
                    tracing::info!("Query pattern statistics: PASSED");
                    tracing::info!("  Patterns tracked: {}", stats.patterns.len());
                    for pattern in &stats.patterns {
                        tracing::info!("  Pattern: {} (count: {}, avg: {:.2}ms)", 
                            pattern.pattern, pattern.count, pattern.avg_time_ms);
                    }
                    passed += 1;
                } else {
                    tracing::info!("Query pattern statistics: FAILED - Invalid response format");
                    failed += 1;
                }
            } else {
                tracing::info!("Query pattern statistics: FAILED - Status: {}", response.status());
                failed += 1;
            }
        }
        Err(e) => {
            tracing::info!("Query pattern statistics: FAILED - Request error: {}", e);
            failed += 1;
        }
    }

    // Summary
    tracing::info!("\n==========================================");
    tracing::info!("Test Summary");
    tracing::info!("==========================================");
    tracing::info!("Passed: {}", passed);
    tracing::info!("Failed: {}", failed);
    tracing::info!("Total:  {}", passed + failed);
    tracing::info!("");

    if failed > 0 {
        tracing::info!("Some tests failed!");
        std::process::exit(1);
    } else {
        tracing::info!("All tests passed!");
    }
}

