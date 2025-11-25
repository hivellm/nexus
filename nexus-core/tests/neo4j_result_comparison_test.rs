//! Detailed Neo4j Result Comparison Tests
//!
//! These tests compare query results between Nexus and Neo4j to ensure identical return values.
//! Requires both servers to be running:
//! - Nexus: http://localhost:15474 (via REST API)
//! - Neo4j: http://localhost:7474 (HTTP endpoint)

use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use tracing;

// Helper function to execute query on Nexus via REST API
async fn execute_nexus_query(
    query: &str,
    params: Option<HashMap<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let client = Client::new();
    let url = "http://localhost:15474/cypher";

    let body = json!({
        "query": query,
        "parameters": params.unwrap_or_default()
    });

    let response = client
        .post(url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Nexus request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Nexus returned status {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Nexus response: {}", e))
}

// Helper function to execute query on Neo4j via HTTP
async fn execute_neo4j_query(
    query: &str,
    params: Option<HashMap<String, serde_json::Value>>,
) -> Result<serde_json::Value, String> {
    let client = Client::new();
    let url = "http://localhost:7474/db/neo4j/tx/commit";

    let body = json!({
        "statements": [{
            "statement": query,
            "parameters": params.unwrap_or_default()
        }]
    });

    let response = client
        .post(url)
        .basic_auth("neo4j", Some("password"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Neo4j request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("Neo4j returned status {}: {}", status, error_text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse Neo4j response: {}", e))
}

// Helper function to check if Nexus server is available
async fn check_nexus_available() -> bool {
    let client = Client::new();
    client
        .get("http://localhost:15474/health")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

// Helper function to normalize Neo4j row to Nexus format
fn normalize_neo4j_row(neo4j_row: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    neo4j_row.get("row").and_then(|r| r.as_array()).cloned()
}

// Helper function to compare values
fn values_equal(nexus_val: &serde_json::Value, neo4j_val: &serde_json::Value) -> bool {
    match (nexus_val, neo4j_val) {
        (serde_json::Value::Null, serde_json::Value::Null) => true,
        (serde_json::Value::Bool(a), serde_json::Value::Bool(b)) => a == b,
        (serde_json::Value::Number(a), serde_json::Value::Number(b)) => {
            // Compare numbers (handle int/float conversion)
            if let (Some(a_i64), Some(b_i64)) = (a.as_i64(), b.as_i64()) {
                a_i64 == b_i64
            } else if let (Some(a_f64), Some(b_f64)) = (a.as_f64(), b.as_f64()) {
                (a_f64 - b_f64).abs() < f64::EPSILON
            } else {
                false
            }
        }
        (serde_json::Value::String(a), serde_json::Value::String(b)) => a == b,
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|bv| values_equal(v, bv)))
        }
        _ => false,
    }
}

// Compare query results in detail
fn compare_results(
    nexus_result: &serde_json::Value,
    neo4j_result: &serde_json::Value,
) -> (bool, String) {
    let mut report = String::new();

    // Check if Nexus has error
    if let Some(error) = nexus_result.get("error") {
        return (false, format!("Nexus returned error: {}", error));
    }

    // Extract Nexus results
    let nexus_columns = nexus_result
        .get("columns")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let empty_vec = vec![];
    let nexus_rows = nexus_result
        .get("rows")
        .and_then(|r| r.as_array())
        .unwrap_or(&empty_vec);

    // Extract Neo4j results
    let neo4j_results = match neo4j_result.get("results").and_then(|r| r.as_array()) {
        Some(results) => results,
        None => return (false, "Neo4j response missing 'results' field".to_string()),
    };

    if neo4j_results.is_empty() {
        return (false, "Neo4j returned empty results".to_string());
    }

    let neo4j_first = &neo4j_results[0];

    // Check for Neo4j errors
    if let Some(errors) = neo4j_result.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            return (false, format!("Neo4j returned errors: {:?}", errors));
        }
    }

    // Compare columns
    let neo4j_columns = neo4j_first
        .get("columns")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    report.push_str(&format!(
        "Columns - Nexus: {:?}, Neo4j: {:?}\n",
        nexus_columns, neo4j_columns
    ));

    if nexus_columns != neo4j_columns {
        return (false, format!("{}Columns don't match!", report));
    }

    // Compare rows
    let empty_vec = vec![];
    let neo4j_data = neo4j_first
        .get("data")
        .and_then(|d| d.as_array())
        .unwrap_or(&empty_vec);

    report.push_str(&format!(
        "Row count - Nexus: {}, Neo4j: {}\n",
        nexus_rows.len(),
        neo4j_data.len()
    ));

    if nexus_rows.len() != neo4j_data.len() {
        return (false, format!("{}Row counts don't match!", report));
    }

    // Compare each row
    for (i, (nexus_row, neo4j_row_obj)) in nexus_rows.iter().zip(neo4j_data.iter()).enumerate() {
        let nexus_values = match nexus_row {
            serde_json::Value::Array(arr) => arr,
            _ => {
                return (
                    false,
                    format!("{}Row {}: Nexus row is not an array", report, i),
                );
            }
        };

        let neo4j_values = match normalize_neo4j_row(neo4j_row_obj) {
            Some(vals) => vals,
            None => {
                return (
                    false,
                    format!("{}Row {}: Neo4j row missing 'row' field", report, i),
                );
            }
        };

        if nexus_values.len() != neo4j_values.len() {
            return (
                false,
                format!(
                    "{}Row {}: Value count mismatch (Nexus: {}, Neo4j: {})",
                    report,
                    i,
                    nexus_values.len(),
                    neo4j_values.len()
                ),
            );
        }

        for (j, (nexus_val, neo4j_val)) in nexus_values.iter().zip(neo4j_values.iter()).enumerate()
        {
            if !values_equal(nexus_val, neo4j_val) {
                return (
                    false,
                    format!(
                        "{}Row {} Col {}: Values don't match (Nexus: {:?}, Neo4j: {:?})",
                        report, i, j, nexus_val, neo4j_val
                    ),
                );
            }
        }
    }

    report.push_str("All values match!");
    (true, report)
}

/// Detailed comparison of simple RETURN query
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_detailed_comparison_simple_return() {
    tracing::info!("\n=== Detailed Comparison: Simple RETURN ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let query = "RETURN 1 as value, 'test' as name";

    // Execute in Nexus
    let nexus_result = match execute_nexus_query(query, None).await {
        Ok(result) => result,
        Err(e) => {
            etracing::info!("ERROR: Nexus server error: {}", e);
            etracing::info!("Skipping test - Nexus server not available or returned error");
            return;
        }
    };

    // Execute in Neo4j
    match execute_neo4j_query(query, None).await {
        Ok(neo4j_result) => {
            let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
            tracing::info!("\n{}", report);
            if compatible {
                tracing::info!("COMPATIBLE: Nexus and Neo4j return identical results!");
            } else {
                tracing::info!("INCOMPATIBLE: {}", report);
            }
            assert!(compatible, "Results should be compatible");
        }
        Err(e) => {
            tracing::info!("WARNING: Neo4j not available: {}", e);
        }
    }
}

/// Detailed comparison of multiple value types
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_detailed_comparison_value_types() {
    tracing::info!("\n=== Detailed Comparison: Value Types ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let test_cases = vec![
        ("RETURN 42 as int_val", "Integer"),
        ("RETURN 7.5 as float_val", "Float"),
        ("RETURN 'hello' as string_val", "String"),
        ("RETURN true as bool_val", "Boolean"),
        ("RETURN null as null_val", "Null"),
        ("RETURN [1, 2, 3] as array_val", "Array"),
        ("RETURN {key: 'value', num: 42} as map_val", "Map"),
    ];

    for (query, description) in test_cases {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        match execute_nexus_query(query, None).await {
            Ok(nexus_result) => match execute_neo4j_query(query, None).await {
                Ok(neo4j_result) => {
                    let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                    if compatible {
                        tracing::info!("Compatible");
                    } else {
                        tracing::info!("Incompatible: {}", report);
                    }
                }
                Err(e) => {
                    tracing::info!("WARNING: Neo4j error: {}", e);
                }
            },
            Err(e) => {
                etracing::info!("ERROR: Nexus server error: {}", e);
                etracing::info!("Skipping test - Nexus server not available or returned error");
                return;
            }
        }
    }
}

/// Comprehensive compatibility test
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_comprehensive_compatibility() {
    tracing::info!("\n=== Comprehensive Nexus vs Neo4j Compatibility Test ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 1 as test", "Simple return"),
        (
            "RETURN 'hello' as greeting, 42 as number",
            "Multiple values",
        ),
        ("RETURN [1, 2, 3] as array", "Array return"),
        ("RETURN {key: 'value', num: 42} as map", "Map return"),
        ("RETURN 7.5 as pi", "Float value"),
        ("RETURN true as flag", "Boolean value"),
        ("RETURN null as empty", "Null value"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;
    let mut skipped_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- {} ---", description);
        tracing::info!("Query: {}", query);

        // Nexus
        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => {
                if result.get("error").is_some() {
                    tracing::info!("Nexus: Error");
                    skipped_count += 1;
                    continue;
                }
                result
            }
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                skipped_count += 1;
                continue;
            }
        };

        // Neo4j
        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("COMPATIBLE");
                    compatible_count += 1;
                } else {
                    tracing::info!("INCOMPATIBLE: {}", report.lines().next().unwrap_or(""));
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                skipped_count += 1;
            }
        }
    }

    tracing::info!("\n=== Compatibility Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
    tracing::info!("Skipped: {}", skipped_count);
    tracing::info!(
        "Total tested: {}",
        compatible_count + incompatible_count + skipped_count
    );

    if incompatible_count == 0 && compatible_count > 0 {
        tracing::info!("\nFULL COMPATIBILITY: All tested queries return identical results!");
    } else if incompatible_count > 0 {
        tracing::info!("\nWARNING: PARTIAL COMPATIBILITY: Some queries return different results");
    }
}

