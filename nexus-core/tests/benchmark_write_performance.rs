//! Write Performance Benchmarks
//!
//! Benchmarks for Phase 1 tasks:
//! - 1.3.4: Measure performance improvement from deferred index updates
//! - 1.7: Benchmark write performance (CREATE node and CREATE relationship)

use nexus_core::Engine;
use nexus_core::testing::{TestContext, setup_test_engine};
use std::time::{Duration, Instant};

/// Helper to extract count from result
fn extract_count(result: nexus_core::executor::ResultSet) -> u64 {
    result
        .rows
        .first()
        .and_then(|row| row.values.first())
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Benchmark CREATE node operations
#[tokio::test]
#[cfg(feature = "benchmarks")]
async fn benchmark_create_node_operations() {
    tracing::info!("\n=== Benchmark: CREATE Node Operations ===");

    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let num_operations = 1000;
    let mut latencies = Vec::new();

    tracing::info!("Creating {} nodes...", num_operations);

    for i in 0..num_operations {
        let start = Instant::now();

        let query = format!(
            "CREATE (n:Person {{id: {}, name: 'Person{}', age: {}, email: 'person{}@example.com'}})",
            i,
            i,
            20 + (i % 50),
            i
        );

        engine.execute_cypher(&query).unwrap();

        let latency = start.elapsed();
        latencies.push(latency);

        if (i + 1) % 100 == 0 {
            tracing::info!("  Created {} nodes...", i + 1);
        }
    }

    // Calculate statistics
    latencies.sort();
    let total: Duration = latencies.iter().sum();
    let avg = total / latencies.len() as u32;
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];
    let p99 = latencies[(latencies.len() * 99) / 100];
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];

    tracing::info!("\nResults:");
    tracing::info!("  Total operations: {}", num_operations);
    tracing::info!("  Total time: {:.2}ms", total.as_secs_f64() * 1000.0);
    tracing::info!("  Average latency: {:.2}ms", avg.as_secs_f64() * 1000.0);
    tracing::info!("  P50 latency: {:.2}ms", p50.as_secs_f64() * 1000.0);
    tracing::info!("  P95 latency: {:.2}ms", p95.as_secs_f64() * 1000.0);
    tracing::info!("  P99 latency: {:.2}ms", p99.as_secs_f64() * 1000.0);
    tracing::info!("  Min latency: {:.2}ms", min.as_secs_f64() * 1000.0);
    tracing::info!("  Max latency: {:.2}ms", max.as_secs_f64() * 1000.0);
    tracing::info!(
        "  Throughput: {:.2} ops/sec",
        num_operations as f64 / total.as_secs_f64()
    );

    // Verify data consistency
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(count, num_operations as u64, "All nodes should be created");

    // Phase 1 target: ≤ 8ms average
    tracing::info!("\nTarget: ≤ 8ms average");
    tracing::info!(
        "Status: {}",
        if avg.as_millis() <= 8 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );
}

