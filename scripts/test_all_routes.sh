#!/bin/bash
# Script para testar todas as rotas do Nexus Server

BASE_URL="${NEXUS_URL:-http://localhost:8080}"
echo "Testing Nexus Server at: $BASE_URL"
echo "=========================================="

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Counter
PASSED=0
FAILED=0

# Test function
test_route() {
    local method=$1
    local endpoint=$2
    local data=$3
    local description=$4
    
    echo -n "Testing $method $endpoint ... "
    
    if [ "$method" = "GET" ]; then
        response=$(curl -s -w "\n%{http_code}" "$BASE_URL$endpoint")
    elif [ "$method" = "POST" ] || [ "$method" = "PUT" ]; then
        if [ -n "$data" ]; then
            response=$(curl -s -w "\n%{http_code}" -X "$method" -H "Content-Type: application/json" -d "$data" "$BASE_URL$endpoint")
        else
            response=$(curl -s -w "\n%{http_code}" -X "$method" "$BASE_URL$endpoint")
        fi
    elif [ "$method" = "DELETE" ]; then
        response=$(curl -s -w "\n%{http_code}" -X "$method" "$BASE_URL$endpoint")
    fi
    
    http_code=$(echo "$response" | tail -n1)
    body=$(echo "$response" | sed '$d')
    
    if [ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]; then
        echo -e "${GREEN}✓ PASS${NC} (HTTP $http_code)"
        ((PASSED++))
        return 0
    elif [ "$http_code" -ge 400 ] && [ "$http_code" -lt 500 ]; then
        echo -e "${YELLOW}⚠ CLIENT ERROR${NC} (HTTP $http_code) - $description"
        ((FAILED++))
        return 1
    else
        echo -e "${RED}✗ FAIL${NC} (HTTP $http_code)"
        echo "  Response: $body"
        ((FAILED++))
        return 1
    fi
}

echo ""
echo "=== Health & Status ==="
test_route "GET" "/health" "" "Health check"
test_route "GET" "/" "" "Root endpoint"
test_route "GET" "/metrics" "" "Metrics endpoint"

echo ""
echo "=== Database Statistics ==="
test_route "GET" "/stats" "" "Database statistics"

echo ""
echo "=== Schema Management ==="
test_route "GET" "/schema/labels" "" "List labels"
test_route "POST" "/schema/labels" '{"label": "TestLabel"}' "Create label"
test_route "GET" "/schema/rel_types" "" "List relationship types"
test_route "POST" "/schema/rel_types" '{"rel_type": "TEST_REL"}' "Create relationship type"

echo ""
echo "=== Cypher Queries ==="
test_route "POST" "/cypher" '{"query": "RETURN 1 as test"}' "Simple Cypher query"
test_route "POST" "/cypher" '{"query": "CREATE (n:Test {name: \"test\"}) RETURN n"}' "CREATE node query"

echo ""
echo "=== Data Management ==="
test_route "POST" "/data/nodes" '{"labels": ["Test"], "properties": {"name": "test"}}' "Create node"
test_route "GET" "/data/nodes?id=1" "" "Get node by ID"
test_route "PUT" "/data/nodes" '{"id": 1, "properties": {"name": "updated"}}' "Update node"
test_route "POST" "/data/relationships" '{"source_id": 1, "target_id": 2, "rel_type": "TEST_REL", "properties": {}}' "Create relationship"

echo ""
echo "=== Bulk Operations ==="
test_route "POST" "/ingest" '{"nodes": [{"labels": ["Test"], "properties": {"name": "bulk1"}}], "relationships": []}' "Bulk ingest"
test_route "GET" "/export?format=json" "" "Export data"

echo ""
echo "=== KNN & Vector Search ==="
test_route "POST" "/knn_traverse" '{"label": "Test", "vector": [0.1, 0.2, 0.3], "k": 5}' "KNN traverse"

echo ""
echo "=== Performance Monitoring ==="
test_route "GET" "/performance/statistics" "" "Query statistics"
test_route "GET" "/performance/slow-queries" "" "Slow queries"
test_route "GET" "/performance/plan-cache" "" "Plan cache"
test_route "GET" "/mcp/performance/statistics" "" "MCP tool statistics"
test_route "GET" "/mcp/performance/cache" "" "MCP cache statistics"

echo ""
echo "=== Graph Correlation ==="
test_route "GET" "/graph-correlation/types" "" "Graph correlation types"
test_route "POST" "/graph-correlation/generate" '{"graph_type": "Call", "files": {"test.rs": "fn main() {}"}, "name": "Test Graph"}' "Generate correlation graph"
test_route "GET" "/graph-correlation/auto-generate" "" "Auto-generate graphs"

echo ""
echo "=== UMICP Protocol ==="
test_route "POST" "/umicp/graph" '{"method": "graph.generate", "params": {"graph_type": "Call", "files": {"test.rs": "fn main() {}"}}}' "UMICP graph.generate"

echo ""
echo "=== Clustering ==="
test_route "GET" "/clustering/algorithms" "" "List clustering algorithms"
test_route "POST" "/clustering/cluster" '{"algorithm": "kmeans", "k": 3}' "Cluster nodes"
test_route "POST" "/clustering/group-by-label" '{"label": "Test"}' "Group by label"
test_route "POST" "/clustering/group-by-property" '{"property": "name"}' "Group by property"

echo ""
echo "=== Comparison ==="
test_route "GET" "/comparison/health" "" "Comparison health check"
test_route "POST" "/comparison/compare" '{"graph1": {"nodes": [], "edges": []}, "graph2": {"nodes": [], "edges": []}}' "Compare graphs"
test_route "POST" "/comparison/stats" '{"graph": {"nodes": [], "edges": []}}' "Graph statistics"

echo ""
echo "=== OpenAPI ==="
test_route "GET" "/openapi.json" "" "OpenAPI specification"

echo ""
echo "=========================================="
echo "Results: ${GREEN}$PASSED passed${NC}, ${RED}$FAILED failed${NC}"
echo ""

if [ $FAILED -eq 0 ]; then
    exit 0
else
    exit 1
fi

