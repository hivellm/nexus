use crate::{check_nexus_available, compare_results, execute_neo4j_query, execute_nexus_query};

/// Test aggregation functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_aggregation_functions() {
    tracing::info!("\n=== Testing Aggregation Functions ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test string functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_string_functions() {
    tracing::info!("\n=== Testing String Functions ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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

/// Test date/time functions
#[tokio::test]
#[cfg_attr(
    not(feature = "slow-tests"),
    ignore = "Slow test - enable with --features slow-tests"
)]
async fn test_datetime_functions() {
    tracing::info!("\n=== Testing Date/Time Functions ===");

    if !check_nexus_available().await {
        tracing::info!("WARNING: Nexus server not available");
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
        tracing::info!("WARNING: Nexus server not available");
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
