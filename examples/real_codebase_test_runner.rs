//! Real Codebase Test Runner for Nexus Graph Database
//!
//! This utility runs comprehensive integration tests using real datasets:
//! - Loads real datasets (Knowledge Graph, Social Network)
//! - Executes Cypher test suites with real data
//! - Performs performance benchmarks
//! - Validates data consistency and integrity
//! - Tests error handling and edge cases

use nexus_core::{
    catalog::Catalog,
    executor::Executor,
    index::{KnnIndex, LabelIndex},
    storage::RecordStore,
};
use nexus_protocol::rest::{CypherRequest, IngestRequest, NodeIngest, RelIngest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing;

/// Comprehensive test runner for real codebase integration tests
pub struct RealCodebaseTestRunner {
    base_url: String,
    client: reqwest::Client,
    temp_dir: Option<TempDir>,
}

impl RealCodebaseTestRunner {
    /// Create a new test runner
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
            temp_dir: None,
        }
    }
    
    /// Run all integration tests with real datasets
    pub async fn run_all_tests(&mut self) -> Result<TestResults, Box<dyn std::error::Error>> {
        tracing::info!("🚀 Starting Real Codebase Integration Tests");
        tracing::info!("==========================================");
        
        let mut results = TestResults::new();
        
        // Check test environment
        self.check_test_environment().await?;
        
        // Test 1: Dataset Loading Tests
        tracing::info!("\n📊 Running Dataset Loading Tests...");
        let dataset_results = self.run_dataset_loading_tests().await?;
        results.add_category("Dataset Loading", dataset_results);
        
        // Test 2: Cypher Query Tests
        tracing::info!("\n🔍 Running Cypher Query Tests...");
        let cypher_results = self.run_cypher_query_tests().await?;
        results.add_category("Cypher Queries", cypher_results);
        
        // Test 3: Vector Search Tests
        tracing::info!("\n🎯 Running Vector Search Tests...");
        let vector_results = self.run_vector_search_tests().await?;
        results.add_category("Vector Search", vector_results);
        
        // Test 4: Performance Tests
        tracing::info!("\n⚡ Running Performance Tests...");
        let perf_results = self.run_performance_tests().await?;
        results.add_category("Performance", perf_results);
        
        // Test 5: Stress Tests
        tracing::info!("\n💪 Running Stress Tests...");
        let stress_results = self.run_stress_tests().await?;
        results.add_category("Stress Testing", stress_results);
        
        // Test 6: Error Handling Tests
        tracing::info!("\n🛡️ Running Error Handling Tests...");
        let error_results = self.run_error_handling_tests().await?;
        results.add_category("Error Handling", error_results);
        
        // Test 7: Data Consistency Tests
        tracing::info!("\n🔒 Running Data Consistency Tests...");
        let consistency_results = self.run_consistency_tests().await?;
        results.add_category("Data Consistency", consistency_results);
        
        results.print_summary();
        Ok(results)
    }
    
    /// Check test environment and dataset availability
    async fn check_test_environment(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("🔍 Checking test environment...");
        
        let datasets = vec![
            ("Knowledge Graph", "examples/datasets/knowledge_graph.json"),
            ("Social Network", "examples/datasets/social_network.json"),
            ("Cypher Test Suite", "examples/cypher_tests/test_suite.json"),
        ];
        
        let mut available_count = 0;
        
        for (name, path) in datasets {
            if Path::new(path).exists() {
                tracing::info!("  ✅ {}: Available", name);
                available_count += 1;
            } else {
                tracing::info!("  ❌ {}: Missing ({})", name, path);
            }
        }
        
        if available_count == 0 {
            return Err("No test datasets available. Please ensure dataset files exist.".into());
        }
        
        tracing::info!("  📊 {} out of {} datasets available", available_count, datasets.len());
        Ok(())
    }
    
    /// Run dataset loading tests
    async fn run_dataset_loading_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Dataset Loading");
        
        // Test Knowledge Graph loading
        if Path::new("examples/datasets/knowledge_graph.json").exists() {
            let test_result = self.test_dataset_loading("Knowledge Graph", "examples/datasets/knowledge_graph.json").await;
            category_results.add_test(test_result);
        }
        
        // Test Social Network loading
        if Path::new("examples/datasets/social_network.json").exists() {
            let test_result = self.test_dataset_loading("Social Network", "examples/datasets/social_network.json").await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Test loading a specific dataset
    async fn test_dataset_loading(&self, name: &str, path: &str) -> TestResult {
        let start_time = Instant::now();
        
        match self.load_dataset(path).await {
            Ok(stats) => {
                let duration = start_time.elapsed();
                TestResult {
                    name: format!("Load {}", name),
                    description: format!("Load {} dataset with {} nodes and {} relationships", 
                        name, stats.nodes_loaded, stats.relationships_loaded),
                    passed: stats.nodes_loaded > 0 && stats.relationships_loaded > 0,
                    execution_time_ms: Some(duration.as_millis() as u64),
                    details: Some(format!("Nodes: {}, Relationships: {}, Labels: {}, Types: {}", 
                        stats.nodes_loaded, stats.relationships_loaded, stats.labels_created, stats.types_created)),
                    error: None,
                }
            }
            Err(e) => {
                let duration = start_time.elapsed();
                TestResult {
                    name: format!("Load {}", name),
                    description: format!("Load {} dataset", name),
                    passed: false,
                    execution_time_ms: Some(duration.as_millis() as u64),
                    details: None,
                    error: Some(e.to_string()),
                }
            }
        }
    }
    
    /// Load a dataset via API
    async fn load_dataset(&self, path: &str) -> Result<DatasetStats, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let dataset: Value = serde_json::from_str(&content)?;
        
        let mut stats = DatasetStats {
            name: dataset["name"].as_str().unwrap_or("Unknown").to_string(),
            nodes_loaded: 0,
            relationships_loaded: 0,
            labels_created: 0,
            types_created: 0,
            vectors_indexed: 0,
        };
        
        // Load nodes
        if let Some(nodes) = dataset["nodes"].as_array() {
            let node_stats = self.load_nodes(nodes).await?;
            stats.nodes_loaded = node_stats.nodes_loaded;
            stats.labels_created = node_stats.labels_created;
            stats.vectors_indexed = node_stats.vectors_indexed;
        }
        
        // Load relationships
        if let Some(relationships) = dataset["relationships"].as_array() {
            let rel_stats = self.load_relationships(relationships).await?;
            stats.relationships_loaded = rel_stats.relationships_loaded;
            stats.types_created = rel_stats.types_created;
        }
        
        Ok(stats)
    }
    
    /// Load nodes from dataset
    async fn load_nodes(&self, nodes: &[Value]) -> Result<DatasetStats, Box<dyn std::error::Error>> {
        let mut ingest_request = IngestRequest {
            nodes: Vec::new(),
            relationships: Vec::new(),
        };
        
        let mut stats = DatasetStats {
            name: "".to_string(),
            nodes_loaded: 0,
            relationships_loaded: 0,
            labels_created: 0,
            types_created: 0,
            vectors_indexed: 0,
        };
        
        for node in nodes {
            let labels = node["labels"]
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .map(|v| v.as_str().unwrap_or("").to_string())
                .collect::<Vec<String>>();
            
            let mut properties = node["properties"].as_object().unwrap_or(&serde_json::Map::new()).clone();
            
            // Extract vector if present
            if let Some(vector) = properties.get("vector") {
                if let Some(vector_array) = vector.as_array() {
                    let _vector_values: Result<Vec<f32>, _> = vector_array
                        .iter()
                        .map(|v| v.as_f64().map(|f| f as f32))
                        .collect::<Option<Vec<_>>>()
                        .ok_or("Invalid vector format")?;
                    
                    stats.vectors_indexed += 1;
                    properties.remove("vector");
                }
            }
            
            let node_ingest = NodeIngest {
                id: Some(node["id"].as_u64().unwrap_or(0) as u32),
                labels,
                properties: Value::Object(properties),
            };
            
            ingest_request.nodes.push(node_ingest);
            stats.nodes_loaded += 1;
        }
        
        // Execute ingestion via API
        self.execute_ingestion_via_api(ingest_request).await?;
        
        Ok(stats)
    }
    
    /// Load relationships from dataset
    async fn load_relationships(&self, relationships: &[Value]) -> Result<DatasetStats, Box<dyn std::error::Error>> {
        let mut ingest_request = IngestRequest {
            nodes: Vec::new(),
            relationships: Vec::new(),
        };
        
        let mut stats = DatasetStats {
            name: "".to_string(),
            nodes_loaded: 0,
            relationships_loaded: 0,
            labels_created: 0,
            types_created: 0,
            vectors_indexed: 0,
        };
        
        for rel in relationships {
            let rel_ingest = RelIngest {
                id: Some(rel["id"].as_u64().unwrap_or(0) as u32),
                src: rel["source"].as_u64().unwrap_or(0) as u32,
                dst: rel["target"].as_u64().unwrap_or(0) as u32,
                r#type: rel["type"].as_str().unwrap_or("").to_string(),
                properties: rel["properties"].clone(),
            };
            
            ingest_request.relationships.push(rel_ingest);
            stats.relationships_loaded += 1;
        }
        
        // Execute ingestion via API
        self.execute_ingestion_via_api(ingest_request).await?;
        
        Ok(stats)
    }
    
    /// Execute ingestion via API
    async fn execute_ingestion_via_api(&self, request: IngestRequest) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/ingest", self.base_url);
        
        let response = timeout(
            Duration::from_secs(30),
            self.client.post(&url).json(&request).send()
        ).await??;
        
        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("Ingestion failed: HTTP {} - {}", response.status(), error_text).into());
        }
        
        Ok(())
    }
    
    /// Run Cypher query tests
    async fn run_cypher_query_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Cypher Queries");
        
        // Load test suite if available
        if Path::new("examples/cypher_tests/test_suite.json").exists() {
            let test_suite_results = self.run_cypher_test_suite().await?;
            category_results.merge(test_suite_results);
        }
        
        // Run basic query tests
        let basic_queries = vec![
            ("Simple Match", "MATCH (n) RETURN count(n) as total"),
            ("Label Filter", "MATCH (n:Person) RETURN count(n) as persons"),
            ("Property Filter", "MATCH (n:User) WHERE n.age > 25 RETURN count(n) as adults"),
            ("Relationship Traversal", "MATCH (a:User)-[:FOLLOWS]->(b:User) RETURN count(a) as follows"),
        ];
        
        for (name, query) in basic_queries {
            let test_result = self.test_cypher_query(name, query).await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Run Cypher test suite
    async fn run_cypher_test_suite(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Cypher Test Suite");
        
        let content = std::fs::read_to_string("examples/cypher_tests/test_suite.json")?;
        let test_suite: Value = serde_json::from_str(&content)?;
        
        if let Some(categories) = test_suite["test_categories"].as_array() {
            for category in categories {
                let category_name = category["name"].as_str().unwrap_or("Unknown");
                
                if let Some(tests) = category["tests"].as_array() {
                    for test in tests {
                        let test_result = self.run_single_cypher_test(test).await?;
                        category_results.add_test(test_result);
                    }
                }
            }
        }
        
        Ok(category_results)
    }
    
    /// Run a single Cypher test
    async fn run_single_cypher_test(&self, test: &Value) -> Result<TestResult, Box<dyn std::error::Error>> {
        let name = test["name"].as_str().unwrap_or("unknown").to_string();
        let description = test["description"].as_str().unwrap_or("").to_string();
        let query = test["query"].as_str().unwrap_or("").to_string();
        let min_results = test["min_results"].as_u64().unwrap_or(0) as usize;
        let should_fail = test["should_fail"].as_bool().unwrap_or(false);
        
        let start_time = Instant::now();
        
        let request = CypherRequest {
            query: query.clone(),
            params: HashMap::new(),
            timeout_ms: Some(5000),
        };
        
        let response = self.execute_cypher_query(request).await;
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        let mut test_result = TestResult {
            name,
            description,
            passed: false,
            execution_time_ms: Some(execution_time),
            details: None,
            error: None,
        };
        
        match response {
            Ok(cypher_response) => {
                if should_fail {
                    test_result.error = Some("Expected query to fail but it succeeded".to_string());
                } else if cypher_response.results.len() >= min_results {
                    test_result.passed = true;
                    test_result.details = Some(format!("Returned {} results", cypher_response.results.len()));
                } else {
                    test_result.error = Some(format!(
                        "Expected at least {} results, got {}", 
                        min_results, cypher_response.results.len()
                    ));
                }
            }
            Err(e) => {
                if should_fail {
                    test_result.passed = true;
                    test_result.details = Some("Failed as expected".to_string());
                } else {
                    test_result.error = Some(e.to_string());
                }
            }
        }
        
        Ok(test_result)
    }
    
    /// Test a single Cypher query
    async fn test_cypher_query(&self, name: &str, query: &str) -> TestResult {
        let start_time = Instant::now();
        
        let request = CypherRequest {
            query: query.to_string(),
            params: HashMap::new(),
            timeout_ms: Some(5000),
        };
        
        let response = self.execute_cypher_query(request).await;
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        match response {
            Ok(cypher_response) => {
                TestResult {
                    name: name.to_string(),
                    description: format!("Execute query: {}", query),
                    passed: cypher_response.status == "success",
                    execution_time_ms: Some(execution_time),
                    details: Some(format!("Returned {} results", cypher_response.results.len())),
                    error: None,
                }
            }
            Err(e) => {
                TestResult {
                    name: name.to_string(),
                    description: format!("Execute query: {}", query),
                    passed: false,
                    execution_time_ms: Some(execution_time),
                    details: None,
                    error: Some(e.to_string()),
                }
            }
        }
    }
    
    /// Execute a Cypher query
    async fn execute_cypher_query(&self, request: CypherRequest) -> Result<CypherResponse, Box<dyn std::error::Error>> {
        let url = format!("{}/cypher", self.base_url);
        
        let response = timeout(
            Duration::from_millis(request.timeout_ms.unwrap_or(5000)),
            self.client.post(&url).json(&request).send()
        ).await??;
        
        if response.status().is_success() {
            let cypher_response: CypherResponse = response.json().await?;
            Ok(cypher_response)
        } else {
            let error_text = response.text().await?;
            Err(format!("HTTP {}: {}", response.status(), error_text).into())
        }
    }
    
    /// Run vector search tests
    async fn run_vector_search_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Vector Search");
        
        // Test vector search if knowledge graph is available
        if Path::new("examples/datasets/knowledge_graph.json").exists() {
            let test_result = self.test_vector_search().await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Test vector search functionality
    async fn test_vector_search(&self) -> TestResult {
        let start_time = Instant::now();
        
        let test_vector = vec![0.8, 0.6, 0.4, 0.9, 0.7, 0.3, 0.5, 0.8];
        
        let request_body = json!({
            "label": "Person",
            "vector": test_vector,
            "k": 5
        });
        
        let url = format!("{}/knn_traverse", self.base_url);
        
        let response = timeout(
            Duration::from_secs(10),
            self.client.post(&url).json(&request_body).send()
        ).await;
        
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        match response {
            Ok(Ok(resp)) if resp.status().is_success() => {
                let result: Value = resp.json().await.unwrap_or(json!({}));
                TestResult {
                    name: "Vector Search".to_string(),
                    description: "Test KNN vector similarity search".to_string(),
                    passed: result["status"] == "success",
                    execution_time_ms: Some(execution_time),
                    details: Some(format!("Status: {}", result["status"])),
                    error: None,
                }
            }
            Ok(Ok(resp)) => {
                let error_text = resp.text().await.unwrap_or("Unknown error".to_string());
                TestResult {
                    name: "Vector Search".to_string(),
                    description: "Test KNN vector similarity search".to_string(),
                    passed: false,
                    execution_time_ms: Some(execution_time),
                    details: None,
                    error: Some(format!("HTTP {}: {}", resp.status(), error_text)),
                }
            }
            Ok(Err(e)) => {
                TestResult {
                    name: "Vector Search".to_string(),
                    description: "Test KNN vector similarity search".to_string(),
                    passed: false,
                    execution_time_ms: Some(execution_time),
                    details: None,
                    error: Some(e.to_string()),
                }
            }
            Err(e) => {
                TestResult {
                    name: "Vector Search".to_string(),
                    description: "Test KNN vector similarity search".to_string(),
                    passed: false,
                    execution_time_ms: Some(execution_time),
                    details: None,
                    error: Some(format!("Timeout: {}", e)),
                }
            }
        }
    }
    
    /// Run performance tests
    async fn run_performance_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Performance");
        
        let performance_queries = vec![
            ("Simple Match", "MATCH (n) RETURN n LIMIT 10", 1000),
            ("Label Filter", "MATCH (n:Person) RETURN n LIMIT 10", 500),
            ("Property Filter", "MATCH (n:User) WHERE n.age > 25 RETURN n LIMIT 10", 200),
            ("Aggregation", "MATCH (n:User) RETURN COUNT(n), AVG(n.age)", 100),
        ];
        
        for (name, query, target_qps) in performance_queries {
            let test_result = self.test_performance(name, query, target_qps).await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Test performance of a query
    async fn test_performance(&self, name: &str, query: &str, target_qps: u32) -> TestResult {
        let iterations = 50;
        let mut total_time = Duration::new(0, 0);
        let mut success_count = 0;
        
        for _ in 0..iterations {
            let start = Instant::now();
            let request = CypherRequest {
                query: query.to_string(),
                params: HashMap::new(),
                timeout_ms: Some(1000),
            };
            
            match self.execute_cypher_query(request).await {
                Ok(_) => {
                    total_time += start.elapsed();
                    success_count += 1;
                }
                Err(_) => {
                    // Count failures but don't include in timing
                }
            }
        }
        
        let avg_time_ms = if success_count > 0 {
            total_time.as_millis() as f64 / success_count as f64
        } else {
            0.0
        };
        
        let actual_qps = if avg_time_ms > 0.0 {
            1000.0 / avg_time_ms
        } else {
            0.0
        };
        
        let success_rate = success_count as f64 / iterations as f64;
        let passed = actual_qps >= target_qps as f64 * 0.5; // Allow 50% of target
        
        TestResult {
            name: name.to_string(),
            description: format!("Performance test: {} (target: {} QPS)", query, target_qps),
            passed,
            execution_time_ms: Some(avg_time_ms as u64),
            details: Some(format!("Actual: {:.2} QPS, Success rate: {:.1}%", actual_qps, success_rate * 100.0)),
            error: if passed { None } else { Some(format!("Performance below target: {:.2} < {} QPS", actual_qps, target_qps)) },
        }
    }
    
    /// Run stress tests
    async fn run_stress_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Stress Testing");
        
        // Test concurrent queries
        let test_result = self.test_concurrent_queries().await;
        category_results.add_test(test_result);
        
        Ok(category_results)
    }
    
    /// Test concurrent query execution
    async fn test_concurrent_queries(&self) -> TestResult {
        let start_time = Instant::now();
        
        let queries = vec![
            "MATCH (n) RETURN count(n)",
            "MATCH (n:Person) RETURN count(n)",
            "MATCH (n:User) RETURN count(n)",
        ];
        
        let mut handles = vec![];
        let concurrent_requests = 20;
        
        for i in 0..concurrent_requests {
            let query = queries[i % queries.len()];
            let client = self.client.clone();
            let base_url = self.base_url.clone();
            
            let handle = tokio::spawn(async move {
                let request = CypherRequest {
                    query: query.to_string(),
                    params: HashMap::new(),
                    timeout_ms: Some(5000),
                };
                
                let url = format!("{}/cypher", base_url);
                let response = client.post(&url).json(&request).send().await;
                (i, response.is_ok())
            });
            
            handles.push(handle);
        }
        
        let mut success_count = 0;
        for handle in handles {
            let (_, success) = handle.await.unwrap();
            if success {
                success_count += 1;
            }
        }
        
        let execution_time = start_time.elapsed();
        let success_rate = success_count as f64 / concurrent_requests as f64;
        let passed = success_rate > 0.8; // 80% success rate
        
        TestResult {
            name: "Concurrent Queries".to_string(),
            description: format!("Execute {} concurrent queries", concurrent_requests),
            passed,
            execution_time_ms: Some(execution_time.as_millis() as u64),
            details: Some(format!("Success rate: {:.1}% ({}/{})", success_rate * 100.0, success_count, concurrent_requests)),
            error: if passed { None } else { Some(format!("Low success rate: {:.1}%", success_rate * 100.0)) },
        }
    }
    
    /// Run error handling tests
    async fn run_error_handling_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Error Handling");
        
        let error_queries = vec![
            ("Invalid Syntax", "INVALID CYPHER SYNTAX", true),
            ("Non-existent Label", "MATCH (n:NonExistentLabel) RETURN n", false),
            ("Malformed Query", "MATCH (n RETURN n", true),
        ];
        
        for (name, query, should_fail) in error_queries {
            let test_result = self.test_error_handling(name, query, should_fail).await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Test error handling for a query
    async fn test_error_handling(&self, name: &str, query: &str, should_fail: bool) -> TestResult {
        let start_time = Instant::now();
        
        let request = CypherRequest {
            query: query.to_string(),
            params: HashMap::new(),
            timeout_ms: Some(5000),
        };
        
        let response = self.execute_cypher_query(request).await;
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        match response {
            Ok(_) => {
                if should_fail {
                    TestResult {
                        name: name.to_string(),
                        description: format!("Error handling test: {}", query),
                        passed: false,
                        execution_time_ms: Some(execution_time),
                        details: None,
                        error: Some("Expected query to fail but it succeeded".to_string()),
                    }
                } else {
                    TestResult {
                        name: name.to_string(),
                        description: format!("Error handling test: {}", query),
                        passed: true,
                        execution_time_ms: Some(execution_time),
                        details: Some("Query succeeded as expected".to_string()),
                        error: None,
                    }
                }
            }
            Err(_) => {
                if should_fail {
                    TestResult {
                        name: name.to_string(),
                        description: format!("Error handling test: {}", query),
                        passed: true,
                        execution_time_ms: Some(execution_time),
                        details: Some("Query failed as expected".to_string()),
                        error: None,
                    }
                } else {
                    TestResult {
                        name: name.to_string(),
                        description: format!("Error handling test: {}", query),
                        passed: false,
                        execution_time_ms: Some(execution_time),
                        details: None,
                        error: Some("Query failed unexpectedly".to_string()),
                    }
                }
            }
        }
    }
    
    /// Run data consistency tests
    async fn run_consistency_tests(&self) -> Result<CategoryResults, Box<dyn std::error::Error>> {
        let mut category_results = CategoryResults::new("Data Consistency");
        
        let consistency_queries = vec![
            ("Node Count", "MATCH (n) RETURN count(n) as count"),
            ("Person Count", "MATCH (n:Person) RETURN count(n) as count"),
            ("User Count", "MATCH (n:User) RETURN count(n) as count"),
        ];
        
        for (name, query) in consistency_queries {
            let test_result = self.test_consistency(name, query).await;
            category_results.add_test(test_result);
        }
        
        Ok(category_results)
    }
    
    /// Test data consistency
    async fn test_consistency(&self, name: &str, query: &str) -> TestResult {
        let start_time = Instant::now();
        
        let request = CypherRequest {
            query: query.to_string(),
            params: HashMap::new(),
            timeout_ms: Some(5000),
        };
        
        let response = self.execute_cypher_query(request).await;
        let execution_time = start_time.elapsed().as_millis() as u64;
        
        match response {
            Ok(cypher_response) => {
                if let Some(first_row) = cypher_response.results.first() {
                    if let Some(count_value) = first_row.first() {
                        if let Some(count) = count_value.as_u64() {
                            TestResult {
                                name: name.to_string(),
                                description: format!("Consistency check: {}", query),
                                passed: count > 0,
                                execution_time_ms: Some(execution_time),
                                details: Some(format!("Count: {}", count)),
                                error: if count == 0 { Some("Count is zero".to_string()) } else { None },
                            }
                        } else {
                            TestResult {
                                name: name.to_string(),
                                description: format!("Consistency check: {}", query),
                                passed: false,
                                execution_time_ms: Some(execution_time),
                                details: None,
                                error: Some("Invalid count format".to_string()),
                            }
                        }
                    } else {
                        TestResult {
                            name: name.to_string(),
                            description: format!("Consistency check: {}", query),
                            passed: false,
                            execution_time_ms: Some(execution_time),
                            details: None,
                            error: Some("No count value returned".to_string()),
                        }
                    }
                } else {
                    TestResult {
                        name: name.to_string(),
                        description: format!("Consistency check: {}", query),
                        passed: false,
                        execution_time_ms: Some(execution_time),
                        details: None,
                        error: Some("No results returned".to_string()),
                    }
                }
            }
            Err(e) => {
                TestResult {
                    name: name.to_string(),
                    description: format!("Consistency check: {}", query),
                    passed: false,
                    execution_time_ms: Some(execution_time),
                    details: None,
                    error: Some(e.to_string()),
                }
            }
        }
    }
}

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Clone)]
struct DatasetStats {
    name: String,
    nodes_loaded: usize,
    relationships_loaded: usize,
    labels_created: usize,
    types_created: usize,
    vectors_indexed: usize,
}