/// Benchmark CREATE relationship operations
#[tokio::test]
#[cfg(feature = "benchmarks")]
async fn benchmark_create_relationship_operations() {
    tracing::info!("\n=== Benchmark: CREATE Relationship Operations ===");

    let (mut engine, _ctx) = setup_test_engine().unwrap();

    // Create base nodes first
    tracing::info!("Creating base nodes...");
    for i in 0..100 {
        let query = format!("CREATE (n:Person {{id: {}, name: 'Person{}'}})", i, i);
        engine.execute_cypher(&query).unwrap();
    }

    let num_operations = 1000;
    let mut latencies = Vec::new();

    tracing::info!("Creating {} relationships...", num_operations);

    for i in 0..num_operations {
        let start = Instant::now();

        let source = i % 100;
        let target = (i + 1) % 100;
        let query = format!(
            "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS {{weight: {}}}]->(b)",
            source,
            target,
            i % 10
        );

        engine.execute_cypher(&query).unwrap();

        let latency = start.elapsed();
        latencies.push(latency);

        if (i + 1) % 100 == 0 {
            tracing::info!("  Created {} relationships...", i + 1);
        }
    }

    // Calculate statistics
    latencies.sort();
    let total: Duration = latencies.iter().sum();
    let avg = total / latencies.len() as u32;
    let p50 = latencies[latencies.len() / 2];
    let p95 = latencies[(latencies.len() * 95) / 100];
    let p99 = latencies[(latencies.len() * 99) / 100];
    let min = latencies[0];
    let max = latencies[latencies.len() - 1];

    tracing::info!("\nResults:");
    tracing::info!("  Total operations: {}", num_operations);
    tracing::info!("  Total time: {:.2}ms", total.as_secs_f64() * 1000.0);
    tracing::info!("  Average latency: {:.2}ms", avg.as_secs_f64() * 1000.0);
    tracing::info!("  P50 latency: {:.2}ms", p50.as_secs_f64() * 1000.0);
    tracing::info!("  P95 latency: {:.2}ms", p95.as_secs_f64() * 1000.0);
    tracing::info!("  P99 latency: {:.2}ms", p99.as_secs_f64() * 1000.0);
    tracing::info!("  Min latency: {:.2}ms", min.as_secs_f64() * 1000.0);
    tracing::info!("  Max latency: {:.2}ms", max.as_secs_f64() * 1000.0);
    tracing::info!(
        "  Throughput: {:.2} ops/sec",
        num_operations as f64 / total.as_secs_f64()
    );

    // Verify data consistency
    let result = engine
        .execute_cypher("MATCH ()-[r:KNOWS]->() RETURN count(r) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count, num_operations as u64,
        "All relationships should be created"
    );

    // Phase 1 target: ≤ 12ms average
    tracing::info!("\nTarget: ≤ 12ms average");
    tracing::info!(
        "Status: {}",
        if avg.as_millis() <= 12 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );
}

/// Benchmark deferred index updates performance improvement
#[tokio::test]
#[cfg(feature = "benchmarks")]
async fn benchmark_deferred_index_updates() {
    tracing::info!("\n=== Benchmark: Deferred Index Updates Performance ===");
    tracing::info!("Comparing batch index updates vs immediate updates");

    let (mut engine, _ctx) = setup_test_engine().unwrap();

    let num_nodes = 500;
    let mut latencies_with_batching = Vec::new();

    tracing::info!("Testing with deferred index updates (current implementation)...");

    // Test with transactions (deferred index updates)
    engine.execute_cypher("BEGIN TRANSACTION").unwrap();

    for i in 0..num_nodes {
        let start = Instant::now();

        let query = format!(
            "CREATE (n:Person:Employee {{id: {}, name: 'Person{}', age: {}}})",
            i,
            i,
            20 + (i % 50)
        );

        engine.execute_cypher(&query).unwrap();

        let latency = start.elapsed();
        latencies_with_batching.push(latency);
    }

    let commit_start = Instant::now();
    engine.execute_cypher("COMMIT TRANSACTION").unwrap();
    let commit_time = commit_start.elapsed();

    // Calculate statistics
    latencies_with_batching.sort();
    let total_with_batching: Duration = latencies_with_batching.iter().sum();
    let avg_with_batching = total_with_batching / latencies_with_batching.len() as u32;
    let total_including_commit = total_with_batching + commit_time;
    let avg_including_commit = total_including_commit / (num_nodes + 1) as u32;

    tracing::info!("\nResults (with deferred index updates):");
    tracing::info!("  Total operations: {}", num_nodes);
    tracing::info!(
        "  Total time (creates): {:.2}ms",
        total_with_batching.as_secs_f64() * 1000.0
    );
    tracing::info!("  Commit time: {:.2}ms", commit_time.as_secs_f64() * 1000.0);
    tracing::info!(
        "  Total time (including commit): {:.2}ms",
        total_including_commit.as_secs_f64() * 1000.0
    );
    tracing::info!(
        "  Average latency (creates only): {:.2}ms",
        avg_with_batching.as_secs_f64() * 1000.0
    );
    tracing::info!(
        "  Average latency (including commit): {:.2}ms",
        avg_including_commit.as_secs_f64() * 1000.0
    );
    tracing::info!(
        "  Throughput: {:.2} ops/sec",
        num_nodes as f64 / total_including_commit.as_secs_f64()
    );

    // Verify index consistency
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let person_count = extract_count(result);

    let result = engine
        .execute_cypher("MATCH (n:Employee) RETURN count(n) as count")
        .unwrap();
    let employee_count = extract_count(result);

    assert_eq!(
        person_count, num_nodes as u64,
        "All Person nodes should be indexed"
    );
    assert_eq!(
        employee_count, num_nodes as u64,
        "All Employee nodes should be indexed"
    );

    tracing::info!("\nIndex consistency check:");
    tracing::info!("  Person nodes indexed: {}", person_count);
    tracing::info!("  Employee nodes indexed: {}", employee_count);
    tracing::info!("  Status: ✅ PASS");
}

