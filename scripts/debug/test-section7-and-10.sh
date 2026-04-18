#!/bin/bash
# Test failing Section 7 and 10 tests

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

echo "DEBUG: Verifying nodes..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) RETURN n.name, labels(n)"}' $NEXUS_URI/cypher | jq '.rows'

# Create WORKS_AT relationships
echo "3. Creating WORKS_AT relationships..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (c1:Company {name: '\''Acme'\''}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)"}' $NEXUS_URI/cypher > /dev/null
sleep 0.5
echo "DEBUG: Verifying WORKS_AT 1..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r)"}' $NEXUS_URI/cypher | jq '.rows'

curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (c2:Company {name: '\''TechCorp'\''}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)"}' $NEXUS_URI/cypher > /dev/null
sleep 0.5
echo "DEBUG: Verifying WORKS_AT 2..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r)"}' $NEXUS_URI/cypher | jq '.rows'

curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p2:Person {name: '\''Bob'\''}), (c1:Company {name: '\''Acme'\''}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)"}' $NEXUS_URI/cypher > /dev/null
sleep 1
echo "DEBUG: Verifying WORKS_AT 3..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r)"}' $NEXUS_URI/cypher | jq '.rows'

# Create KNOWS relationship
echo "4. Creating KNOWS relationship..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}) CREATE (p1)-[:KNOWS]->(p2)"}' $NEXUS_URI/cypher
sleep 1
echo "DEBUG: Verifying KNOWS..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r:KNOWS]->() RETURN count(r)"}' $NEXUS_URI/cypher | jq '.rows'

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
echo "Row count: $row_count (expected 2)"

# Test 7.11
echo "=== Test 7.11: Return nodes from relationship pattern ==="
echo "Query: MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:KNOWS]->(b) RETURN a.name AS source"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected 1)"

# Test 7.12
echo "=== Test 7.12: Return properties from relationship pattern ==="
echo "Query: MATCH (a)-[r:WORKS_AT]->(b) RETURN a.name AS source, r.since AS since, b.name AS company ORDER BY since"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r:WORKS_AT]->(b) RETURN a.name AS source, r.since AS since, b.name AS company ORDER BY since"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected 3)"

# Test 7.13
echo "=== Test 7.13: Return relationship type ==="
echo "Query: MATCH (a)-[r]->(b) WHERE a.name = 'Alice' AND b.name = 'Bob' RETURN type(r) AS type"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (a)-[r]->(b) WHERE a.name = '\''Alice'\'' AND b.name = '\''Bob'\'' RETURN type(r) AS type"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
row_count=$(echo "$result" | jq '.rows | length')
echo "Row count: $row_count (expected 1)"

# Test 7.29
echo "=== Test 7.29: Count all relationships ==="
echo "Query: MATCH ()-[r]->() RETURN count(r) AS cnt"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH ()-[r]->() RETURN count(r) AS cnt"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
cnt=$(echo "$result" | jq -r '.rows[0][0]')
echo "Count: $cnt (expected 4)"

# Test 7.30
echo "=== Test 7.30: Multiple CREATE with relationships ==="
echo "Query: CREATE (a:Person {name: 'Test1'})-[r:TEST]->(b:Person {name: 'Test2'}) RETURN type(r)"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "CREATE (a:Person {name: '\''Test1'\''})-[r:TEST]->(b:Person {name: '\''Test2'\''}) RETURN type(r)"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'


echo ""
echo "=== Testing Section 10 Errors ==="
echo ""

# Clean database
echo "Cleaning database..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Create nodes
echo "Creating nodes for Section 10..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "CREATE (p1:Person {name: '\''Alice'\''}), (p2:Person {name: '\''Bob'\''}), (c1:Company {name: '\''Acme'\''})"}' $NEXUS_URI/cypher > /dev/null
sleep 1

# Test 10.01
echo "=== Test 10.01: UNION two queries ==="
echo "Query: MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
count=$(echo "$result" | jq '.rows | length')
echo "Count: $count (expected 3)"

# Test 10.02
echo "=== Test 10.02: UNION ALL ==="
echo "Query: MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
count=$(echo "$result" | jq '.rows | length')
echo "Count: $count (expected 3)"

# Test 10.05
echo "=== Test 10.05: UNION with WHERE ==="
# Need ages for this test
echo "Updating ages..."
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n) DETACH DELETE n"}' $NEXUS_URI/cypher > /dev/null
sleep 0.5
curl -s -X POST -H "Content-Type: application/json" -d '{"query": "CREATE (p1:Person {name: '\''Alice'\'', age: 30}), (p2:Person {name: '\''Bob'\'', age: 25}), (c1:Company {name: '\''Acme'\''})"}' $NEXUS_URI/cypher > /dev/null
sleep 1

echo "Query: MATCH (n:Person) WHERE n.age > 25 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n:Person) WHERE n.age > 25 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
count=$(echo "$result" | jq '.rows | length')
echo "Count: $count (expected 2: Alice, Acme)"

# Test 10.06
echo "=== Test 10.06: UNION with COUNT ==="
echo "Query: MATCH (n:Person) RETURN count(n) AS cnt UNION MATCH (n:Company) RETURN count(n) AS cnt"
result=$(curl -s -X POST -H "Content-Type: application/json" -d '{"query": "MATCH (n:Person) RETURN count(n) AS cnt UNION MATCH (n:Company) RETURN count(n) AS cnt"}' $NEXUS_URI/cypher)
echo "Result:"
echo "$result" | jq '.rows'
# Expected: [2], [1] (order implies rows, but union dedups rows? No, count is 2 and 1. 2 != 1. So 2 rows.)
count=$(echo "$result" | jq '.rows | length')
echo "Count: $count (expected 2)"