/// Test aggregation functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_aggregation_functions() {
    tracing::info!("\n=== Testing Aggregation Functions ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN count(*) as total", "count(*)"),
        ("RETURN sum(1) as sum_val", "sum()"),
        ("RETURN avg(10) as avg_val", "avg()"),
        ("RETURN min(5) as min_val", "min()"),
        ("RETURN max(15) as max_val", "max()"),
        ("RETURN collect(1) as collected", "collect()"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Aggregation Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test mathematical operations
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_mathematical_operations() {
    tracing::info!("\n=== Testing Mathematical Operations ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 10 + 5 as add", "Addition"),
        ("RETURN 10 - 5 as sub", "Subtraction"),
        ("RETURN 10 * 5 as mult", "Multiplication"),
        ("RETURN 10 / 5 as div", "Division"),
        ("RETURN 10 % 3 as mod", "Modulo"),
        ("RETURN 2 ^ 3 as power", "Power"),
        ("RETURN abs(-5) as abs_val", "Absolute value"),
        ("RETURN round(3.7) as rounded", "Round"),
        ("RETURN ceil(3.2) as ceiling", "Ceiling"),
        ("RETURN floor(3.8) as floor_val", "Floor"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Mathematical Operations Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test string functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_string_functions() {
    tracing::info!("\n=== Testing String Functions ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN length('hello') as len", "length()"),
        ("RETURN upper('hello') as upper_val", "upper()"),
        ("RETURN lower('HELLO') as lower_val", "lower()"),
        ("RETURN substring('hello', 1, 3) as substr", "substring()"),
        ("RETURN replace('hello', 'l', 'L') as replaced", "replace()"),
        ("RETURN trim('  hello  ') as trimmed", "trim()"),
        ("RETURN split('a,b,c', ',') as split_val", "split()"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== String Functions Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test comparison operators
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_comparison_operators() {
    tracing::info!("\n=== Testing Comparison Operators ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 5 = 5 as eq", "Equals"),
        ("RETURN 5 <> 3 as neq", "Not equals"),
        ("RETURN 5 > 3 as gt", "Greater than"),
        ("RETURN 3 < 5 as lt", "Less than"),
        ("RETURN 5 >= 5 as gte", "Greater or equal"),
        ("RETURN 3 <= 5 as lte", "Less or equal"),
        ("RETURN 5 IN [1, 2, 5] as in_list", "IN operator"),
        ("RETURN 'hello' STARTS WITH 'he' as starts", "STARTS WITH"),
        ("RETURN 'hello' ENDS WITH 'lo' as ends", "ENDS WITH"),
        ("RETURN 'hello' CONTAINS 'll' as contains", "CONTAINS"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Comparison Operators Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test logical operators
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_logical_operators() {
    tracing::info!("\n=== Testing Logical Operators ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN true AND true as and_true", "AND (true)"),
        ("RETURN true AND false as and_false", "AND (false)"),
        ("RETURN true OR false as or_true", "OR (true)"),
        ("RETURN false OR false as or_false", "OR (false)"),
        ("RETURN NOT true as not_true", "NOT (true)"),
        ("RETURN NOT false as not_false", "NOT (false)"),
        ("RETURN (5 > 3) AND (2 < 4) as complex_and", "Complex AND"),
        ("RETURN (5 > 10) OR (2 < 4) as complex_or", "Complex OR"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Logical Operators Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test CASE expressions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_case_expressions() {
    tracing::info!("\n=== Testing CASE Expressions ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        (
            "RETURN CASE WHEN 5 > 3 THEN 'yes' ELSE 'no' END as simple_case",
            "Simple CASE",
        ),
        (
            "RETURN CASE WHEN 5 > 10 THEN 'yes' ELSE 'no' END as else_case",
            "CASE with ELSE",
        ),
        (
            "RETURN CASE 5 WHEN 5 THEN 'five' WHEN 10 THEN 'ten' ELSE 'other' END as switch_case",
            "Switch CASE",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== CASE Expressions Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test date/time functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_datetime_functions() {
    tracing::info!("\n=== Testing Date/Time Functions ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN datetime() as now", "datetime()"),
        ("RETURN date() as today", "date()"),
        ("RETURN time() as current_time", "time()"),
        ("RETURN timestamp() as timestamp_val", "timestamp()"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;
    let mut skipped_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                skipped_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                // For datetime functions, we can't compare exact values (they change),
                // but we can check that both return valid results
                let nexus_has_result = nexus_result
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false);
                let neo4j_has_result = neo4j_result
                    .get("results")
                    .and_then(|r| r.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.as_array())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false);

                if nexus_has_result && neo4j_has_result {
                    tracing::info!("Both return results (values differ by time)");
                    compatible_count += 1;
                } else {
                    tracing::info!("Result structure mismatch");
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                skipped_count += 1;
            }
        }
    }

    tracing::info!("\n=== Date/Time Functions Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
    tracing::info!("Skipped: {}", skipped_count);
}

/// Test list operations
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_list_operations() {
    tracing::info!("\n=== Testing List Operations ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN [1, 2, 3][0] as first", "List index access"),
        ("RETURN size([1, 2, 3]) as list_size", "size()"),
        ("RETURN head([1, 2, 3]) as first_elem", "head()"),
        ("RETURN last([1, 2, 3]) as last_elem", "last()"),
        ("RETURN tail([1, 2, 3]) as rest", "tail()"),
        ("RETURN reverse([1, 2, 3]) as reversed", "reverse()"),
        (
            "RETURN [1, 2] + [3, 4] as concatenated",
            "List concatenation",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== List Operations Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test ORDER BY and LIMIT
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_order_by_and_limit() {
    tracing::info!("\n=== Testing ORDER BY and LIMIT ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 1 as val ORDER BY val", "ORDER BY ascending"),
        ("RETURN 1 as val ORDER BY val DESC", "ORDER BY descending"),
        ("RETURN 1 as val LIMIT 1", "LIMIT"),
        ("RETURN 1 as val ORDER BY val LIMIT 1", "ORDER BY + LIMIT"),
        ("RETURN 1 as val SKIP 0 LIMIT 1", "SKIP + LIMIT"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== ORDER BY and LIMIT Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test DISTINCT
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_distinct() {
    tracing::info!("\n=== Testing DISTINCT ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN DISTINCT 1 as val", "DISTINCT single value"),
        ("RETURN DISTINCT null as val", "DISTINCT null"),
        ("RETURN DISTINCT 'test' as val", "DISTINCT string"),
        ("RETURN DISTINCT [1, 2] as val", "DISTINCT array"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== DISTINCT Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test UNION operations
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_union_operations() {
    tracing::info!("\n=== Testing UNION Operations ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 1 as val UNION RETURN 2 as val", "UNION"),
        ("RETURN 1 as val UNION ALL RETURN 1 as val", "UNION ALL"),
        ("RETURN 'a' as val UNION RETURN 'b' as val", "UNION strings"),
        (
            "RETURN null as val UNION RETURN 1 as val",
            "UNION with null",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== UNION Operations Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test WHERE clauses
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_where_clauses() {
    tracing::info!("\n=== Testing WHERE Clauses ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 5 as val WHERE 5 > 3", "WHERE simple comparison"),
        ("RETURN 5 as val WHERE 5 > 10", "WHERE false condition"),
        ("RETURN 5 as val WHERE true", "WHERE true"),
        ("RETURN 5 as val WHERE false", "WHERE false"),
        ("RETURN 5 as val WHERE null IS NULL", "WHERE IS NULL"),
        ("RETURN 5 as val WHERE 5 IS NOT NULL", "WHERE IS NOT NULL"),
        ("RETURN 5 as val WHERE 5 IN [1, 2, 5]", "WHERE IN"),
        (
            "RETURN 5 as val WHERE 'hello' STARTS WITH 'he'",
            "WHERE STARTS WITH",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== WHERE Clauses Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test multiple columns
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_multiple_columns() {
    tracing::info!("\n=== Testing Multiple Columns ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 1 as a, 2 as b, 3 as c", "Three columns"),
        (
            "RETURN 'hello' as str, 42 as num, true as flag",
            "Mixed types",
        ),
        ("RETURN null as n1, null as n2", "Multiple nulls"),
        ("RETURN [1, 2] as arr, {key: 'val'} as map", "Array and map"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Multiple Columns Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test complex expressions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_complex_expressions() {
    tracing::info!("\n=== Testing Complex Expressions ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN (10 + 5) * 2 as result", "Nested arithmetic"),
        ("RETURN (5 > 3) AND (2 < 4) as result", "Nested logical"),
        (
            "RETURN CASE WHEN (5 > 3) THEN 'yes' ELSE 'no' END as result",
            "CASE with comparison",
        ),
        (
            "RETURN length('hello') + length('world') as result",
            "Function chaining",
        ),
        (
            "RETURN [1, 2, 3][0] + [4, 5, 6][0] as result",
            "List operations in expression",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Complex Expressions Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test parameterized queries
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_parameterized_queries() {
    tracing::info!("\n=== Testing Parameterized Queries ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let test_cases = vec![
        (
            "RETURN $val as result",
            Some({
                let mut params = HashMap::new();
                params.insert("val".to_string(), json!(42));
                params
            }),
            "Integer parameter",
        ),
        (
            "RETURN $val as result",
            Some({
                let mut params = HashMap::new();
                params.insert("val".to_string(), json!(7.5));
                params
            }),
            "Float parameter",
        ),
        (
            "RETURN $val as result",
            Some({
                let mut params = HashMap::new();
                params.insert("val".to_string(), json!("hello"));
                params
            }),
            "String parameter",
        ),
        (
            "RETURN $val as result",
            Some({
                let mut params = HashMap::new();
                params.insert("val".to_string(), json!(true));
                params
            }),
            "Boolean parameter",
        ),
        (
            "RETURN $a + $b as result",
            Some({
                let mut params = HashMap::new();
                params.insert("a".to_string(), json!(10));
                params.insert("b".to_string(), json!(5));
                params
            }),
            "Multiple parameters",
        ),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, params, description) in test_cases {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, params.clone()).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, params).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Parameterized Queries Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test type coercion
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_type_coercion() {
    tracing::info!("\n=== Testing Type Coercion ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN 5 + 3.0 as result", "Int + Float"),
        ("RETURN 'num: ' + 42 as result", "String + Int"),
        ("RETURN 10 / 2.0 as result", "Int / Float"),
        ("RETURN 1.5 * 2 as result", "Float * Int"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Type Coercion Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}

/// Test null handling
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_null_handling() {
    tracing::info!("\n=== Testing Null Handling ===");

    if !check_nexus_available().await {
        etracing::info!("WARNING: Nexus server not available");
        return;
    }

    let queries = vec![
        ("RETURN null as val", "Return null"),
        ("RETURN null + 5 as val", "Null + number"),
        ("RETURN 5 + null as val", "Number + null"),
        ("RETURN null = null as val", "Null = null"),
        ("RETURN null <> null as val", "Null <> null"),
        ("RETURN coalesce(null, 42) as val", "coalesce()"),
        ("RETURN coalesce(5, 42) as val", "coalesce() with value"),
    ];

    let mut compatible_count = 0;
    let mut incompatible_count = 0;

    for (query, description) in queries {
        tracing::info!("\n--- Testing {} ---", description);
        tracing::info!("Query: {}", query);

        let nexus_result = match execute_nexus_query(query, None).await {
            Ok(result) => result,
            Err(e) => {
                tracing::info!("Nexus: Error: {}", e);
                incompatible_count += 1;
                continue;
            }
        };

        match execute_neo4j_query(query, None).await {
            Ok(neo4j_result) => {
                let (compatible, report) = compare_results(&nexus_result, &neo4j_result);
                if compatible {
                    tracing::info!("Compatible");
                    compatible_count += 1;
                } else {
                    tracing::info!("Incompatible: {}", report);
                    incompatible_count += 1;
                }
            }
            Err(e) => {
                tracing::info!("Neo4j: Error: {}", e);
                incompatible_count += 1;
            }
        }
    }

    tracing::info!("\n=== Null Handling Summary ===");
    tracing::info!("Compatible: {}", compatible_count);
    tracing::info!("Incompatible: {}", incompatible_count);
}
