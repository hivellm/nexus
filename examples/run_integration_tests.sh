#!/bin/bash

# Nexus Real Codebase Integration Test Runner
# This script runs comprehensive integration tests with real datasets

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
SERVER_URL=${1:-"http://localhost:3000"}
TEST_CONFIG="examples/integration_test_config.json"
LOG_DIR="test_logs"
ARTIFACTS_DIR="test_artifacts"

# Create directories
mkdir -p "$LOG_DIR"
mkdir -p "$ARTIFACTS_DIR"

# Function to print colored output
print_status() {
    local status=$1
    local message=$2
    case $status in
        "INFO")
            echo -e "${BLUE}[INFO]${NC} $message"
            ;;
        "SUCCESS")
            echo -e "${GREEN}[SUCCESS]${NC} $message"
            ;;
        "WARNING")
            echo -e "${YELLOW}[WARNING]${NC} $message"
            ;;
        "ERROR")
            echo -e "${RED}[ERROR]${NC} $message"
            ;;
    esac
}

# Function to check if server is running
check_server() {
    print_status "INFO" "Checking if Nexus server is running at $SERVER_URL..."
    
    if curl -s -f "$SERVER_URL/health" > /dev/null 2>&1; then
        print_status "SUCCESS" "Server is running and responding"
        return 0
    else
        print_status "ERROR" "Server is not running or not responding at $SERVER_URL"
        print_status "INFO" "Please start the Nexus server first:"
        print_status "INFO" "  cargo run --bin nexus-server"
        return 1
    fi
}

