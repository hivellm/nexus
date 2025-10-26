# Nexus Real Codebase Integration Tests

This directory contains comprehensive integration tests for the Nexus graph database using real datasets and realistic scenarios.

## Overview

The integration tests verify the complete system functionality across all components using real data:

- **Dataset Loading**: Load real datasets (Knowledge Graph, Social Network)
- **Cypher Queries**: Execute complex queries with real data
- **Vector Search**: Test KNN similarity search functionality
- **Performance**: Benchmark system performance with realistic workloads
- **Stress Testing**: Test concurrent operations and system limits
- **Error Handling**: Validate error handling and edge cases
- **Data Consistency**: Ensure data integrity and consistency

## Test Structure

### Core Test Files

- `tests/real_codebase_integration_test.rs` - Main integration test suite
- `tests/api_integration_test.rs` - API endpoint integration tests
- `examples/real_codebase_test_runner.rs` - Comprehensive test runner
- `examples/cypher_test_runner.rs` - Cypher query test executor
- `examples/dataset_loader.rs` - Dataset loading utilities

### Configuration Files

- `examples/integration_test_config.json` - Test configuration and parameters
- `examples/cypher_tests/test_suite.json` - Cypher query test definitions

### Test Datasets

- `examples/datasets/knowledge_graph.json` - Scientific entities and relationships
- `examples/datasets/social_network.json` - Social network data with users and posts

### Test Scripts

- `examples/run_integration_tests.sh` - Main test execution script

## Quick Start

### Prerequisites

1. **Start the Nexus Server**:
   ```bash
   cargo run --bin nexus-server
   ```

2. **Ensure Datasets Exist**:
   ```bash
   ls examples/datasets/
   # Should show: knowledge_graph.json, social_network.json
   ```

### Running Tests

#### Option 1: Run All Tests (Recommended)
```bash
./examples/run_integration_tests.sh
```

#### Option 2: Run Specific Test Suites
```bash
# Run Rust integration tests
cargo test --test real_codebase_integration_test

# Run API integration tests
cargo test --test api_integration_test

# Run comprehensive test suite
cargo run --example real_codebase_test_runner -- http://localhost:3000

# Run Cypher test suite
cargo run --example cypher_test_runner -- http://localhost:3000
```

#### Option 3: Run with Custom Server URL
```bash
./examples/run_integration_tests.sh http://localhost:8080
```

## Test Categories

### 1. Dataset Loading Tests

Tests the loading of real datasets into the graph database:

- **Knowledge Graph Dataset**: Scientific entities with vector embeddings
- **Social Network Dataset**: Users, posts, and social relationships
- **Data Validation**: Verify loaded data integrity and statistics

**Example Test**:
```rust
#[tokio::test]
async fn test_knowledge_graph_dataset_loading() {
    let server = RealDataTestServer::new().await.unwrap();
    let stats = server.load_dataset(Path::new("examples/datasets/knowledge_graph.json")).await.unwrap();
    
    assert!(stats.nodes_loaded > 0);
    assert!(stats.relationships_loaded > 0);
    assert!(stats.vectors_indexed > 0);
}
```

### 2. Cypher Query Tests

Tests Cypher query execution with real data:

- **Basic Queries**: Simple MATCH and RETURN operations
- **Filtering**: Label and property-based filtering
- **Relationships**: Traversal and pattern matching
- **Aggregations**: COUNT, AVG, and other aggregate functions
- **Complex Patterns**: Multi-hop traversals and complex patterns

**Example Test**:
```rust
#[tokio::test]
async fn test_knowledge_graph_cypher_queries() {
    let server = RealDataTestServer::new().await.unwrap();
    let _stats = server.load_dataset(Path::new("examples/datasets/knowledge_graph.json")).await.unwrap();
    
    let queries = vec![
        ("MATCH (n) RETURN count(n) as total_nodes", "count_query"),
        ("MATCH (n:Person) RETURN n.name, n.profession LIMIT 5", "person_query"),
        ("MATCH (a:Person)-[r:DEVELOPED]->(b:Concept) RETURN a.name, b.name LIMIT 5", "relationship_query"),
    ];
    
    for (query, description) in queries {
        let result = server.execute_cypher(query, HashMap::new()).await;
        assert!(result.is_ok());
    }
}
```

### 3. Vector Search Tests

Tests KNN similarity search functionality:

- **Person Similarity**: Find similar people based on vector embeddings
- **Concept Similarity**: Find related concepts using vector search
- **Performance**: Measure search performance and accuracy

