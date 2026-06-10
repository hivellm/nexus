use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};

/// Test mathematical operations
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_mathematical_operations() {
    tracing::info!("\n=== Testing Mathematical Operations ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test comparison operators
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_comparison_operators() {
    tracing::info!("\n=== Testing Comparison Operators ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