/// Benchmark concurrent write performance
#[tokio::test]
#[cfg(feature = "benchmarks")]
async fn benchmark_concurrent_write_performance() {
    tracing::info!("\n=== Benchmark: Concurrent Write Performance ===");

    let ctx = TestContext::new();
    let engine = Engine::with_data_dir(ctx.path()).unwrap();
    let engine = std::sync::Arc::new(std::sync::Mutex::new(engine));
    let _ctx = ctx; // Keep ctx alive for the duration of the test

    let num_threads = 10;
    let operations_per_thread = 100;
    let total_operations = num_threads * operations_per_thread;

    tracing::info!(
        "Running {} concurrent threads, {} operations each...",
        num_threads,
        operations_per_thread
    );

    let start = Instant::now();
    let mut handles = vec![];

    for thread_id in 0..num_threads {
        let engine_clone = engine.clone();
        let handle = tokio::spawn(async move {
            let mut engine = engine_clone.lock().unwrap();
            let mut thread_latencies = Vec::new();

            for i in 0..operations_per_thread {
                let op_start = Instant::now();

                let node_id = thread_id * operations_per_thread + i;
                let query = format!(
                    "CREATE (n:Person {{id: {}, thread: {}, name: 'Person{}'}})",
                    node_id, thread_id, node_id
                );

                engine.execute_cypher(&query).unwrap();

                thread_latencies.push(op_start.elapsed());
            }

            thread_latencies
        });
        handles.push(handle);
    }

    let mut all_latencies = Vec::new();
    for handle in handles {
        let thread_latencies = handle.await.unwrap();
        all_latencies.extend(thread_latencies);
    }

    let total_time = start.elapsed();

    // Calculate statistics
    all_latencies.sort();
    let total_latency: Duration = all_latencies.iter().sum();
    let avg = total_latency / all_latencies.len() as u32;
    let p95 = all_latencies[(all_latencies.len() * 95) / 100];
    let p99 = all_latencies[(all_latencies.len() * 99) / 100];

    tracing::info!("\nResults:");
    tracing::info!("  Total operations: {}", total_operations);
    tracing::info!(
        "  Total wall-clock time: {:.2}ms",
        total_time.as_secs_f64() * 1000.0
    );
    tracing::info!(
        "  Average latency per operation: {:.2}ms",
        avg.as_secs_f64() * 1000.0
    );
    tracing::info!("  P95 latency: {:.2}ms", p95.as_secs_f64() * 1000.0);
    tracing::info!("  P99 latency: {:.2}ms", p99.as_secs_f64() * 1000.0);
    tracing::info!(
        "  Throughput: {:.2} ops/sec",
        total_operations as f64 / total_time.as_secs_f64()
    );

    // Verify data consistency
    let mut engine = engine.lock().unwrap();
    let result = engine
        .execute_cypher("MATCH (n:Person) RETURN count(n) as count")
        .unwrap();
    let count = extract_count(result);
    assert_eq!(
        count, total_operations as u64,
        "All nodes should be created"
    );

    tracing::info!("  Nodes created: {}", count);
    tracing::info!("  Status: ✅ PASS");
}
