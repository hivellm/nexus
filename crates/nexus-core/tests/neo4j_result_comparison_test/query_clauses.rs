use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};

/// Test ORDER BY and LIMIT
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_order_by_and_limit() {
    tracing::info!("\n=== Testing ORDER BY and LIMIT ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
