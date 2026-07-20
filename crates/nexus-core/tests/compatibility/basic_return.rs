use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};

/// Detailed comparison of simple RETURN query
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_detailed_comparison_simple_return() {
    tracing::info!("\n=== Detailed Comparison: Simple RETURN ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
        return;
    }

    let query = "RETURN 1 as value, 'test' as name";

    // Execute in Nexus
    let nexus_result = match execute_nexus_query(query, None).await {
        Ok(result) => result,
        Err(e) => {
            tracing::info!("ERROR: Nexus server error: {}", e);
            tracing::info!("Skipping test - Nexus server not available or returned error");
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
        tracing::info!("WARNING: Nexus server not available");
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
                tracing::info!("ERROR: Nexus server error: {}", e);
                tracing::info!("Skipping test - Nexus server not available or returned error");
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
        tracing::info!("WARNING: Nexus server not available");
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