**Example Test**:
```rust
#[tokio::test]
async fn test_knowledge_graph_vector_search() {
    let server = RealDataTestServer::new().await.unwrap();
    let _stats = server.load_dataset(Path::new("examples/datasets/knowledge_graph.json")).await.unwrap();
    
    let test_vector = vec![0.8, 0.6, 0.4, 0.9, 0.7, 0.3, 0.5, 0.8];
    let result = server.execute_knn_search("Person", test_vector, 5).await;
    
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response["status"], "success");
}
```

### 4. Performance Tests

Benchmarks system performance with realistic workloads:

- **Query Performance**: Measure query execution times
- **Throughput**: Test queries per second (QPS)
- **Concurrent Operations**: Test system under concurrent load
- **Memory Usage**: Monitor memory consumption with large datasets

**Example Test**:
```rust
#[tokio::test]
async fn test_performance_with_real_data() {
    let server = RealDataTestServer::new().await.unwrap();
    // Load datasets...
    
    let performance_tests = vec![
        ("Simple Match", "MATCH (n) RETURN n LIMIT 10", 1000),
        ("Label Filter", "MATCH (n:Person) RETURN n LIMIT 10", 500),
        ("Property Filter", "MATCH (n:User) WHERE n.age > 25 RETURN n LIMIT 10", 200),
    ];
    
    for (name, query, target_qps) in performance_tests {
        // Run performance test...
        assert!(actual_qps >= target_qps * 0.5); // Allow 50% of target
    }
}
```

### 5. Stress Tests

Tests system behavior under stress conditions:

- **Concurrent Queries**: Multiple simultaneous queries
- **Concurrent Ingestion**: Simultaneous data loading
- **Mixed Workloads**: Combined read/write operations
- **Resource Limits**: Test system behavior at limits

**Example Test**:
```rust
#[tokio::test]
async fn test_concurrent_queries_with_real_data() {
    let server = RealDataTestServer::new().await.unwrap();
    // Load dataset...
    
    let mut handles = vec![];
    let concurrent_requests = 20;
    
    for i in 0..concurrent_requests {
        let handle = tokio::spawn(async move {
            // Execute query...
        });
        handles.push(handle);
    }
    
    // Wait for all requests to complete
    for handle in handles {
        let (_, success) = handle.await.unwrap();
        assert!(success);
    }
}
```

### 6. Error Handling Tests

Tests error handling and edge cases:

- **Invalid Syntax**: Malformed Cypher queries
- **Non-existent Labels**: Queries with invalid labels
- **Timeout Handling**: Query timeout scenarios
- **Resource Exhaustion**: System behavior under resource constraints

**Example Test**:
```rust
#[tokio::test]
async fn test_error_handling_with_real_data() {
    let server = RealDataTestServer::new().await.unwrap();
    // Load dataset...
    
    let error_tests = vec![
        ("Invalid Syntax", "INVALID CYPHER SYNTAX", true),
        ("Non-existent Label", "MATCH (n:NonExistentLabel) RETURN n", false),
        ("Malformed Query", "MATCH (n RETURN n", true),
    ];
    
    for (description, query, should_fail) in error_tests {
        let result = server.execute_cypher(query, HashMap::new()).await;
        // Verify expected behavior...
    }
}
```

### 7. Data Consistency Tests

Tests data consistency and integrity:

- **Count Consistency**: Verify node and relationship counts
- **Label Distribution**: Check label distribution across nodes
- **Relationship Integrity**: Validate relationship consistency
- **Vector Index Consistency**: Ensure vector index accuracy

**Example Test**:
```rust
#[tokio::test]
async fn test_data_consistency_after_loading() {
    let server = RealDataTestServer::new().await.unwrap();
    let stats = server.load_dataset(Path::new("examples/datasets/knowledge_graph.json")).await.unwrap();
    
    let consistency_queries = vec![
        ("Node Count", "MATCH (n) RETURN count(n) as count"),
        ("Person Count", "MATCH (n:Person) RETURN count(n) as count"),
        ("Concept Count", "MATCH (n:Concept) RETURN count(n) as count"),
    ];
    
    for (description, query) in consistency_queries {
        let result = server.execute_cypher(query, HashMap::new()).await;
        // Verify counts are consistent...
    }
}
```

## Test Configuration

The test configuration is defined in `examples/integration_test_config.json`:

```json
{
  "name": "Nexus Real Codebase Integration Test Configuration",
  "test_suites": [
    {
      "name": "Dataset Loading Tests",
      "datasets": [
        {
          "name": "Knowledge Graph",
          "path": "examples/datasets/knowledge_graph.json",
          "expected_nodes": 50,
          "expected_relationships": 30
        }
      ]
    }
  ],
  "global_settings": {
    "default_timeout_seconds": 30,
    "max_retries": 3,
    "performance_threshold_factor": 0.5
  }
}
```

## Test Results and Reporting

### Test Output

Tests generate detailed output including:

