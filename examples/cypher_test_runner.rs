use nexus_protocol::rest::{CypherRequest, CypherResponse};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::time::timeout;
use tracing;

/// Cypher test executor for running test suites
pub struct CypherTestExecutor {
    base_url: String,
    client: reqwest::Client,
}

impl CypherTestExecutor {
    /// Create a new test executor
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
    
    /// Load and run a test suite from JSON file
    pub async fn run_test_suite(&self, test_suite_path: &Path) -> Result<TestSuiteResults, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(test_suite_path)?;
        let test_suite: Value = serde_json::from_str(&content)?;
        
        tracing::info!("Running test suite: {}", test_suite["name"].as_str().unwrap_or("Unknown"));
        tracing::info!("Description: {}", test_suite["description"].as_str().unwrap_or(""));
        
        let mut results = TestSuiteResults {
            suite_name: test_suite["name"].as_str().unwrap_or("Unknown").to_string(),
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            category_results: Vec::new(),
            performance_metrics: Vec::new(),
        };
        
        if let Some(categories) = test_suite["test_categories"].as_array() {
            for category in categories {
                let category_name = category["name"].as_str().unwrap_or("Unknown");
                tracing::info!("\n=== {} ===", category_name);
                
                let mut category_result = CategoryResults {
                    name: category_name.to_string(),
                    tests: Vec::new(),
                };
                
                if let Some(tests) = category["tests"].as_array() {
                    for test in tests {
                        let test_result = self.run_single_test(test).await?;
                        category_result.tests.push(test_result.clone());
                        
                        results.total_tests += 1;
                        if test_result.passed {
                            results.passed_tests += 1;
                        } else {
                            results.failed_tests += 1;
                        }
                        
                        // Print test result
                        let status = if test_result.passed { "✅ PASS" } else { "❌ FAIL" };
                        tracing::info!("  {} {} - {}", status, test_result.name, test_result.description);
                        
                        if !test_result.passed {
                            tracing::info!("    Error: {}", test_result.error.unwrap_or("Unknown error".to_string()));
                        }
                        
                        if let Some(execution_time) = test_result.execution_time_ms {
                            tracing::info!("    Execution time: {}ms", execution_time);
                        }
                    }
                }
                
                results.category_results.push(category_result);
            }
        }
        