#[derive(Debug, Clone)]
struct TestResult {
    name: String,
    description: String,
    passed: bool,
    execution_time_ms: Option<u64>,
    details: Option<String>,
    error: Option<String>,
}

#[derive(Debug)]
struct CategoryResults {
    name: String,
    tests: Vec<TestResult>,
}

impl CategoryResults {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tests: Vec::new(),
        }
    }
    
    fn add_test(&mut self, test: TestResult) {
        self.tests.push(test);
    }
    
    fn merge(&mut self, other: CategoryResults) {
        self.tests.extend(other.tests);
    }
    
    fn passed_count(&self) -> usize {
        self.tests.iter().filter(|t| t.passed).count()
    }
    
    fn total_count(&self) -> usize {
        self.tests.len()
    }
}

#[derive(Debug)]
struct TestResults {
    categories: Vec<CategoryResults>,
}

impl TestResults {
    fn new() -> Self {
        Self {
            categories: Vec::new(),
        }
    }
    
    fn add_category(&mut self, name: &str, category: CategoryResults) {
        self.categories.push(category);
    }
    
    fn total_tests(&self) -> usize {
        self.categories.iter().map(|c| c.total_count()).sum()
    }
    
    fn passed_tests(&self) -> usize {
        self.categories.iter().map(|c| c.passed_count()).sum()
    }
    
