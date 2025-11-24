#!/bin/bash
# Test relationship duplication issue

NEXUS_URI="http://localhost:15474"

echo "=== Testing Relationship Duplication ==="
echo ""

# Clean and setup
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URI/cypher > /dev/null
sleep 1

curl -s -X POST -H "Content-Type: application/json" -d '{"query": "CREATE (a:Person {name: '\''Alice'\''}), (b:Person {name: '\''Bob'\''}) RETURN a.name, b.name"}' $NEXUS_URI/cypher > /dev/null
sleep 1

curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a:Person {name: '\''Alice'\''}), (b:Person {name: '\''Bob'\''}) CREATE (a)-[:KNOWS]->(b)"}' $NEXUS_URI/cypher > /dev/null
sleep 1

echo "1. Verify KNOWS relationship count:"
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:KNOWS]->() RETURN count(r) AS cnt"}' $NEXUS_URI/cypher | jq -r '.rows[0][0]'

echo "2. Query with relationship pattern (should return 1 row):"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source, b.name AS target"}' $NEXUS_URI/cypher)
echo "$result" | jq '.'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"

echo ""
echo "3. Query with specific labels (should return 1 row):"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name AS source, b.name AS target"}' $NEXUS_URI/cypher)
echo "$result" | jq '.'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"

echo ""
echo "4. Query returning only source (should return 1 row):"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"}' $NEXUS_URI/cypher)
echo "$result" | jq '.'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected: 1)"

