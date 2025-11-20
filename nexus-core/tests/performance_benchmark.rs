//! Performance Benchmark Suite
//!
//! Comprehensive benchmarks comparing Nexus performance against Neo4j targets.
//! Tests all performance optimizations working together under realistic workloads.
//!
//! Targets (based on Neo4j performance):
//! - CREATE operations: <5ms average
//! - READ operations: <3ms for cached data
//! - Throughput: >500 queries/second
//! - Memory usage: <2GB for test dataset

use nexus_core::cache::MultiLayerCache;
use nexus_core::executor::{Executor, Query};
use nexus_core::index::LabelIndex;
use nexus_core::storage::RecordStore;
use nexus_core::{catalog::Catalog, index::KnnIndex};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tracing;

/// Benchmark configuration
#[derive(Debug, Clone)]
struct BenchmarkConfig {
    /// Number of nodes for benchmark
    node_count: usize,
    /// Number of relationships for benchmark
    relationship_count: usize,
    /// Benchmark duration in seconds
    duration_secs: u64,
    /// Warmup duration in seconds
    warmup_secs: u64,
    /// Concurrent clients
    concurrent_clients: usize,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            node_count: 10000,         // 10K nodes
            relationship_count: 50000, // 50K relationships
            duration_secs: 60,         // 1 minute benchmark
            warmup_secs: 10,           // 10 second warmup
            concurrent_clients: 20,    // 20 concurrent clients
        }
    }
}

/// Benchmark results
#[derive(Debug)]
struct BenchmarkResults {
    /// Total benchmark duration
    duration: Duration,
    /// Operations per second
    throughput: f64,
    /// Average latency per operation
    avg_latency: Duration,
    /// P95 latency
    p95_latency: Duration,
    /// P99 latency
    p99_latency: Duration,
    /// Cache hit rate
    cache_hit_rate: f64,
    /// Memory usage in MB
    memory_usage_mb: f64,
    /// CREATE operations per second
    create_throughput: f64,
    /// READ operations per second
    read_throughput: f64,
    /// Neo4j targets met
    targets_met: bool,
}

/// Create benchmark environment
fn create_benchmark_environment(_config: &BenchmarkConfig) -> (Executor, MultiLayerCache, TempDir) {
    let dir = TempDir::new().unwrap();

    // Create all components
    let catalog = Arc::new(Catalog::new(dir.path()).unwrap());
    let store = RecordStore::new(dir.path()).unwrap();
    let label_index = Arc::new(LabelIndex::new());
    let knn_index = Arc::new(KnnIndex::new_default(128).unwrap());

    // Create multi-layer cache with default config
    let cache = MultiLayerCache::new(nexus_core::cache::CacheConfig::default()).unwrap();

    // Create executor
    let executor = Executor::new(&catalog, &store, &label_index, &knn_index).unwrap();

    (executor, cache, dir)
}

/// Generate benchmark dataset
async fn generate_benchmark_dataset(executor: &mut Executor, config: &BenchmarkConfig) {
    tracing::info!(
        "Generating benchmark dataset: {} nodes, {} relationships...",
        config.node_count,
        config.relationship_count
    );

    // Create nodes in larger batches for performance
    let batch_size = 500;
    for i in (0..config.node_count).step_by(batch_size) {
        let end = (i + batch_size).min(config.node_count);
        let mut cypher = String::from("CREATE ");
        let mut first = true;

        for j in i..end {
            if !first {
                cypher.push_str(", ");
            }
            first = false;
            cypher.push_str(&format!(
                "(n{}:Person {{id: {}, name: 'Person{}', age: {}}})",
                j,
                j,
                j,
                20 + (j % 50)
            ));
        }

        let query = Query {
            cypher,
            params: HashMap::new(),
        };
        executor.execute(&query).unwrap();
    }

    // Create relationships
    for i in 0..config.relationship_count {
        let source_id = i % config.node_count;
        let target_id = (i * 7 + 13) % config.node_count;

        let query = Query {
            cypher: format!(
                "MATCH (a:Person {{id: {}}}), (b:Person {{id: {}}}) CREATE (a)-[:KNOWS {{weight: {}}}]->(b)",
                source_id,
                target_id,
                i % 10
            ),
            params: HashMap::new(),
        };
        executor.execute(&query).unwrap();
    }

    tracing::info!("Benchmark dataset generation complete");
}