    fn failed_tests(&self) -> usize {
        self.total_tests() - self.passed_tests()
    }
    
    fn print_summary(&self) {
        tracing::info!("\n🎯 Test Results Summary");
        tracing::info!("========================");
        tracing::info!("Total tests: {}", self.total_tests());
        tracing::info!("Passed: {} ({:.1}%)", self.passed_tests(), 
            self.passed_tests() as f64 / self.total_tests() as f64 * 100.0);
        tracing::info!("Failed: {} ({:.1}%)", self.failed_tests(),
            self.failed_tests() as f64 / self.total_tests() as f64 * 100.0);
        
        tracing::info!("\n📊 Category Breakdown:");
        for category in &self.categories {
            let passed = category.passed_count();
            let total = category.total_count();
            let percentage = if total > 0 { passed as f64 / total as f64 * 100.0 } else { 0.0 };
            tracing::info!("  {}: {}/{} ({:.1}%)", category.name, passed, total, percentage);
        }
        
        // Print failed tests
        let failed_tests: Vec<_> = self.categories.iter()
            .flat_map(|c| &c.tests)
            .filter(|t| !t.passed)
            .collect();
        
        if !failed_tests.is_empty() {
            tracing::info!("\n❌ Failed Tests:");
            for test in failed_tests {
                tracing::info!("  - {}: {}", test.name, test.description);
                if let Some(error) = &test.error {
                    tracing::info!("    Error: {}", error);
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct CypherResponse {
    pub status: String,
    pub columns: Vec<String>,
    pub results: Vec<Vec<Value>>,
    pub execution_time_ms: u64,
}

// ============================================================================
// CLI Interface
// ============================================================================

/// CLI utility for running real codebase integration tests
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    let base_url = if args.len() > 1 {
        args[1].clone()
    } else {
        "http://localhost:3000".to_string()
    };
    
    tracing::info!("🚀 Nexus Real Codebase Integration Test Runner");
    tracing::info!("==============================================");
    tracing::info!("Server URL: {}", base_url);
    tracing::info!();
    
    let mut runner = RealCodebaseTestRunner::new(base_url);
    let results = runner.run_all_tests().await?;
    
    // Exit with appropriate code
    if results.failed_tests() > 0 {
        std::process::exit(1);
    } else {
        std::process::exit(0);
    }
}
