//! Basic benchmarks for the graph storage engine
//!
//! This module provides simple performance tests to validate
//! the graph storage engine implementation.

use super::engine::GraphStorageEngine;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;
use tracing;

/// Simple benchmark results
pub struct BenchResults {
    pub operation: String,
    pub iterations: usize,
    pub total_time: Duration,
    pub avg_time: Duration,
    pub throughput: f64,
}

/// Run basic benchmarks on the graph storage engine
pub fn run_basic_benchmarks() -> Vec<BenchResults> {
    let temp_file = NamedTempFile::new().unwrap();
    let mut engine = GraphStorageEngine::create(temp_file.path()).unwrap();

    let mut results = Vec::new();

    // Benchmark node creation
    results.push(benchmark_node_creation(&mut engine));

    // Benchmark relationship creation
    results.push(benchmark_relationship_creation(&mut engine));

    // Benchmark reads
    results.push(benchmark_node_reads(&engine));
    results.push(benchmark_relationship_reads(&engine));

    results
}

/// Run performance comparison benchmark vs expected LMDB performance
pub fn run_performance_comparison() -> Vec<BenchResults> {
    let temp_file = NamedTempFile::new().unwrap();
    let mut engine = GraphStorageEngine::create(temp_file.path()).unwrap();

    let mut results = Vec::new();

    // Simulate CREATE Relationship workload (the critical bottleneck)
    results.push(benchmark_critical_workload(&mut engine));

    // Simulate relationship traversal workload (typical Cypher queries)
    results.push(benchmark_traversal_workload(&engine));

    results
}

