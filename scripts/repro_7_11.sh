#!/bin/bash

NEXUS_URL="http://localhost:15474/cypher"

echo "=== Reproduction: 7.11 Return source node ==="

# 1. Clean database
echo "1. Cleaning database..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URL > /dev/null

sleep 1

# 2. Create test data (Alice, Bob, Acme)
echo "2. Creating nodes..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "CREATE (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}), (c:Company {name: '\''Acme'\''})"}' $NEXUS_URL > /dev/null

# 3. Create relationship (Alice)-[:KNOWS]->(Bob)
echo "3. Creating relationship..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}) CREATE (p1)-[:KNOWS]->(p2)"}' $NEXUS_URL > /dev/null

# 4. Verify Nodes
echo "4. Verifying Nodes (expect 3)..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) RETURN n.name"}' $NEXUS_URL | jq '.rows'

# 5. Verify Relationships
echo "5. Verifying Relationships (expect 1 KNOWS)..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH ()-[r:KNOWS]->() RETURN type(r)"}' $NEXUS_URL | jq '.rows'

# 6. Run Failing Query
echo "6. Running Failing Query: MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"}' $NEXUS_URL)

echo "Result:"
echo "$RESULT" | jq .

COUNT=$(echo "$RESULT" | jq '.rows | length')
echo "Row Count: $COUNT (Expected: 1)"