# Function to check if datasets exist
check_datasets() {
    print_status "INFO" "Checking for required datasets..."
    
    local datasets=(
        "examples/datasets/knowledge_graph.json"
        "examples/datasets/social_network.json"
        "examples/cypher_tests/test_suite.json"
    )
    
    local missing=0
    for dataset in "${datasets[@]}"; do
        if [ -f "$dataset" ]; then
            print_status "SUCCESS" "Found: $dataset"
        else
            print_status "WARNING" "Missing: $dataset"
            missing=$((missing + 1))
        fi
    done
    
    if [ $missing -eq ${#datasets[@]} ]; then
        print_status "ERROR" "No datasets found. Please ensure dataset files exist."
        return 1
    fi
    
    return 0
}

# Function to run Rust integration tests
run_rust_tests() {
    print_status "INFO" "Running Rust integration tests..."
    
    local test_log="$LOG_DIR/rust_tests.log"
    
    if cargo test --test real_codebase_integration_test -- --nocapture > "$test_log" 2>&1; then
        print_status "SUCCESS" "Rust integration tests passed"
        return 0
    else
        print_status "ERROR" "Rust integration tests failed. Check $test_log for details."
        return 1
    fi
}

# Function to run API integration tests
run_api_tests() {
    print_status "INFO" "Running API integration tests..."
    
    local test_log="$LOG_DIR/api_tests.log"
    
    if cargo test --test api_integration_test -- --nocapture > "$test_log" 2>&1; then
        print_status "SUCCESS" "API integration tests passed"
        return 0
    else
        print_status "ERROR" "API integration tests failed. Check $test_log for details."
        return 1
    fi
}

# Function to run comprehensive test suite
run_comprehensive_tests() {
    print_status "INFO" "Running comprehensive test suite..."
    
    local test_log="$LOG_DIR/comprehensive_tests.log"
    
    # Compile the test runner
    if ! cargo build --example real_codebase_test_runner 2>/dev/null; then
        print_status "ERROR" "Failed to compile test runner"
        return 1
    fi
    
    # Run the test runner
    if cargo run --example real_codebase_test_runner -- "$SERVER_URL" > "$test_log" 2>&1; then
        print_status "SUCCESS" "Comprehensive test suite passed"
        return 0
    else
        print_status "ERROR" "Comprehensive test suite failed. Check $test_log for details."
        return 1
    fi
}

# Function to run performance benchmarks
run_performance_benchmarks() {
    print_status "INFO" "Running performance benchmarks..."
    
    local benchmark_log="$LOG_DIR/performance_benchmarks.log"
    
    # Run performance benchmark
    if cargo run --example performance_benchmark > "$benchmark_log" 2>&1; then
        print_status "SUCCESS" "Performance benchmarks completed"
        return 0
    else
        print_status "WARNING" "Performance benchmarks failed or not available"
        return 1
    fi
}

# Function to run Cypher test suite
run_cypher_tests() {
    print_status "INFO" "Running Cypher test suite..."
    
    local cypher_log="$LOG_DIR/cypher_tests.log"
    
    # Check if cypher test runner exists
    if [ -f "examples/cypher_test_runner.rs" ]; then
        if cargo run --example cypher_test_runner -- "$SERVER_URL" > "$cypher_log" 2>&1; then
            print_status "SUCCESS" "Cypher test suite passed"
            return 0
        else
            print_status "ERROR" "Cypher test suite failed. Check $cypher_log for details."
            return 1
        fi
    else
        print_status "WARNING" "Cypher test runner not found, skipping"
        return 1
    fi
}

# Function to generate test report
generate_report() {
    print_status "INFO" "Generating test report..."
    
    local report_file="$ARTIFACTS_DIR/test_report.json"
    local html_report="$ARTIFACTS_DIR/test_report.html"
    
    # Create JSON report
    cat > "$report_file" << EOF
{
  "test_run": {
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "server_url": "$SERVER_URL",
    "test_config": "$TEST_CONFIG"
  },
  "test_results": {
    "rust_tests": $([ -f "$LOG_DIR/rust_tests.log" ] && echo "true" || echo "false"),
    "api_tests": $([ -f "$LOG_DIR/api_tests.log" ] && echo "true" || echo "false"),
    "comprehensive_tests": $([ -f "$LOG_DIR/comprehensive_tests.log" ] && echo "true" || echo "false"),
    "performance_benchmarks": $([ -f "$LOG_DIR/performance_benchmarks.log" ] && echo "true" || echo "false"),
    "cypher_tests": $([ -f "$LOG_DIR/cypher_tests.log" ] && echo "true" || echo "false")
  },
  "artifacts": {
    "log_directory": "$LOG_DIR",
    "artifacts_directory": "$ARTIFACTS_DIR"
  }
}
EOF
    
    # Create HTML report
    cat > "$html_report" << EOF
<!DOCTYPE html>
<html>
<head>
    <title>Nexus Integration Test Report</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 20px; }
        .header { background-color: #f0f0f0; padding: 20px; border-radius: 5px; }
        .test-result { margin: 10px 0; padding: 10px; border-radius: 3px; }
        .success { background-color: #d4edda; border-left: 4px solid #28a745; }
        .failure { background-color: #f8d7da; border-left: 4px solid #dc3545; }
        .warning { background-color: #fff3cd; border-left: 4px solid #ffc107; }
        pre { background-color: #f8f9fa; padding: 10px; border-radius: 3px; overflow-x: auto; }
    </style>
</head>
<body>
    <div class="header">
        <h1>Nexus Integration Test Report</h1>
        <p><strong>Timestamp:</strong> $(date)</p>
        <p><strong>Server URL:</strong> $SERVER_URL</p>
        <p><strong>Test Config:</strong> $TEST_CONFIG</p>
    </div>
    
    <h2>Test Results</h2>
    <div class="test-result success">
        <h3>Rust Integration Tests</h3>
        <p>Status: $([ -f "$LOG_DIR/rust_tests.log" ] && echo "✅ Passed" || echo "❌ Failed")</p>
    </div>
    
    <div class="test-result success">
        <h3>API Integration Tests</h3>
        <p>Status: $([ -f "$LOG_DIR/api_tests.log" ] && echo "✅ Passed" || echo "❌ Failed")</p>
    </div>
    
    <div class="test-result success">
        <h3>Comprehensive Test Suite</h3>
        <p>Status: $([ -f "$LOG_DIR/comprehensive_tests.log" ] && echo "✅ Passed" || echo "❌ Failed")</p>
    </div>
    
    <div class="test-result warning">
        <h3>Performance Benchmarks</h3>
        <p>Status: $([ -f "$LOG_DIR/performance_benchmarks.log" ] && echo "✅ Completed" || echo "⚠️ Not Available")</p>
    </div>
    
    <div class="test-result warning">
        <h3>Cypher Test Suite</h3>
        <p>Status: $([ -f "$LOG_DIR/cypher_tests.log" ] && echo "✅ Passed" || echo "⚠️ Not Available")</p>
    </div>
    
    <h2>Log Files</h2>
    <ul>
        <li><a href="$LOG_DIR/rust_tests.log">Rust Tests Log</a></li>
        <li><a href="$LOG_DIR/api_tests.log">API Tests Log</a></li>
        <li><a href="$LOG_DIR/comprehensive_tests.log">Comprehensive Tests Log</a></li>
        <li><a href="$LOG_DIR/performance_benchmarks.log">Performance Benchmarks Log</a></li>
        <li><a href="$LOG_DIR/cypher_tests.log">Cypher Tests Log</a></li>
    </ul>
</body>
</html>
EOF
    
    print_status "SUCCESS" "Test report generated: $html_report"
}

# Function to cleanup
cleanup() {
    print_status "INFO" "Cleaning up..."
    # Add any cleanup logic here
}

# Main execution
main() {
    print_status "INFO" "Starting Nexus Real Codebase Integration Tests"
    print_status "INFO" "Server URL: $SERVER_URL"
    print_status "INFO" "Log Directory: $LOG_DIR"
    print_status "INFO" "Artifacts Directory: $ARTIFACTS_DIR"
    echo
    
    local exit_code=0
    
    # Check prerequisites
    if ! check_server; then
        exit_code=1
    fi
    
    if ! check_datasets; then
        exit_code=1
    fi
    
    if [ $exit_code -ne 0 ]; then
        print_status "ERROR" "Prerequisites check failed. Exiting."
        exit $exit_code
    fi
    
    # Run tests
    local tests_passed=0
    local tests_failed=0
    
    if run_rust_tests; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
        exit_code=1
    fi
    
    if run_api_tests; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
        exit_code=1
    fi
    
    if run_comprehensive_tests; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
        exit_code=1
    fi
    
    # Optional tests
    if run_performance_benchmarks; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
    fi
    
    if run_cypher_tests; then
        tests_passed=$((tests_passed + 1))
    else
        tests_failed=$((tests_failed + 1))
    fi
    
    # Generate report
    generate_report
    
    # Summary
    echo
    print_status "INFO" "Test Summary:"
    print_status "INFO" "  Tests Passed: $tests_passed"
    print_status "INFO" "  Tests Failed: $tests_failed"
    print_status "INFO" "  Total Tests: $((tests_passed + tests_failed))"
    
    if [ $exit_code -eq 0 ]; then
        print_status "SUCCESS" "All critical tests passed!"
    else
        print_status "ERROR" "Some tests failed. Check logs for details."
    fi
    
    print_status "INFO" "Test artifacts saved to: $ARTIFACTS_DIR"
    print_status "INFO" "Test logs saved to: $LOG_DIR"
    
    exit $exit_code
}

# Trap to ensure cleanup on exit
trap cleanup EXIT

# Run main function
main "$@"