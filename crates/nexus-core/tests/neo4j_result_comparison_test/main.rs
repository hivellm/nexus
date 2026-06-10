//! Detailed Neo4j Result Comparison Tests
//!
//! These tests compare query results between Nexus and Neo4j to ensure identical return values.
//! Requires both servers to be running:
//! - Nexus: http://localhost:15474 (via REST API)
//! - Neo4j: http://localhost:7474 (HTTP endpoint)

mod basic_return;
mod expressions;
mod functions;
mod operators;
mod parameters_and_null;
mod query_clauses;

use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;

/// Execute a query on Nexus via the REST API.
pub(crate) async fn execute_nexus_query(
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

/// Execute a query on Neo4j via the HTTP API.
pub(crate) async fn execute_neo4j_query(
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

/// Check whether the Nexus server is reachable.
pub(crate) async fn check_nexus_available() -> bool {
    let client = Client::new();
    client
        .get("http://localhost:15474/health")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Normalize a single Neo4j data row to a plain `Vec` of values.
pub(crate) fn normalize_neo4j_row(neo4j_row: &serde_json::Value) -> Option<Vec<serde_json::Value>> {
    neo4j_row.get("row").and_then(|r| r.as_array()).cloned()
}

/// Deep-equal comparison that handles int/float cross-type equality.
pub(crate) fn values_equal(nexus_val: &serde_json::Value, neo4j_val: &serde_json::Value) -> bool {
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

/// Compare a Nexus result with a Neo4j result and return a (match, report) pair.
pub(crate) fn compare_results(
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