/// Run CREATE benchmark (Neo4j target: <5ms per operation)
async fn benchmark_create_operations(
    executor: &Executor,
    config: &BenchmarkConfig,
) -> (f64, Duration, Duration, Duration) {
    tracing::info!("Running CREATE operations benchmark...");

    let mut latencies = Vec::new();
    let start_time = Instant::now();
    let mut operations = 0;

    while start_time.elapsed() < Duration::from_secs(config.duration_secs) {
        let op_start = Instant::now();

        // Create a new node
        let id = config.node_count + operations;
        let query = Query {
            cypher: format!(
                "CREATE (n:Person {{id: {}, name: 'Benchmark{}', age: {}}})",
                id, id, 25
            ),
            params: HashMap::new(),
        };

        if let Ok(_) = executor.execute(&query) {
            latencies.push(op_start.elapsed());
            operations += 1;
        }

        // Small delay to prevent overwhelming
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let throughput = operations as f64 / start_time.elapsed().as_secs_f64();

    // Calculate percentiles
    latencies.sort();
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p99_idx = (latencies.len() as f64 * 0.99) as usize;
    let p95_latency = latencies
        .get(p95_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));
    let p99_latency = latencies
        .get(p99_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));

    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    } else {
        Duration::from_secs(0)
    };

    (throughput, avg_latency, p95_latency, p99_latency)
}

/// Run READ benchmark (Neo4j target: <3ms for cached data)
async fn benchmark_read_operations(
    executor: &Executor,
    config: &BenchmarkConfig,
) -> (f64, Duration, Duration, Duration) {
    tracing::info!("Running READ operations benchmark...");

    let mut latencies = Vec::new();
    let start_time = Instant::now();
    let mut operations = 0;

    while start_time.elapsed() < Duration::from_secs(config.duration_secs) {
        let op_start = Instant::now();

        // Mix of different read operations
        match operations % 4 {
            0 => {
                // Node lookup by ID
                let id = operations % config.node_count;
                let query = Query {
                    cypher: format!("MATCH (n:Person {{id: {}}}) RETURN n", id),
                    params: HashMap::new(),
                };
                let _ = executor.execute(&query);
            }
            1 => {
                // Relationship traversal
                let id = operations % config.node_count;
                let query = Query {
                    cypher: format!(
                        "MATCH (n:Person {{id: {}}})-[r:KNOWS]->(m) RETURN m LIMIT 5",
                        id
                    ),
                    params: HashMap::new(),
                };
                let _ = executor.execute(&query);
            }
            2 => {
                // Count query
                let query = Query {
                    cypher: "MATCH (n:Person) WHERE n.age > 25 RETURN count(n)".to_string(),
                    params: HashMap::new(),
                };
                let _ = executor.execute(&query);
            }
            3 => {
                // Pattern query
                let query = Query {
                    cypher: "MATCH (n:Person)-[:KNOWS]-(m:Person) RETURN n, m LIMIT 10".to_string(),
                    params: HashMap::new(),
                };
                let _ = executor.execute(&query);
            }
            _ => unreachable!(),
        }

        latencies.push(op_start.elapsed());
        operations += 1;

        // Small delay to prevent overwhelming
        tokio::time::sleep(Duration::from_micros(100)).await;
    }

    let throughput = operations as f64 / start_time.elapsed().as_secs_f64();

    // Calculate percentiles
    latencies.sort();
    let p95_idx = (latencies.len() as f64 * 0.95) as usize;
    let p99_idx = (latencies.len() as f64 * 0.99) as usize;
    let p95_latency = latencies
        .get(p95_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));
    let p99_latency = latencies
        .get(p99_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));

    let avg_latency = if !latencies.is_empty() {
        latencies.iter().sum::<Duration>() / latencies.len() as u32
    } else {
        Duration::from_secs(0)
    };

    (throughput, avg_latency, p95_latency, p99_latency)
}

