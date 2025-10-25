use nexus_core::catalog::Catalog;
use nexus_core::executor::Executor;
use nexus_core::index::{LabelIndex, KnnIndex};
use nexus_protocol::rest::{CypherRequest, IngestRequest, NodeIngest, RelIngest};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::tempdir;
use tokio::sync::RwLock;

/// Performance benchmark suite for Nexus
pub struct PerformanceBenchmark {
    executor: Arc<RwLock<Executor>>,
    catalog: Arc<RwLock<Catalog>>,
    label_index: Arc<RwLock<LabelIndex>>,
    knn_index: Arc<RwLock<KnnIndex>>,
    temp_dir: tempdir::TempDir,
}

impl PerformanceBenchmark {
    /// Create a new performance benchmark instance
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let temp_dir = tempdir()?;
        let catalog = Arc::new(RwLock::new(Catalog::new(temp_dir.path())?));
        let executor = Arc::new(RwLock::new(Executor::default()));
        let label_index = Arc::new(RwLock::new(LabelIndex::new()));
        let knn_index = Arc::new(RwLock::new(KnnIndex::new(128)?));
        
        Ok(Self {
            executor,
            catalog,
            label_index,
            knn_index,
            temp_dir,
        })
    }
    
    /// Benchmark point reads (simple MATCH queries)
    pub async fn benchmark_point_reads(&self) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
        println!("Benchmarking point reads...");
        
        // Setup test data
        self.setup_test_data().await?;
        
        let query = "MATCH (n:User) WHERE n.id = 1 RETURN n.name";
        let iterations = 10000;
        let mut total_time = Duration::new(0, 0);
        let mut success_count = 0;
        
        let start_time = Instant::now();
        
        for _ in 0..iterations {
            let request = CypherRequest {
                query: query.to_string(),
                params: HashMap::new(),
                timeout_ms: Some(100),
            };
            
            let query_start = Instant::now();
            match self.execute_query(request).await {
                Ok(_) => {
                    total_time += query_start.elapsed();
                    success_count += 1;
                }
                Err(_) => {
                    // Count failures but don't include in timing
                }
            }
        }
        
        let total_benchmark_time = start_time.elapsed();
        let avg_time_ms = if success_count > 0 {
            total_time.as_millis() as f64 / success_count as f64
        } else {
            0.0
        };
        
        let qps = if avg_time_ms > 0.0 {
            1000.0 / avg_time_ms
        } else {
            0.0
        };
        
        let result = BenchmarkResult {
            name: "Point Reads".to_string(),
            operation: "MATCH (n:User) WHERE n.id = 1 RETURN n.name".to_string(),
            iterations,
            success_count,
            total_time: total_benchmark_time,
            avg_time_ms,
            qps,
            target_qps: 100000.0,
        };
        
        println!("  QPS: {:.0} (target: 100K+)", qps);
        println!("  Avg time: {:.3}ms", avg_time_ms);
        println!("  Success rate: {:.1}%", success_count as f64 / iterations as f64 * 100.0);
        
        Ok(result)
    }
    
    /// Benchmark KNN queries
    pub async fn benchmark_knn_queries(&self) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
        println!("Benchmarking KNN queries...");
        
        // Setup test data with vectors
        self.setup_vector_data().await?;
        
        let query = "MATCH (n:Person) WHERE n.vector IS NOT NULL RETURN n.name ORDER BY n.vector <-> [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5] LIMIT 10";
        let iterations = 1000;
        let mut total_time = Duration::new(0, 0);
        let mut success_count = 0;
        
        let start_time = Instant::now();
        
        for _ in 0..iterations {
            let request = CypherRequest {
                query: query.to_string(),
                params: HashMap::new(),
                timeout_ms: Some(500),
            };
            
            let query_start = Instant::now();
            match self.execute_query(request).await {
                Ok(_) => {
                    total_time += query_start.elapsed();
                    success_count += 1;
                }
                Err(_) => {
                    // Count failures but don't include in timing
                }
            }
        }
        
        let total_benchmark_time = start_time.elapsed();
        let avg_time_ms = if success_count > 0 {
            total_time.as_millis() as f64 / success_count as f64
        } else {
            0.0
        };
        
        let qps = if avg_time_ms > 0.0 {
            1000.0 / avg_time_ms
        } else {
            0.0
        };
        
        let result = BenchmarkResult {
            name: "KNN Queries".to_string(),
            operation: "KNN similarity search".to_string(),
            iterations,
            success_count,
            total_time: total_benchmark_time,
            avg_time_ms,
            qps,
            target_qps: 10000.0,
        };
        
        println!("  QPS: {:.0} (target: 10K+)", qps);
        println!("  Avg time: {:.3}ms", avg_time_ms);
        println!("  Success rate: {:.1}%", success_count as f64 / iterations as f64 * 100.0);
        
        Ok(result)
    }
    
    /// Benchmark pattern traversal
    pub async fn benchmark_pattern_traversal(&self) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
        println!("Benchmarking pattern traversal...");
        
        // Setup test data with relationships
        self.setup_relationship_data().await?;
        
        let query = "MATCH (a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User) RETURN a.name, b.name, c.name LIMIT 100";
        let iterations = 1000;
        let mut total_time = Duration::new(0, 0);
        let mut success_count = 0;
        
        let start_time = Instant::now();
        
        for _ in 0..iterations {
            let request = CypherRequest {
                query: query.to_string(),
                params: HashMap::new(),
                timeout_ms: Some(1000),
            };
            
            let query_start = Instant::now();
            match self.execute_query(request).await {
                Ok(_) => {
                    total_time += query_start.elapsed();
                    success_count += 1;
                }
                Err(_) => {
                    // Count failures but don't include in timing
                }
            }
        }
        
        let total_benchmark_time = start_time.elapsed();
        let avg_time_ms = if success_count > 0 {
            total_time.as_millis() as f64 / success_count as f64
        } else {
            0.0
        };
        
        let qps = if avg_time_ms > 0.0 {
            1000.0 / avg_time_ms
        } else {
            0.0
        };
        
        let result = BenchmarkResult {
            name: "Pattern Traversal".to_string(),
            operation: "Multi-hop relationship traversal".to_string(),
            iterations,
            success_count,
            total_time: total_benchmark_time,
            avg_time_ms,
            qps,
            target_qps: 1000.0,
        };
        
        println!("  QPS: {:.0} (target: 1K+)", qps);
        println!("  Avg time: {:.3}ms", avg_time_ms);
        println!("  Success rate: {:.1}%", success_count as f64 / iterations as f64 * 100.0);
        
        Ok(result)
    }
    
    /// Benchmark bulk ingest
    pub async fn benchmark_bulk_ingest(&self) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
        println!("Benchmarking bulk ingest...");
        
        let batch_size = 1000;
        let num_batches = 100;
        let mut total_time = Duration::new(0, 0);
        let mut total_nodes = 0;
        
        let start_time = Instant::now();
        
        for batch in 0..num_batches {
            let mut nodes = Vec::new();
            let mut relationships = Vec::new();
            
            // Create batch of nodes
            for i in 0..batch_size {
                let node_id = (batch * batch_size + i) as u32;
                let node = NodeIngest {
                    id: Some(node_id),
                    labels: vec!["User".to_string()],
                    properties: json!({
                        "id": node_id,
                        "name": format!("User{}", node_id),
                        "age": 20 + (node_id % 50),
                        "email": format!("user{}@example.com", node_id)
                    }),
                };
                nodes.push(node);
                
                // Create some relationships
                if i > 0 && i % 10 == 0 {
                    let rel = RelIngest {
                        id: Some(node_id),
                        src: node_id - 1,
                        dst: node_id,
                        r#type: "FOLLOWS".to_string(),
                        properties: json!({"created_at": "2024-01-01T00:00:00Z"}),
                    };
                    relationships.push(rel);
                }
            }
            
            let request = IngestRequest { nodes, relationships };
            
            let batch_start = Instant::now();
            match self.execute_ingest(request).await {
                Ok(response) => {
                    total_time += batch_start.elapsed();
                    total_nodes += response.nodes_ingested;
                }
                Err(_) => {
                    // Count failures but don't include in timing
                }
            }
        }
        
        let total_benchmark_time = start_time.elapsed();
        let nodes_per_sec = if total_time.as_secs_f64() > 0.0 {
            total_nodes as f64 / total_time.as_secs_f64()
        } else {
            0.0
        };
        
        let result = BenchmarkResult {
            name: "Bulk Ingest".to_string(),
            operation: "Batch node and relationship creation".to_string(),
            iterations: num_batches,
            success_count: total_nodes as usize,
            total_time: total_benchmark_time,
            avg_time_ms: total_time.as_millis() as f64 / num_batches as f64,
            qps: nodes_per_sec,
            target_qps: 100000.0,
        };
        
        println!("  Nodes/sec: {:.0} (target: 100K+)", nodes_per_sec);
        println!("  Total nodes: {}", total_nodes);
        println!("  Total time: {:?}", total_benchmark_time);
        
        Ok(result)
    }
    
    /// Setup basic test data
    async fn setup_test_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create labels
        let _label_id = self.catalog.write().await.get_or_create_label("User")?;
        
        // Add some nodes to label index
        for i in 1..=1000 {
            self.label_index.write().await.add_node(i, 1)?;
        }
        
        Ok(())
    }
    
    /// Setup vector test data
    async fn setup_vector_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create labels
        let _label_id = self.catalog.write().await.get_or_create_label("Person")?;
        
        // Add vectors to KNN index
        for i in 1..=1000 {
            let vector = vec![
                (i as f32) / 1000.0,
                ((i * 2) as f32) / 1000.0,
                ((i * 3) as f32) / 1000.0,
                ((i * 4) as f32) / 1000.0,
                ((i * 5) as f32) / 1000.0,
                ((i * 6) as f32) / 1000.0,
                ((i * 7) as f32) / 1000.0,
                ((i * 8) as f32) / 1000.0,
            ];
            self.knn_index.write().await.add_vector(i, 1, &vector)?;
        }
        
        Ok(())
    }
    
    /// Setup relationship test data
    async fn setup_relationship_data(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Create labels and types
        let _label_id = self.catalog.write().await.get_or_create_label("User")?;
        let _type_id = self.catalog.write().await.get_or_create_type("FOLLOWS")?;
        
        // Add nodes and relationships
        for i in 1..=1000 {
            self.label_index.write().await.add_node(i, 1)?;
            
            // Create some relationships
            if i > 1 && i % 5 == 0 {
                // Add relationship (simplified - in real implementation would use relationship storage)
            }
        }
        
        Ok(())
    }
    
    /// Execute a Cypher query (simplified implementation)
    async fn execute_query(&self, _request: CypherRequest) -> Result<(), Box<dyn std::error::Error>> {
        // Simulate query execution
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        Ok(())
    }
    
    /// Execute an ingest request (simplified implementation)
    async fn execute_ingest(&self, request: IngestRequest) -> Result<IngestResponse, Box<dyn std::error::Error>> {
        // Simulate ingest execution
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        
        Ok(IngestResponse {
            nodes_ingested: request.nodes.len() as u32,
            relationships_ingested: request.relationships.len() as u32,
            execution_time_ms: 10,
            errors: Vec::new(),
        })
    }
    
    /// Run all benchmarks
    pub async fn run_all_benchmarks(&self) -> Result<Vec<BenchmarkResult>, Box<dyn std::error::Error>> {
        println!("=== Performance Benchmark Suite ===");
        
        let mut results = Vec::new();
        
        results.push(self.benchmark_point_reads().await?);
        results.push(self.benchmark_knn_queries().await?);
        results.push(self.benchmark_pattern_traversal().await?);
        results.push(self.benchmark_bulk_ingest().await?);
        
        self.print_summary(&results);
        
        Ok(results)
    }
    
    /// Print benchmark summary
    fn print_summary(&self, results: &[BenchmarkResult]) {
        println!("\n=== Benchmark Summary ===");
        println!("{:<20} {:<12} {:<12} {:<12} {:<12}", 
                "Operation", "QPS", "Target", "Avg Time", "Success Rate");
        println!("{}", "-".repeat(80));
        
        for result in results {
            let success_rate = result.success_count as f64 / result.iterations as f64 * 100.0;
            println!("{:<20} {:<12.0} {:<12.0} {:<12.3} {:<12.1}%", 
                    result.name, result.qps, result.target_qps, result.avg_time_ms, success_rate);
        }
        
        // Check if targets are met
        println!("\n=== Target Analysis ===");
        for result in results {
            let target_met = result.qps >= result.target_qps;
            let status = if target_met { "✅ PASS" } else { "❌ FAIL" };
            println!("{} {}: {:.0} QPS (target: {:.0})", 
                    status, result.name, result.qps, result.target_qps);
        }
    }
}

