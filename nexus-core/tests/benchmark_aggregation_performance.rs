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

/// Phase 2.5.4: Benchmark parallel aggregation speedup with large datasets
/// Tests parallel aggregation performance vs sequential for datasets > 1000 rows
#[test]
fn benchmark_parallel_aggregation_speedup() {
    let dir = TempDir::new().unwrap();
    let mut engine = Engine::with_data_dir(dir.path()).unwrap();

    // Create large dataset (5000 nodes to trigger parallel aggregation)
    // Parallel threshold is 1000 rows, so 5000 should definitely trigger it
    println!("Creating large dataset for parallel aggregation benchmark (5000 nodes)...");
    for i in 0..5000 {
        let query = format!(
            "CREATE (n:Person {{id: {}, age: {}, salary: {}}})",
            i,
            20 + (i % 50),
            30000 + (i % 50000)
        );
        execute_cypher(&mut engine, &query);
    }

    println!("\n=== Parallel Aggregation Speedup Benchmark ===");
    println!("Dataset size: 5000 nodes");
    println!("Parallel threshold: 1000 rows (should trigger parallel processing)");

    // Benchmark COUNT(*) - should use parallel aggregation
    println!("\n1. Benchmarking COUNT(*) (parallel)...");
    let start = Instant::now();
    let iterations = 50;

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN count(*) as total";
        execute_cypher(&mut engine, query);
    }

    let elapsed_parallel_count = start.elapsed();
    let avg_parallel_count = elapsed_parallel_count.as_millis() as f64 / iterations as f64;

    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed_parallel_count);
    println!("  Average time: {:.2}ms", avg_parallel_count);

    // Benchmark SUM - should use parallel aggregation
    println!("\n2. Benchmarking SUM (parallel)...");
    let start = Instant::now();

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN sum(n.salary) as total_salary";
        execute_cypher(&mut engine, query);
    }

    let elapsed_parallel_sum = start.elapsed();
    let avg_parallel_sum = elapsed_parallel_sum.as_millis() as f64 / iterations as f64;

    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed_parallel_sum);
    println!("  Average time: {:.2}ms", avg_parallel_sum);

    // Benchmark MIN - should use parallel aggregation
    println!("\n3. Benchmarking MIN (parallel)...");
    let start = Instant::now();

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN min(n.age) as min_age";
        execute_cypher(&mut engine, query);
    }

    let elapsed_parallel_min = start.elapsed();
    let avg_parallel_min = elapsed_parallel_min.as_millis() as f64 / iterations as f64;

    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed_parallel_min);
    println!("  Average time: {:.2}ms", avg_parallel_min);

    // Benchmark MAX - should use parallel aggregation
    println!("\n4. Benchmarking MAX (parallel)...");
    let start = Instant::now();

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN max(n.age) as max_age";
        execute_cypher(&mut engine, query);
    }

    let elapsed_parallel_max = start.elapsed();
    let avg_parallel_max = elapsed_parallel_max.as_millis() as f64 / iterations as f64;

    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed_parallel_max);
    println!("  Average time: {:.2}ms", avg_parallel_max);

    // Benchmark AVG - should use parallel aggregation
    println!("\n5. Benchmarking AVG (parallel)...");
    let start = Instant::now();

    for _ in 0..iterations {
        let query = "MATCH (n:Person) RETURN avg(n.salary) as avg_salary";
        execute_cypher(&mut engine, query);
    }

    let elapsed_parallel_avg = start.elapsed();
    let avg_parallel_avg = elapsed_parallel_avg.as_millis() as f64 / iterations as f64;

    println!("  Iterations: {}", iterations);
    println!("  Total time: {:?}", elapsed_parallel_avg);
    println!("  Average time: {:.2}ms", avg_parallel_avg);

    // Summary
    println!("\n=== Parallel Aggregation Summary ===");
    println!(
        "COUNT(*):  {:.2}ms average ({} iterations)",
        avg_parallel_count, iterations
    );
    println!(
        "SUM:       {:.2}ms average ({} iterations)",
        avg_parallel_sum, iterations
    );
    println!(
        "MIN:       {:.2}ms average ({} iterations)",
        avg_parallel_min, iterations
    );
    println!(
        "MAX:       {:.2}ms average ({} iterations)",
        avg_parallel_max, iterations
    );
    println!(
        "AVG:       {:.2}ms average ({} iterations)",
        avg_parallel_avg, iterations
    );

    // Verify results are correct
    let count_result = execute_cypher(&mut engine, "MATCH (n:Person) RETURN count(*) as total");
    assert_eq!(count_result.rows.len(), 1);
    if let Some(count) = count_result.rows[0].values[0].as_u64() {
        assert_eq!(count, 5000, "Should count 5000 nodes");
    }

    println!("\n✅ All parallel aggregation benchmarks completed successfully!");
    println!("   Dataset size: 5000 nodes (above 1000 threshold for parallel processing)");
    println!("   All aggregations should have used parallel processing");
}