- **Test Status**: Pass/Fail for each test
- **Execution Time**: Performance metrics
- **Error Details**: Detailed error information for failures
- **Statistics**: Data loading and query statistics

### Log Files

Test execution creates log files in `test_logs/`:

- `rust_tests.log` - Rust integration test output
- `api_tests.log` - API integration test output
- `comprehensive_tests.log` - Comprehensive test suite output
- `performance_benchmarks.log` - Performance benchmark results
- `cypher_tests.log` - Cypher test suite output

### Test Artifacts

Test artifacts are saved in `test_artifacts/`:

- `test_report.json` - Machine-readable test results
- `test_report.html` - Human-readable HTML report
- Performance metrics and statistics

## Continuous Integration

### GitHub Actions

The integration tests can be run in CI/CD pipelines:

```yaml
name: Integration Tests
on: [push, pull_request]
jobs:
  integration-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Setup Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: nightly
      - name: Start Nexus Server
        run: cargo run --bin nexus-server &
      - name: Wait for Server
        run: sleep 10
      - name: Run Integration Tests
        run: ./examples/run_integration_tests.sh
```

### Local Development

For local development, run tests in watch mode:

```bash
# Watch mode for Rust tests
cargo watch -x "test --test real_codebase_integration_test"

# Run specific test categories
cargo test --test real_codebase_integration_test test_knowledge_graph
```

## Troubleshooting

### Common Issues

1. **Server Not Running**:
   ```
   ERROR: Server is not running or not responding
   ```
   **Solution**: Start the Nexus server first: `cargo run --bin nexus-server`

2. **Missing Datasets**:
   ```
   WARNING: Missing: examples/datasets/knowledge_graph.json
   ```
   **Solution**: Ensure dataset files exist in the correct location

3. **Test Timeouts**:
   ```
   ERROR: Query timeout
   ```
   **Solution**: Increase timeout values in test configuration

4. **Memory Issues**:
   ```
   ERROR: Out of memory
   ```
   **Solution**: Reduce dataset size or increase system memory

### Debug Mode

Run tests with debug output:

```bash
RUST_LOG=debug cargo test --test real_codebase_integration_test
```

### Verbose Output

Enable verbose test output:

```bash
cargo test --test real_codebase_integration_test -- --nocapture
```

## Contributing

### Adding New Tests

1. **Create Test Function**:
   ```rust
   #[tokio::test]
   async fn test_new_functionality() {
       // Test implementation
   }
   ```

2. **Add to Test Suite**:
   Update the appropriate test category in the test runner

3. **Update Configuration**:
   Add test parameters to `integration_test_config.json`

### Test Guidelines

- **Use Real Data**: Always test with real datasets when possible
- **Test Edge Cases**: Include error conditions and edge cases
- **Measure Performance**: Include performance assertions
- **Document Tests**: Provide clear descriptions and expected behavior
- **Handle Failures Gracefully**: Tests should fail with clear error messages

## Performance Benchmarks

### Expected Performance

Based on the test configuration, expected performance metrics:

- **Simple Match**: > 1000 QPS
- **Label Filter**: > 500 QPS
- **Property Filter**: > 200 QPS
- **Aggregation**: > 100 QPS
- **Relationship Traversal**: > 50 QPS

### Performance Monitoring

Monitor performance metrics during test execution:

```bash
# Monitor system resources
htop

# Monitor memory usage
free -h

# Monitor disk I/O
iostat -x 1
```

## Security Considerations

### Test Data

- **No Sensitive Data**: Test datasets contain only synthetic data
- **Data Isolation**: Tests use temporary directories for data storage
- **Cleanup**: Test artifacts are cleaned up after execution

### Network Security

- **Local Testing**: Tests run against localhost by default
- **Authentication**: Tests assume no authentication is required
- **TLS**: Tests use HTTP by default (not HTTPS)

## Future Enhancements

### Planned Features

1. **Distributed Testing**: Test multi-node deployments
2. **Load Testing**: Large-scale performance testing
3. **Fault Tolerance**: Test system behavior under failures
4. **Security Testing**: Authentication and authorization tests
5. **Backup/Recovery**: Data backup and recovery testing

### Test Automation

1. **Scheduled Testing**: Automated test execution
2. **Performance Regression**: Detect performance regressions
3. **Test Reporting**: Enhanced reporting and visualization
4. **Test Metrics**: Track test coverage and quality metrics

## Support

For issues with integration tests:

1. **Check Logs**: Review test log files for error details
2. **Verify Prerequisites**: Ensure server is running and datasets exist
3. **Update Configuration**: Adjust timeout and performance settings
4. **Report Issues**: Create GitHub issues with test logs and configuration

## License

The integration tests are part of the Nexus project and follow the same license terms.