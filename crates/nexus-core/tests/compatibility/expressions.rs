use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};

/// Test CASE expressions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_case_expressions() {
    tracing::info!("\n=== Testing CASE Expressions ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test complex expressions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_complex_expressions() {
    tracing::info!("\n=== Testing Complex Expressions ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test type coercion
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_type_coercion() {
    tracing::info!("\n=== Testing Type Coercion ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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
