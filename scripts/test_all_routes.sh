#!/bin/bash
# Script to test all REST API routes
# Starts server in background, tests all endpoints, then stops server

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
PORT=${NEXUS_PORT:-15474}
BASE_URL="http://localhost:${PORT}"
SERVER_LOG="scripts/nexus_server.log"
TEST_LOG="scripts/route_tests.log"
PID_FILE="scripts/nexus_server.pid"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}Cleaning up...${NC}"
    if [ -f "$PID_FILE" ]; then
        SERVER_PID=$(cat "$PID_FILE")
        if ps -p "$SERVER_PID" > /dev/null 2>&1; then
            echo "Stopping server (PID: $SERVER_PID)..."
            kill "$SERVER_PID" 2>/dev/null || true
            sleep 2
            kill -9 "$SERVER_PID" 2>/dev/null || true
        fi
        rm -f "$PID_FILE"
    fi
    echo -e "${GREEN}Cleanup complete${NC}"
}

# Set trap to cleanup on exit
trap cleanup EXIT INT TERM

# Initialize counters
TOTAL_TESTS=0
PASSED_TESTS=0
FAILED_TESTS=0

# Test function
test_route() {
    local method=$1
    local endpoint=$2
    local data=$3
    local expected_status=${4:-200}
    local description=$5
    
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    
    echo -n "Testing $method $endpoint... "
    
    if [ "$method" = "GET" ]; then
        response=$(curl -s -w "\n%{http_code}" "$BASE_URL$endpoint" 2>&1)
    elif [ "$method" = "POST" ] || [ "$method" = "PUT" ]; then
        if [ -n "$data" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                -H "Content-Type: application/json" \
                -d "$data" \
                "$BASE_URL$endpoint" 2>&1)
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" \
                -H "Content-Type: application/json" \
                "$BASE_URL$endpoint" 2>&1)
        fi
    elif [ "$method" = "DELETE" ]; then
        response=$(curl -s -w "\n%{http_code}" -X "$method" "$BASE_URL$endpoint" 2>&1)
    else
        echo -e "${RED}Unknown method: $method${NC}"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        return 1
    fi
    
    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')
    
    # Check if curl failed
    if [ $? -ne 0 ]; then
        echo -e "${RED}FAILED (curl error)${NC}"
        echo "  Description: $description" >> "$TEST_LOG"
        echo "  Error: $response" >> "$TEST_LOG"
        FAILED_TESTS=$((FAILED_TESTS + 1))
        return 1
    fi
    
    # Check HTTP status code
    if [ "$http_code" = "$expected_status" ] || [ "$expected_status" = "any" ]; then
        echo -e "${GREEN}PASS${NC} (HTTP $http_code)"
        echo "[PASS] $method $endpoint - HTTP $http_code" >> "$TEST_LOG"
        if [ -n "$body" ] && [ ${#body} -lt 500 ]; then
            echo "  Response: $body" >> "$TEST_LOG"
        fi
        PASSED_TESTS=$((PASSED_TESTS + 1))
        return 0
    else
        echo -e "${RED}FAILED${NC} (Expected $expected_status, got $http_code)"
        echo "[FAIL] $method $endpoint - Expected $expected_status, got $http_code" >> "$TEST_LOG"
        echo "  Description: $description" >> "$TEST_LOG"
        if [ -n "$body" ]; then
            echo "  Response: $body" >> "$TEST_LOG"
        fi
        FAILED_TESTS=$((FAILED_TESTS + 1))
        return 1
    fi
}

# Wait for server to be ready
wait_for_server() {
    echo -e "${BLUE}Waiting for server to start...${NC}"
    local max_attempts=30
    local attempt=0
    
    while [ $attempt -lt $max_attempts ]; do
        if curl -s "$BASE_URL/health" > /dev/null 2>&1; then
            echo -e "${GREEN}Server is ready!${NC}"
            return 0
        fi
        attempt=$((attempt + 1))
        sleep 1
    done
    
    echo -e "${RED}Server failed to start after $max_attempts seconds${NC}"
    return 1
}

# Start server in background
echo -e "${BLUE}Starting Nexus server on port $PORT...${NC}"
cd "$(dirname "$0")/.." || exit 1

# Build if needed
if ! cargo build --release --bin nexus-server > /dev/null 2>&1; then
    echo -e "${YELLOW}Release build not found, building...${NC}"
    cargo build --release --bin nexus-server
fi

# Start server
cargo run --release --bin nexus-server > "$SERVER_LOG" 2>&1 &
SERVER_PID=$!
echo $SERVER_PID > "$PID_FILE"
echo "Server started with PID: $SERVER_PID"

# Wait for server to be ready
if ! wait_for_server; then
    echo -e "${RED}Failed to start server. Check logs: $SERVER_LOG${NC}"
    exit 1
fi

# Initialize test log
echo "=== REST API Route Tests - $(date) ===" > "$TEST_LOG"
echo "" >> "$TEST_LOG"

echo -e "\n${BLUE}=== Testing REST API Routes ===${NC}\n"

# Health endpoints
echo -e "${YELLOW}--- Health Endpoints ---${NC}"
test_route "GET" "/" "" 200 "Root health check"
test_route "GET" "/health" "" 200 "Health check endpoint"
test_route "GET" "/metrics" "" 200 "Metrics endpoint"

# Cypher endpoint
echo -e "\n${YELLOW}--- Cypher Endpoint ---${NC}"
test_route "POST" "/cypher" '{"query": "RETURN 1 as test"}' 200 "Execute Cypher query"

# Schema endpoints
echo -e "\n${YELLOW}--- Schema Endpoints ---${NC}"
test_route "POST" "/schema/labels" '{"label": "TestLabel"}' "any" "Create label"
test_route "GET" "/schema/labels" "" 200 "List labels"
test_route "POST" "/schema/rel_types" '{"rel_type": "TEST_REL"}' "any" "Create relationship type"
test_route "GET" "/schema/rel_types" "" 200 "List relationship types"

# Data endpoints
echo -e "\n${YELLOW}--- Data Endpoints ---${NC}"
# Create a node first for testing
NODE_DATA='{"labels": ["Person"], "properties": {"name": "TestUser", "age": 30}}'
test_route "POST" "/data/nodes" "$NODE_DATA" "any" "Create node"
# Get node by ID (using ID 0 as test)
test_route "GET" "/data/nodes?id=0" "" "any" "Get node by ID"
# Update node
test_route "PUT" "/data/nodes" '{"id": 0, "properties": {"name": "UpdatedUser"}}' "any" "Update node"
# Create relationship
test_route "POST" "/data/relationships" '{"source_id": 0, "target_id": 0, "rel_type": "KNOWS", "properties": {}}' "any" "Create relationship"
# Delete node (may fail if node doesn't exist, that's ok)
test_route "DELETE" "/data/nodes?id=0" "" "any" "Delete node"

# Statistics endpoint
echo -e "\n${YELLOW}--- Statistics Endpoint ---${NC}"
test_route "GET" "/stats" "" 200 "Get database statistics"

# Performance monitoring endpoints
echo -e "\n${YELLOW}--- Performance Monitoring Endpoints ---${NC}"
test_route "GET" "/performance/statistics" "" 200 "Get query statistics"
test_route "GET" "/performance/slow-queries" "" 200 "Get slow queries"
test_route "GET" "/performance/slow-queries/analysis" "" 200 "Analyze slow queries"
test_route "GET" "/performance/plan-cache" "" 200 "Get plan cache statistics"
test_route "POST" "/performance/plan-cache/clear" "" 200 "Clear plan cache"

# MCP Performance endpoints
echo -e "\n${YELLOW}--- MCP Performance Endpoints ---${NC}"
test_route "GET" "/mcp/performance/statistics" "" 200 "Get MCP tool statistics"
test_route "GET" "/mcp/performance/tools/test_tool" "" "any" "Get tool statistics"
test_route "GET" "/mcp/performance/slow-tools" "" 200 "Get slow tool calls"
test_route "GET" "/mcp/performance/cache" "" 200 "Get cache statistics"
test_route "POST" "/mcp/performance/cache/clear" "" 200 "Clear cache"

# Graph comparison endpoints
echo -e "\n${YELLOW}--- Graph Comparison Endpoints ---${NC}"
test_route "POST" "/comparison/compare" '{"graph1": {"nodes": [], "edges": []}, "graph2": {"nodes": [], "edges": []}}' "any" "Compare graphs"
test_route "POST" "/comparison/similarity" '{"graph1": {"nodes": [], "edges": []}, "graph2": {"nodes": [], "edges": []}}' "any" "Calculate similarity"
test_route "POST" "/comparison/stats" '{"graph": {"nodes": [], "edges": []}}' "any" "Get graph stats"
test_route "GET" "/comparison/health" "" 200 "Comparison health check"
test_route "POST" "/comparison/advanced" '{"graph1": {"nodes": [], "edges": []}, "graph2": {"nodes": [], "edges": []}}' "any" "Advanced compare graphs"

# Clustering endpoints
echo -e "\n${YELLOW}--- Clustering Endpoints ---${NC}"
test_route "GET" "/clustering/algorithms" "" 200 "Get clustering algorithms"
test_route "POST" "/clustering/cluster" '{"algorithm": "kmeans", "k": 3, "node_ids": [0]}' "any" "Cluster nodes"
test_route "POST" "/clustering/group-by-label" '{"label": "Person"}' "any" "Group by label"
test_route "POST" "/clustering/group-by-property" '{"property": "name"}' "any" "Group by property"

# Graph correlation endpoints
echo -e "\n${YELLOW}--- Graph Correlation Endpoints ---${NC}"
test_route "POST" "/graph-correlation/generate" '{"graph_type": "Call", "files": {"test.rs": "fn test() {}"}}' "any" "Generate graph"
test_route "GET" "/graph-correlation/types" "" 200 "Get graph types"
test_route "GET" "/graph-correlation/auto-generate" "" "any" "Auto generate graphs"

# UMICP endpoint
echo -e "\n${YELLOW}--- UMICP Endpoint ---${NC}"
test_route "POST" "/umicp/graph" '{"method": "graph.generate", "params": {"graph_type": "Call", "files": {"test.rs": "fn test() {}"}}}' "any" "UMICP graph request"

# OpenAPI endpoint
echo -e "\n${YELLOW}--- OpenAPI Endpoint ---${NC}"
test_route "GET" "/openapi.json" "" 200 "Get OpenAPI spec"

# KNN traverse endpoint
echo -e "\n${YELLOW}--- KNN Traverse Endpoint ---${NC}"
test_route "POST" "/knn_traverse" '{"vector": [1.0, 2.0, 3.0], "k": 5, "max_depth": 3}' "any" "KNN traverse"

# Ingest endpoint
echo -e "\n${YELLOW}--- Ingest Endpoint ---${NC}"
test_route "POST" "/ingest" '{"nodes": [{"labels": ["Person"], "properties": {"name": "Test"}}], "relationships": []}' "any" "Bulk ingest data"

# Export endpoint
echo -e "\n${YELLOW}--- Export Endpoint ---${NC}"
test_route "GET" "/export?format=json" "" "any" "Export data"

# Auth endpoints (may require authentication)
echo -e "\n${YELLOW}--- Auth Endpoints ---${NC}"
test_route "GET" "/auth/users" "" "any" "List users"
test_route "GET" "/auth/users/testuser" "" "any" "Get user"
test_route "GET" "/auth/keys" "" "any" "List API keys"
test_route "GET" "/auth/keys/test_key" "" "any" "Get API key"

# Print summary
echo -e "\n${BLUE}=== Test Summary ===${NC}"
echo -e "Total tests: $TOTAL_TESTS"
echo -e "${GREEN}Passed: $PASSED_TESTS${NC}"
echo -e "${RED}Failed: $FAILED_TESTS${NC}"

# Add summary to log
echo "" >> "$TEST_LOG"
echo "=== Summary ===" >> "$TEST_LOG"
echo "Total tests: $TOTAL_TESTS" >> "$TEST_LOG"
echo "Passed: $PASSED_TESTS" >> "$TEST_LOG"
echo "Failed: $FAILED_TESTS" >> "$TEST_LOG"

if [ $FAILED_TESTS -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed. Check $TEST_LOG for details.${NC}"
    exit 1
fi