        Ok(results)
    }
    
    /// Run a single test
    async fn run_single_test(&self, test: &Value) -> Result<TestResult, Box<dyn std::error::Error>> {
        let name = test["name"].as_str().unwrap_or("unknown").to_string();
        let description = test["description"].as_str().unwrap_or("").to_string();
        let query = test["query"].as_str().unwrap_or("").to_string();
        let expected_type = test["expected_result_type"].as_str().unwrap_or("unknown").to_string();
        let min_results = test["min_results"].as_u64().unwrap_or(0) as usize;
        let should_fail = test["should_fail"].as_bool().unwrap_or(false);
        let performance_target = test["performance_target_ms"].as_u64();
        
        let start_time = Instant::now();
        
        // Execute the query
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
            query,
            expected_result_type: expected_type,
            passed: false,
            execution_time_ms: Some(execution_time),
            result_count: 0,
            error: None,
        };
        
        match response {
            Ok(cypher_response) => {
                test_result.result_count = cypher_response.results.len();
                
                if should_fail {
                    // Test should have failed but didn't
                    test_result.error = Some("Expected query to fail but it succeeded".to_string());
                } else {
                    // Check if we got the minimum expected results
                    if test_result.result_count >= min_results {
                        test_result.passed = true;
                    } else {
                        test_result.error = Some(format!(
                            "Expected at least {} results, got {}", 
                            min_results, test_result.result_count
                        ));
                    }
                }
                
                // Check performance target if specified
                if let Some(target_ms) = performance_target {
                    if execution_time > target_ms {
                        test_result.error = Some(format!(
                            "Performance target exceeded: {}ms > {}ms", 
                            execution_time, target_ms
                        ));
                        test_result.passed = false;
                    }
                }
            }
            Err(e) => {
                if should_fail {
                    // Test was expected to fail and it did
                    test_result.passed = true;
                } else {
                    test_result.error = Some(e.to_string());
                }
            }
        }
        
        Ok(test_result)
    }
    
    /// Execute a Cypher query against the server
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
    
    /// Run performance benchmarks
    pub async fn run_performance_benchmarks(&self) -> Result<Vec<BenchmarkResult>, Box<dyn std::error::Error>> {
        let benchmarks = vec![
            ("simple_match", "MATCH (n) RETURN n LIMIT 10", 1000),
            ("aggregation", "MATCH (n:User) RETURN COUNT(n), AVG(n.age)", 500),
            ("knn_search", "MATCH (n:Person) WHERE n.vector IS NOT NULL RETURN n.name LIMIT 5", 100),
            ("complex_pattern", "MATCH (a:User)-[:FOLLOWS*2]->(b:User) RETURN a.name, b.name LIMIT 10", 50),
        ];
        
        let mut results = Vec::new();
        
        for (name, query, target_qps) in benchmarks {
            tracing::info!("Running benchmark: {}", name);
            
            let request = CypherRequest {
                query: query.to_string(),
                params: HashMap::new(),
                timeout_ms: Some(1000),
            };
            
            let iterations = 100;
            let mut total_time = Duration::new(0, 0);
            let mut success_count = 0;
            
            for _ in 0..iterations {
                let start = Instant::now();
                match self.execute_cypher_query(request.clone()).await {
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
            
            let result = BenchmarkResult {
                name: name.to_string(),
                query: query.to_string(),
                target_qps,
                actual_qps,
                avg_time_ms,
                success_rate: success_count as f64 / iterations as f64,
                iterations,
            };
            
            results.push(result);
            
            tracing::info!("  Target QPS: {}", target_qps);
            tracing::info!("  Actual QPS: {:.2}", actual_qps);
            tracing::info!("  Avg time: {:.2}ms", avg_time_ms);
            tracing::info!("  Success rate: {:.1}%", result.success_rate * 100.0);
        }
        
        Ok(results)
    }
}

/// Test result structures
#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub description: String,
    pub query: String,
    pub expected_result_type: String,
    pub passed: bool,
    pub execution_time_ms: Option<u64>,
    pub result_count: usize,
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct CategoryResults {
    pub name: String,
    pub tests: Vec<TestResult>,
}

#[derive(Debug)]
pub struct TestSuiteResults {
    pub suite_name: String,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub category_results: Vec<CategoryResults>,
    pub performance_metrics: Vec<BenchmarkResult>,
}

#[derive(Debug)]
pub struct BenchmarkResult {
    pub name: String,
    pub query: String,
    pub target_qps: u32,
    pub actual_qps: f64,
    pub avg_time_ms: f64,
    pub success_rate: f64,
    pub iterations: usize,
}

impl TestSuiteResults {
    /// Print a summary of test results
    pub fn print_summary(&self) {
        tracing::info!("\n=== Test Suite Summary ===");
        tracing::info!("Suite: {}", self.suite_name);
        tracing::info!("Total tests: {}", self.total_tests);
        tracing::info!("Passed: {} ({:.1}%)", self.passed_tests, 
                self.passed_tests as f64 / self.total_tests as f64 * 100.0);
        tracing::info!("Failed: {} ({:.1}%)", self.failed_tests,
                self.failed_tests as f64 / self.total_tests as f64 * 100.0);
        
        tracing::info!("\n=== Category Breakdown ===");
        for category in &self.category_results {
            let passed = category.tests.iter().filter(|t| t.passed).count();
            let total = category.tests.len();
            tracing::info!("{}: {}/{} passed", category.name, passed, total);
        }
    }
}

/// CLI utility for running Cypher tests
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() < 2 {
        etracing::info!("Usage: {} <server_url> [test_suite.json]", args[0]);
        etracing::info!("Example: {} http://localhost:3000 examples/cypher_tests/test_suite.json", args[0]);
        std::process::exit(1);
    }
    
    let server_url = args[1].clone();
    let test_suite_path = if args.len() > 2 {
        Path::new(&args[2])
    } else {
        Path::new("examples/cypher_tests/test_suite.json")
    };
    
    if !test_suite_path.exists() {
        etracing::info!("Test suite file not found: {}", test_suite_path.display());
        std::process::exit(1);
    }
    
    let executor = CypherTestExecutor::new(server_url);
    
    // Run test suite
    let results = executor.run_test_suite(test_suite_path).await?;
    results.print_summary();
    
    // Run performance benchmarks
    tracing::info!("\n=== Performance Benchmarks ===");
    let benchmarks = executor.run_performance_benchmarks().await?;
    
    for benchmark in benchmarks {
        tracing::info!("{}: {:.2} QPS (target: {})", 
                benchmark.name, benchmark.actual_qps, benchmark.target_qps);
    }
    
    Ok(())
}