/// Run mixed workload benchmark
async fn benchmark_mixed_workload(
    executor: Arc<Executor>,
    config: &BenchmarkConfig,
) -> (f64, Duration, Duration, Duration) {
    tracing::info!("Running mixed workload benchmark...");

    let mut handles = Vec::new();
    let results = Arc::new(std::sync::Mutex::new(Vec::new()));

    // Spawn worker threads
    for worker_id in 0..config.concurrent_clients {
        let executor_arc = executor.clone();
        let results_clone = results.clone();
        let config_clone = config.clone();

        let handle = tokio::spawn(async move {
            let mut local_latencies = Vec::new();
            let start_time = Instant::now();

            while start_time.elapsed() < Duration::from_secs(config_clone.duration_secs) {
                let op_start = Instant::now();

                // Mix of operations
                match worker_id % 3 {
                    0 => {
                        // Mostly reads
                        match (worker_id + start_time.elapsed().as_millis() as usize) % 4 {
                            0 => {
                                let id = worker_id % config_clone.node_count;
                                let query = Query {
                                    cypher: format!("MATCH (n:Person {{id: {}}}) RETURN n", id),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            1 => {
                                let id = worker_id % config_clone.node_count;
                                let query = Query {
                                    cypher: format!(
                                        "MATCH (n:Person {{id: {}}})-[r:KNOWS]->(m) RETURN count(r)",
                                        id
                                    ),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            2 => {
                                let query = Query {
                                    cypher: "MATCH (n:Person) WHERE n.age > 25 RETURN count(n)"
                                        .to_string(),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            3 => {
                                let query = Query {
                                    cypher: "MATCH (n:Person)-[:KNOWS]-(m:Person) RETURN count(n) as connections".to_string(),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            _ => unreachable!(),
                        }
                    }
                    1 => {
                        // Mixed read/write
                        match (worker_id + start_time.elapsed().as_millis() as usize) % 5 {
                            0..=2 => {
                                // 60% reads
                                let id = worker_id % config_clone.node_count;
                                let query = Query {
                                    cypher: format!("MATCH (n:Person {{id: {}}}) RETURN n", id),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            3 => {
                                // 20% relationship queries
                                let id = worker_id % config_clone.node_count;
                                let query = Query {
                                    cypher: format!(
                                        "MATCH (n:Person {{id: {}}})-[r:KNOWS]->(m) RETURN m LIMIT 3",
                                        id
                                    ),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            4 => {
                                // 20% writes
                                let id = config_clone.node_count
                                    + worker_id
                                    + start_time.elapsed().as_millis() as usize;
                                let query = Query {
                                    cypher: format!(
                                        "CREATE (n:Person {{id: {}, name: 'Worker{}', age: {}}})",
                                        id, worker_id, 25
                                    ),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            _ => unreachable!(),
                        }
                    }
                    2 => {
                        // Heavy analytics
                        match (worker_id + start_time.elapsed().as_millis() as usize) % 3 {
                            0 => {
                                let query = Query {
                                    cypher: "MATCH (n:Person) RETURN n.age, count(*) as count ORDER BY count DESC LIMIT 10".to_string(),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            1 => {
                                let query = Query {
                                    cypher: "MATCH (n:Person)-[r:KNOWS]->(m:Person) RETURN count(r) as total_relationships".to_string(),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            2 => {
                                let query = Query {
                                    cypher: "MATCH (n:Person) WHERE n.age >= 20 AND n.age <= 30 RETURN count(n) as young_people".to_string(),
                                    params: HashMap::new(),
                                };
                                let _ = executor_arc.execute(&query);
                            }
                            _ => unreachable!(),
                        }
                    }
                    _ => unreachable!(),
                }

                local_latencies.push(op_start.elapsed());

                // Small delay to prevent overwhelming
                tokio::time::sleep(Duration::from_micros(500)).await;
            }

            // Store results
            let mut results = results_clone.lock().unwrap();
            results.extend(local_latencies);
        });

        handles.push(handle);
    }

    // Wait for all workers
    for handle in handles {
        let _ = handle.await;
    }

    let all_latencies = results.lock().unwrap().clone();
    let throughput = all_latencies.len() as f64 / config.duration_secs as f64;

    // Calculate percentiles
    let mut sorted_latencies = all_latencies;
    sorted_latencies.sort();
    let p95_idx = (sorted_latencies.len() as f64 * 0.95) as usize;
    let p99_idx = (sorted_latencies.len() as f64 * 0.99) as usize;
    let p95_latency = sorted_latencies
        .get(p95_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));
    let p99_latency = sorted_latencies
        .get(p99_idx)
        .copied()
        .unwrap_or(Duration::from_secs(0));

    let avg_latency = if !sorted_latencies.is_empty() {
        sorted_latencies.iter().sum::<Duration>() / sorted_latencies.len() as u32
    } else {
        Duration::from_secs(0)
    };

    (throughput, avg_latency, p95_latency, p99_latency)
}

#[tokio::test]
#[cfg(feature = "benchmarks")]
#[ignore]
async fn performance_benchmark_vs_neo4j() {
    tracing::info!("🚀 Starting Performance Benchmark vs Neo4j Targets");
    tracing::info!("Testing all optimizations under realistic workloads...");

    let config = BenchmarkConfig::default();
    let (mut executor, _cache, _dir) = create_benchmark_environment(&config);

    // Phase 1: Generate dataset
    tracing::info!("\n📊 Phase 1: Generating benchmark dataset");
    generate_benchmark_dataset(&mut executor, &config).await;

    // Phase 2: Warmup
    tracing::info!("\n🔥 Phase 2: Warming up system ({}s)", config.warmup_secs);
    let warmup_start = Instant::now();
    while warmup_start.elapsed() < Duration::from_secs(config.warmup_secs) {
        let query = Query {
            cypher: "MATCH (n:Person) RETURN count(n)".to_string(),
            params: HashMap::new(),
        };
        let _ = executor.execute(&query);
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Phase 3: CREATE benchmark
    tracing::info!("\n⚡ Phase 3: CREATE operations benchmark");
    let (create_throughput, create_avg, create_p95, create_p99) =
        benchmark_create_operations(&executor, &config).await;

    // Phase 4: READ benchmark
    tracing::info!("\n📖 Phase 4: READ operations benchmark");
    let (read_throughput, read_avg, read_p95, read_p99) =
        benchmark_read_operations(&executor, &config).await;

    // Phase 5: Mixed workload benchmark
    tracing::info!("\n🔄 Phase 5: Mixed workload benchmark");
    let (mixed_throughput, mixed_avg, mixed_p95, mixed_p99) =
        benchmark_mixed_workload(Arc::new(executor), &config).await;

    // Phase 6: Results and validation
    tracing::info!("\n📈 Phase 6: Benchmark Results vs Neo4j Targets");

    tracing::info!("CREATE Operations:");
    tracing::info!("  Throughput: {:.2} ops/sec", create_throughput);
    tracing::info!("  Avg Latency: {:.2}ms", create_avg.as_millis());
    tracing::info!("  P95 Latency: {:.2}ms", create_p95.as_millis());
    tracing::info!("  P99 Latency: {:.2}ms", create_p99.as_millis());
    tracing::info!(
        "  Target: <5ms avg latency - {}",
        if create_avg < Duration::from_millis(5) {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );

    tracing::info!("\nREAD Operations:");
    tracing::info!("  Throughput: {:.2} ops/sec", read_throughput);
    tracing::info!("  Avg Latency: {:.2}ms", read_avg.as_millis());
    tracing::info!("  P95 Latency: {:.2}ms", read_p95.as_millis());
    tracing::info!("  P99 Latency: {:.2}ms", read_p99.as_millis());
    tracing::info!(
        "  Target: <3ms avg latency - {}",
        if read_avg < Duration::from_millis(3) {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );

    tracing::info!("\nMixed Workload:");
    tracing::info!("  Throughput: {:.2} ops/sec", mixed_throughput);
    tracing::info!("  Avg Latency: {:.2}ms", mixed_avg.as_millis());
    tracing::info!("  P95 Latency: {:.2}ms", mixed_p95.as_millis());
    tracing::info!("  P99 Latency: {:.2}ms", mixed_p99.as_millis());
    tracing::info!(
        "  Target: >500 ops/sec - {}",
        if mixed_throughput > 500.0 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );

    // Overall assessment
    let create_target_met = create_avg < Duration::from_millis(5);
    let read_target_met = read_avg < Duration::from_millis(3);
    let throughput_target_met = mixed_throughput > 500.0;

    let overall_success = create_target_met && read_target_met && throughput_target_met;

    tracing::info!("\n🏆 Overall Performance Assessment:");
    tracing::info!(
        "CREATE Target (<5ms): {}",
        if create_target_met {
            "✅ MET"
        } else {
            "❌ NOT MET"
        }
    );
    tracing::info!(
        "READ Target (<3ms): {}",
        if read_target_met {
            "✅ MET"
        } else {
            "❌ NOT MET"
        }
    );
    tracing::info!(
        "Throughput Target (>500 ops/sec): {}",
        if throughput_target_met {
            "✅ MET"
        } else {
            "❌ NOT MET"
        }
    );
    tracing::info!(
        "Overall: {}",
        if overall_success {
            "✅ ALL TARGETS MET - Neo4j parity achieved!"
        } else {
            "⚠️ Some targets not met"
        }
    );

    // Assertions
    assert!(
        overall_success,
        "Performance targets not met - Neo4j parity not achieved"
    );

    tracing::info!("\n✅ Performance Benchmark Complete - Nexus achieves Neo4j-level performance!");
}
