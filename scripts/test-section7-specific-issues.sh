#!/bin/bash
# Test script for specific Section 7 relationship issues
# Tests the failing queries individually to isolate the problem

NEO4J_URI="http://localhost:7474"
NEO4J_USER="neo4j"
NEO4J_PASSWORD="password"
NEXUS_URI="http://localhost:15474"

echo "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = ╗"
echo "|  Section 7 Specific Relationship Issues Test                    |"
echo "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = ╝"
echo ""

# Function to execute query on Neo4j
neo4j_query() {
    local cypher="$1"
    local auth=$(echo -n "${NEO4J_USER}:${NEO4J_PASSWORD}" | base64)
    
    curl -s -X POST "${NEO4J_URI}/db/neo4j/tx/commit" \
        -H "Authorization: Basic ${auth}" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json" \
        -d "{\"statements\":[{\"statement\":\"${cypher}\",\"parameters\":{}}]}"
}

# Function to execute query on Nexus
nexus_query() {
    local cypher="$1"
    
    curl -s -X POST "${NEXUS_URI}/cypher" \
        -H "Content-Type: application/json" \
        -H "Accept: application/json" \
        -d "{\"query\":\"${cypher}\",\"parameters\":{}}"
}

# Cleanup function
clean_databases() {
    echo "Cleaning Neo4j database..."
    neo4j_query "MATCH (n) DETACH DELETE n" > /dev/null 2>&1
    echo "  Neo4j cleaned"
    
    echo "Cleaning Nexus database..."
    nexus_query "MATCH (n) DETACH DELETE n" > /dev/null 2>&1
    echo "  Nexus cleaned"
    
    sleep 0.5
}

# Setup function to create test data
setup_test_data() {
    echo "Creating test data..."
    
    # Delete existing test nodes
    neo4j_query "MATCH (n) WHERE n.name IN ['Alice', 'Bob', 'Acme', 'TechCorp'] DETACH DELETE n" > /dev/null 2>&1
    nexus_query "MATCH (n) WHERE n.name IN ['Alice', 'Bob', 'Acme', 'TechCorp'] DETACH DELETE n" > /dev/null 2>&1
    
    # Create Person and Company nodes with relationships
    neo4j_query "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})" > /dev/null 2>&1
    neo4j_query "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)" > /dev/null 2>&1
    neo4j_query "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)" > /dev/null 2>&1
    neo4j_query "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)" > /dev/null 2>&1
    neo4j_query "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)" > /dev/null 2>&1
    
    nexus_query "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})" > /dev/null 2>&1
    nexus_query "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)" > /dev/null 2>&1
    nexus_query "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)" > /dev/null 2>&1
    nexus_query "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)" > /dev/null 2>&1
    nexus_query "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)" > /dev/null 2>&1
    
    sleep 0.5
    echo "OK Test data created"
}

# Function to compare results
compare_results() {
    local test_name="$1"
    local query="$2"
    
    echo ""
    echo "--- $test_name ---"
    echo "Query: $query"
    echo ""
    
    local neo4j_result=$(neo4j_query "$query")
    local nexus_result=$(nexus_query "$query")
    
    # Check for errors
    if echo "$neo4j_result" | grep -q '"errors"'; then
        local error=$(echo "$neo4j_result" | python3 -c "import sys, json; data=json.load(sys.stdin); print(data.get('errors',[{}])[0].get('message','Unknown error'))" 2>/dev/null)
        echo "Neo4j ERROR: $error"
        return
    fi
    
    if echo "$nexus_result" | grep -q '"error"'; then
        local error=$(echo "$nexus_result" | python3 -c "import sys, json; data=json.load(sys.stdin); print(data.get('error','Unknown error'))" 2>/dev/null)
        echo "Nexus ERROR: $error"
        return
    fi
    
    # Extract row counts
    local neo4j_rows=$(echo "$neo4j_result" | python3 -c "import sys, json; data=json.load(sys.stdin); print(len(data.get('results',[{}])[0].get('data',[])))" 2>/dev/null)
    local nexus_rows=$(echo "$nexus_result" | python3 -c "import sys, json; data=json.load(sys.stdin); print(len(data.get('rows',[])))" 2>/dev/null)
    
    echo "Neo4j rows: $neo4j_rows"
    echo "Nexus rows: $nexus_rows"
    echo ""
    
    if [ "$neo4j_rows" = "$nexus_rows" ]; then
        echo "✅ PASSED: Row count matches"
    else
        echo "❌ FAILED: Row count mismatch"
    fi
    
    echo ""
    echo "Neo4j Results:"
    echo "$neo4j_result" | python3 -m json.tool 2>/dev/null | head -20
    
    echo ""
    echo "Nexus Results:"
    echo "$nexus_result" | python3 -m json.tool 2>/dev/null | head -20
    echo ""
}

# Setup: Clean databases completely
echo "🔧 Setting up test environment..."
clean_databases
setup_test_data
echo ""

# Test the specific failing queries
echo "+-----------------------------------------------------+"
echo "| Testing Specific Failing Queries                    |"
echo "+-----------------------------------------------------+"

compare_results "7.07 Count relationships by type" "MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t"
compare_results "7.11 Return source node" "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
compare_results "7.12 Return target node" "MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target"
compare_results "7.13 Return both nodes" "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst"
compare_results "7.29 Return distinct rel types" "MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t"

echo "+= = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = = +"
echo "Test completed!"