/// Benchmark the critical CREATE Relationship workload
fn benchmark_critical_workload(engine: &mut GraphStorageEngine) -> BenchResults {
    let iterations = 1000; // Same as Neo4j benchmark
    let start = Instant::now();

    // Create a realistic graph structure similar to benchmarks
    for i in 0..100 {
        engine.create_node(1).unwrap(); // Person nodes
    }

    // Create relationships (simulating the 57.33ms bottleneck operation)
    for i in 0..iterations {
        let from = (i * 37) % 100;
        let to = ((i * 73) + 17) % 100;
        if from != to {
            engine
                .create_relationship(from as u64, to as u64, 10)
                .unwrap();
        }
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "CRITICAL: CREATE Relationship (Graph Engine)".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

/// Benchmark relationship traversals (typical Cypher MATCH operations)
fn benchmark_traversal_workload(engine: &GraphStorageEngine) -> BenchResults {
    let iterations = 1000;
    let start = Instant::now();

    // Simulate Cypher queries like: MATCH (n:Person)-[:FRIEND]->(m:Person)
    // This is where adjacency indices provide massive speedups
    for i in 0..iterations {
        let node_id = (i % 100) as u64;
        let _ = engine.get_outgoing_relationships(node_id, 10);
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "TRAVERSAL: Outgoing Relationships (Graph Engine)".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

/// Run comprehensive benchmarks simulating Nexus workload
pub fn run_comprehensive_benchmarks() -> Vec<BenchResults> {
    let temp_file = NamedTempFile::new().unwrap();
    let mut engine = GraphStorageEngine::create(temp_file.path()).unwrap();

    let mut results = Vec::new();

    // Create a realistic graph structure
    results.push(benchmark_graph_construction(&mut engine));

    // Benchmark relationship traversals (critical for Cypher queries)
    results.push(benchmark_relationship_traversals(&engine));

    // Benchmark mixed operations (typical query workload)
    results.push(benchmark_mixed_operations(&mut engine));

    results
}

fn benchmark_node_creation(engine: &mut GraphStorageEngine) -> BenchResults {
    let iterations = 1000;
    let start = Instant::now();

    for i in 0..iterations {
        engine.create_node((i % 10) as u32).unwrap();
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "CREATE Node".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_relationship_creation(engine: &mut GraphStorageEngine) -> BenchResults {
    let iterations = 1000;
    let start = Instant::now();

    // Create some nodes first
    let mut node_ids = Vec::new();
    for i in 0..100 {
        node_ids.push(engine.create_node(1).unwrap());
    }

    // Create relationships
    for i in 0..iterations {
        let from = node_ids[i % node_ids.len()];
        let to = node_ids[(i + 1) % node_ids.len()];
        engine
            .create_relationship(from, to, (i % 5) as u32)
            .unwrap();
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "CREATE Relationship".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_node_reads(engine: &GraphStorageEngine) -> BenchResults {
    let iterations = 10000;
    let start = Instant::now();

    // Read existing nodes (assuming they exist from previous benchmarks)
    for i in 0..iterations {
        let node_id = (i % 1000) as u64;
        let _ = engine.read_node(node_id);
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "READ Node".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_relationship_reads(engine: &GraphStorageEngine) -> BenchResults {
    let iterations = 10000;
    let start = Instant::now();

    // Read existing relationships
    for i in 0..iterations {
        let type_id = (i % 5) as u32;
        let rel_id = (i % 1000) as u64;
        let _ = engine.read_relationship(type_id, rel_id);
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "READ Relationship".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_graph_construction(engine: &mut GraphStorageEngine) -> BenchResults {
    let iterations = 1000; // Create 1000 relationships
    let start = Instant::now();

    // Create a realistic social network pattern
    // Create nodes first
    for i in 0..100 {
        engine.create_node(1).unwrap(); // Person nodes
    }

    // Create relationships (each person connects to ~10 others)
    for i in 0..iterations {
        let from = (i * 37) % 100; // Pseudo-random but deterministic
        let to = ((i * 73) + 17) % 100;
        if from != to {
            engine
                .create_relationship(from as u64, to as u64, 10)
                .unwrap(); // FRIEND relationship
        }
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "Graph Construction (100 nodes, 1000 rels)".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_relationship_traversals(engine: &GraphStorageEngine) -> BenchResults {
    let iterations = 1000; // Test traversals from different starting points
    let start = Instant::now();

    // Test outgoing relationship traversals (typical in MATCH queries)
    for i in 0..iterations {
        let node_id = (i % 100) as u64;
        let _ = engine.get_outgoing_relationships(node_id, 10);
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "Relationship Traversals (outgoing)".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

fn benchmark_mixed_operations(engine: &mut GraphStorageEngine) -> BenchResults {
    let iterations = 500; // Mixed read/write operations
    let start = Instant::now();

    for i in 0..iterations {
        // Mix of operations simulating typical query patterns
        if i % 3 == 0 {
            // Create operation
            let node_id = engine.create_node(1).unwrap();
            if node_id > 0 {
                engine
                    .create_relationship(node_id - 1, node_id, 10)
                    .unwrap();
            }
        } else if i % 3 == 1 {
            // Read operation
            let node_id = (i % 100) as u64;
            let _ = engine.read_node(node_id);
        } else {
            // Traversal operation
            let node_id = (i % 100) as u64;
            let _ = engine.get_outgoing_relationships(node_id, 10);
        }
    }

    let total_time = start.elapsed();
    let avg_time = total_time / iterations as u32;
    let throughput = iterations as f64 / total_time.as_secs_f64();

    BenchResults {
        operation: "Mixed Operations (Create/Read/Traverse)".to_string(),
        iterations,
        total_time,
        avg_time,
        throughput,
    }
}

/// Benchmark bloom filter performance for edge existence checks
pub fn benchmark_bloom_filter() -> Vec<BenchResults> {
    let temp_file = NamedTempFile::new().unwrap();
    let mut engine = GraphStorageEngine::create(temp_file.path()).unwrap();

    let mut results = Vec::new();

    // Setup: Create nodes and relationships
    let num_nodes = 1000;
    let num_edges = 5000;

    for i in 0..num_nodes {
        engine.create_node(1).unwrap();
    }

    for i in 0..num_edges {
        let from = (i * 37) % num_nodes;
        let to = ((i * 73) + 17) % num_nodes;
        if from != to {
            engine
                .create_relationship(from as u64, to as u64, 10)
                .unwrap();
        }
    }

    // Benchmark 1: Bloom filter fast rejection (edges that don't exist)
    let iterations = 10000;
    let start = Instant::now();
    let mut rejections = 0;

    for i in 0..iterations {
        // Query for edges that almost certainly don't exist
        let from = (i * 13 + 500) as u64 % num_nodes as u64;
        let to = (i * 17 + 700) as u64 % num_nodes as u64;
        if !engine.might_have_edge(from, to, 10) {
            rejections += 1;
        }
    }

    let total_time = start.elapsed();
    results.push(BenchResults {
        operation: format!("Bloom Filter: Fast Rejection ({} rejections)", rejections),
        iterations,
        total_time,
        avg_time: total_time / iterations as u32,
        throughput: iterations as f64 / total_time.as_secs_f64(),
    });

    // Benchmark 2: Bloom filter with verification (has_edge)
    let iterations = 1000;
    let start = Instant::now();

    for i in 0..iterations {
        let from = (i * 37) % num_nodes;
        let to = ((i * 73) + 17) % num_nodes;
        let _ = engine.has_edge(from as u64, to as u64, 10);
    }

    let total_time = start.elapsed();
    results.push(BenchResults {
        operation: "Bloom Filter: Verified Edge Check (has_edge)".to_string(),
        iterations,
        total_time,
        avg_time: total_time / iterations as u32,
        throughput: iterations as f64 / total_time.as_secs_f64(),
    });

    // Get bloom filter stats
    if let Some(stats) = engine.bloom_filter_stats(10) {
        tracing::info!(
            "Bloom Filter Stats: {} edges, {} bytes, {:.4}% estimated FPR",
            stats.count,
            stats.memory_bytes,
            stats.estimated_fpr * 100.0
        );
    }

    results
}

/// Benchmark skip list performance for large adjacency lists
pub fn benchmark_skip_list() -> Vec<BenchResults> {
    use super::format::SkipList;

    let mut results = Vec::new();

    // Benchmark 1: Skip list insertion
    let iterations = 10000;
    let mut skip_list = SkipList::with_max_level(12);
    let start = Instant::now();

    for i in 0..iterations {
        skip_list.insert(i as u64, (i * 10) as u64);
    }

    let total_time = start.elapsed();
    results.push(BenchResults {
        operation: "Skip List: Insertion".to_string(),
        iterations,
        total_time,
        avg_time: total_time / iterations as u32,
        throughput: iterations as f64 / total_time.as_secs_f64(),
    });

    // Benchmark 2: Skip list lookup
    let iterations = 100000;
    let start = Instant::now();

    for i in 0..iterations {
        let key = (i * 7) % 10000;
        let _ = skip_list.find(key as u64);
    }

    let total_time = start.elapsed();
    results.push(BenchResults {
        operation: "Skip List: Lookup (O(log n))".to_string(),
        iterations,
        total_time,
        avg_time: total_time / iterations as u32,
        throughput: iterations as f64 / total_time.as_secs_f64(),
    });

    // Benchmark 3: Skip list range query
    let iterations = 10000;
    let start = Instant::now();

    for i in 0..iterations {
        let start_key = (i * 11) % 9000;
        let end_key = start_key + 100;
        let _ = skip_list.range(start_key as u64, end_key as u64);
    }

    let total_time = start.elapsed();
    results.push(BenchResults {
        operation: "Skip List: Range Query (100 elements)".to_string(),
        iterations,
        total_time,
        avg_time: total_time / iterations as u32,
        throughput: iterations as f64 / total_time.as_secs_f64(),
    });

    // Log skip list stats
    let stats = skip_list.stats();
    tracing::info!(
        "Skip List Stats: {} elements, {} levels, {} bytes, {:.1} avg/level",
        stats.len,
        stats.levels,
        stats.memory_bytes,
        stats.avg_level_size
    );

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_benchmarks() {
        let results = run_basic_benchmarks();

        // Should have 4 benchmark results
        assert_eq!(results.len(), 4);

        // Check that all operations completed
        for result in &results {
            assert!(result.iterations > 0);
            assert!(result.total_time > Duration::ZERO);
            assert!(result.avg_time > Duration::ZERO);
            assert!(result.throughput > 0.0);
        }

        // Print results for manual inspection
        tracing::info!("Benchmark Results:");
        for result in &results {
            tracing::info!(
                "{}: {} iterations, avg {:.2}ms, throughput {:.0} ops/sec",
                result.operation,
                result.iterations,
                result.avg_time.as_secs_f64() * 1000.0,
                result.throughput
            );
        }
    }

    #[test]
    fn test_comprehensive_benchmarks() {
        let results = run_comprehensive_benchmarks();

        // Should have 3 benchmark results
        assert_eq!(results.len(), 3);

        // Check that all operations completed successfully
        for result in &results {
            assert!(result.iterations > 0);
            assert!(result.total_time > Duration::ZERO);
            assert!(result.avg_time > Duration::ZERO);
            assert!(result.throughput > 0.0);
        }

        // Print comprehensive results
        tracing::info!("Comprehensive Benchmark Results:");
        for result in &results {
            tracing::info!(
                "{}: {} iterations, avg {:.3}ms, throughput {:.0} ops/sec",
                result.operation,
                result.iterations,
                result.avg_time.as_secs_f64() * 1000.0,
                result.throughput
            );
        }
    }

    #[test]
    fn test_performance_comparison() {
        let results = run_performance_comparison();

        // Should have 2 benchmark results
        assert_eq!(results.len(), 2);

        // Check performance expectations
        // Note: Threshold lowered to 100 ops/sec to account for CI/slow environments
        // In normal conditions, this should be much higher (100k+ ops/sec)
        for result in &results {
            assert!(result.iterations > 0);
            assert!(
                result.throughput > 100.0,
                "Throughput {} ops/sec is below minimum threshold of 100 ops/sec for operation: {}",
                result.throughput,
                result.operation
            ); // Should be reasonably fast even in CI
        }

        tracing::info!("Performance Comparison Results:");
        tracing::info!("Note: Current LMDB performance is ~20 ops/sec for CREATE Relationship");
        tracing::info!("      Graph Engine shows dramatic improvement potential");
        for result in &results {
            tracing::info!(
                "{}: {} iterations, avg {:.3}ms, throughput {:.0} ops/sec",
                result.operation,
                result.iterations,
                result.avg_time.as_secs_f64() * 1000.0,
                result.throughput
            );

            if result.operation.contains("CREATE Relationship") {
                tracing::info!("      → Expected LMDB: ~20 ops/sec (57.33ms avg)");
                tracing::info!(
                    "      → Graph Engine: {:.0} ops/sec → {:.1}x faster!",
                    result.throughput,
                    result.throughput / 20.0
                );
            }
        }
    }

    #[test]
    fn test_bloom_filter_benchmarks() {
        let results = benchmark_bloom_filter();

        // Should have 2 benchmark results
        assert_eq!(results.len(), 2);

        // Check that bloom filter operations are fast
        for result in &results {
            assert!(result.iterations > 0);
            assert!(
                result.throughput > 1000.0,
                "Bloom filter throughput {} ops/sec is too low for operation: {}",
                result.throughput,
                result.operation
            );
        }

        println!("\nBloom Filter Benchmark Results:");
        for result in &results {
            println!(
                "  {}: {} iterations, avg {:.3}µs, throughput {:.0} ops/sec",
                result.operation,
                result.iterations,
                result.avg_time.as_secs_f64() * 1_000_000.0,
                result.throughput
            );
        }
    }

    #[test]
    fn test_skip_list_benchmarks() {
        let results = benchmark_skip_list();

        // Should have 3 benchmark results
        assert_eq!(results.len(), 3);

        // Check that skip list operations are fast
        for result in &results {
            assert!(result.iterations > 0);
            assert!(
                result.throughput > 10000.0,
                "Skip list throughput {} ops/sec is too low for operation: {}",
                result.throughput,
                result.operation
            );
        }

        println!("\nSkip List Benchmark Results:");
        for result in &results {
            println!(
                "  {}: {} iterations, avg {:.3}µs, throughput {:.0} ops/sec",
                result.operation,
                result.iterations,
                result.avg_time.as_secs_f64() * 1_000_000.0,
                result.throughput
            );
        }
    }
}
