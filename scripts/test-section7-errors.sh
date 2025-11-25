#!/bin/bash
# Test only failing Section 7 tests

NEXUS_URI="http://localhost:15474"

echo "=== Testing Section 7 Errors ==="
echo ""

# Clean database
echo "1. Cleaning database..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Create nodes
echo "2. Creating nodes..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "CREATE (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}), (c1:Company {name: '\''Acme'\''}), (c2:Company {name: '\''TechCorp'\''})"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Create WORKS_AT relationships
echo "3. Creating WORKS_AT relationships..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (c1:Company {name: '\''Acme'\''}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)"}' $NEXUS_URI/cypher > /dev/null
sleep 0.5
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (c2:Company {name: '\''TechCorp'\''}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)"}' $NEXUS_URI/cypher > /dev/null
sleep 0.5
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p2:Person {name: '\''Bob'\''}), (c1:Company {name: '\''Acme'\''}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Create KNOWS relationship
echo "4. Creating KNOWS relationship..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}) CREATE (p1)-[:KNOWS]->(p2)"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Verify setup
echo ""
echo "=== Setup Verification ==="
echo -n "Nodes: "
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) RETURN count(n) AS cnt"}' $NEXUS_URI/cypher | jq -r '.rows[0][0]'
echo -n "WORKS_AT rels: "
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS cnt"}' $NEXUS_URI/cypher | jq -r '.rows[0][0]'
echo -n "KNOWS rels: "
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:KNOWS]->() RETURN count(r) AS cnt"}' $NEXUS_URI/cypher | jq -r '.rows[0][0]'
echo ""

# Test 7.07
echo "=== Test 7.07: Count relationships by type ==="
echo "Query: MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r]->() RETURN type(r) AS t, count(r) AS cnt ORDER BY t"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 2)"
echo ""

# Test 7.11
echo "=== Test 7.11: Return source node ==="
echo "Query: MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"
echo ""

# Test 7.12
echo "=== Test 7.12: Return target node ==="
echo "Query: MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN b.name AS target"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"
echo ""

# Test 7.13
echo "=== Test 7.13: Return both nodes ==="
echo "Query: MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS src, b.name AS dst"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"
echo ""

# Test 7.29
echo "=== Test 7.29: Return distinct rel types ==="
echo "Query: MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r]->() RETURN DISTINCT type(r) AS t ORDER BY t"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 2)"
echo ""

echo "=== Test Complete ==="

