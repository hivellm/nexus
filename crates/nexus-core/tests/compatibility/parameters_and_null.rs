use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};
use serde_json::json;
use std::collections::HashMap;

/// Test parameterized queries
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_parameterized_queries() {
    tracing::info!("\n=== Testing Parameterized Queries ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test null handling
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_null_handling() {
    tracing::info!("\n=== Testing Null Handling ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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
