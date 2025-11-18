//! Benchmark aggregation performance
//!
//! Phase 2.6: Benchmark aggregation performance
//! Tests measure COUNT, GROUP BY, and COLLECT performance

use nexus_core::Engine;
use std::time::Instant;
use tempfile::TempDir;

/// Helper function to execute a Cypher query
fn execute_cypher(engine: &mut Engine, query: &str) -> nexus_core::executor::ResultSet {
    engine.execute_cypher(query).unwrap()
}

/// Benchmark COUNT(*) performance
#[test]
fn benchmark_count_star() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    println!("Creating test data for COUNT(*) benchmark...");
    for i in 0..1000 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + (i % 50));
        execute_cypher(&mut engine, &query);
    }

    // Benchmark COUNT(*)
    println!("Benchmarking COUNT(*)...");
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let query = "MATCH (n) RETURN count(*) as total";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("COUNT(*) benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);
    println!("  Target: ≤ 2ms average");

    // Verify result
    let result = execute_cypher(&mut engine, "MATCH (n) RETURN count(*) as total");
    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert_eq!(count, 1000, "Should count 1000 nodes");
    }
}

/// Benchmark GROUP BY performance
#[test]
fn benchmark_group_by() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data with different labels
    println!("Creating test data for GROUP BY benchmark...");
    for i in 0..500 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + (i % 50));
        execute_cypher(&mut engine, &query);
    }
    for i in 0..300 {
        let query = format!(
            "CREATE (n:Company {{id: {}, employees: {}}})",
            i,
            10 + (i % 20)
        );
        execute_cypher(&mut engine, &query);
    }

    // Benchmark GROUP BY
    println!("Benchmarking GROUP BY...");
    let start = Instant::now();
    let iterations = 50;

    for _ in 0..iterations {
        let query = "MATCH (n) RETURN labels(n)[0] as label, count(*) as total ORDER BY label";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("GROUP BY benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);
    println!("  Target: ≤ 3ms average");

    // Verify result
    let result = execute_cypher(
        &mut engine,
        "MATCH (n) RETURN labels(n)[0] as label, count(*) as total ORDER BY label",
    );
    assert!(result.rows.len() >= 1, "Should have at least 1 group");
}

/// Benchmark COLLECT performance
#[test]
fn benchmark_collect() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    println!("Creating test data for COLLECT benchmark...");
    for i in 0..500 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + (i % 50));
        execute_cypher(&mut engine, &query);
    }

    // Benchmark COLLECT
    println!("Benchmarking COLLECT...");
    let start = Instant::now();
    let iterations = 50;

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN collect(n.age) as ages";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("COLLECT benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);
    println!("  Target: ≤ 3ms average");

    // Verify result
    let result = execute_cypher(
        &mut engine,
        "MATCH (n:Person) RETURN collect(n.age) as ages",
    );
    assert_eq!(result.rows.len(), 1);
    if let Some(ages) = result.rows[0].values[0].as_array() {
        assert_eq!(ages.len(), 500, "Should collect 500 ages");
    }
}

/// Benchmark MIN/MAX performance
#[test]
fn benchmark_min_max() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    println!("Creating test data for MIN/MAX benchmark...");
    for i in 0..1000 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + (i % 50));
        execute_cypher(&mut engine, &query);
    }

    // Benchmark MIN
    println!("Benchmarking MIN...");
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN min(n.age) as min_age";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("MIN benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);

    // Benchmark MAX
    println!("Benchmarking MAX...");
    let start = Instant::now();

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN max(n.age) as max_age";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("MAX benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);

    // Verify results
    let min_result = execute_cypher(&mut engine, "MATCH (n:Person) RETURN min(n.age) as min_age");
    let max_result = execute_cypher(&mut engine, "MATCH (n:Person) RETURN max(n.age) as max_age");

    assert_eq!(min_result.rows.len(), 1);
    assert_eq!(max_result.rows.len(), 1);

    if let Some(min_age) = min_result.rows[0].values[0].as_u64() {
        assert_eq!(min_age, 20, "Min age should be 20");
    }
    if let Some(max_age) = max_result.rows[0].values[0].as_u64() {
        assert_eq!(max_age, 69, "Max age should be 69 (20 + 49)");
    }
}

/// Benchmark aggregation with WHERE filter
#[test]
fn benchmark_aggregation_with_filter() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create test data
    println!("Creating test data for filtered aggregation benchmark...");
    for i in 0..1000 {
        let query = format!("CREATE (n:Person {{id: {}, age: {}}})", i, 20 + (i % 50));
        execute_cypher(&mut engine, &query);
    }

    // Benchmark COUNT with WHERE
    println!("Benchmarking COUNT with WHERE filter...");
    let start = Instant::now();
    let iterations = 100;

    for _ in 0..iterations {
        let query = "MATCH (n:Person) WHERE n.age > 40 RETURN count(*) as total";
        execute_cypher(&mut engine, query);
    }

    let elapsed = start.elapsed();
    let avg_time = elapsed.as_millis() as f64 / iterations as f64;

    println!("COUNT with WHERE benchmark:");
    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed);
    println!("  Average time: {:.2}ms", avg_time);

    // Verify result
    let result = execute_cypher(
        &mut engine,
        "MATCH (n:Person) WHERE n.age > 40 RETURN count(*) as total",
    );
    assert_eq!(result.rows.len(), 1);
    if let Some(count) = result.rows[0].values[0].as_u64() {
        assert!(count > 0, "Should count some nodes with age > 40");
    }
}
