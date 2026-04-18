#!/bin/bash

NEXUS_URL="http://localhost:15474/cypher"

echo "=== Testing 7.11-7.13: Return nodes from relationship pattern ==="

# Clean database
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URL > /dev/null

# Create test data
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "CREATE (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}), (c:Company {name: '\''Acme'\''})"}' $NEXUS_URL > /dev/null

# Create relationships
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}) CREATE (p1)-[:KNOWS]->(p2)"}' $NEXUS_URL > /dev/null

# Create another relationship (noise)
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (c:Company {name: '\''Acme'\''}) CREATE (p1)-[:WORKS_AT]->(c)"}' $NEXUS_URL > /dev/null

echo "1. Test 7.11: MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"}' $NEXUS_URL)
echo "Raw response: $RESULT"
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 1)"
echo "$RESULT" | jq '.rows'

echo ""
echo "2. Test with labels: MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name AS source"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person)-[r:KNOWS]->(b:Person) RETURN a.name AS source"}' $NEXUS_URL)
echo "Raw response: $RESULT"
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 1)"
echo "$RESULT" | jq '.rows'