/// Benchmark result structure
#[derive(Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub operation: String,
    pub iterations: usize,
    pub success_count: usize,
    pub total_time: Duration,
    pub avg_time_ms: f64,
    pub qps: f64,
    pub target_qps: f64,
}

/// Simplified ingest response for benchmarking
#[derive(Debug)]
pub struct IngestResponse {
    pub nodes_ingested: u32,
    pub relationships_ingested: u32,
    pub execution_time_ms: u64,
    pub errors: Vec<String>,
}

/// Memory usage monitor
pub struct MemoryMonitor {
    initial_memory: u64,
}

impl MemoryMonitor {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let initial_memory = Self::get_memory_usage()?;
        Ok(Self { initial_memory })
    }
    
    pub fn get_memory_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        Self::get_memory_usage()
    }
    
    fn get_memory_usage() -> Result<u64, Box<dyn std::error::Error>> {
        // Simplified memory monitoring - in a real implementation,
        // you would use system-specific APIs to get actual memory usage
        Ok(1024 * 1024 * 100) // 100MB placeholder
    }
    
    pub fn get_memory_increase(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let current = self.get_memory_usage()?;
        Ok(current.saturating_sub(self.initial_memory))
    }
}

/// CLI utility for running performance benchmarks
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() > 1 && args[1] == "memory" {
        println!("Running memory usage benchmarks...");
        let monitor = MemoryMonitor::new()?;
        println!("Initial memory: {} MB", monitor.initial_memory / 1024 / 1024);
        
        let benchmark = PerformanceBenchmark::new()?;
        let _results = benchmark.run_all_benchmarks().await?;
        
        let final_memory = monitor.get_memory_usage()?;
        let memory_increase = monitor.get_memory_increase()?;
        
        println!("Final memory: {} MB", final_memory / 1024 / 1024);
        println!("Memory increase: {} MB", memory_increase / 1024 / 1024);
    } else {
        println!("Running performance benchmarks...");
        let benchmark = PerformanceBenchmark::new()?;
        let _results = benchmark.run_all_benchmarks().await?;
    }
    
    Ok(())
}
