#!/bin/bash

NEXUS_URL="http://localhost:15474/cypher"

echo "=== Testing UNION Queries ==="

# Clean database
echo "1. Cleaning database..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) DETACH DELETE n"}' $NEXUS_URL > /dev/null
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Company) DETACH DELETE n"}' $NEXUS_URL > /dev/null

sleep 1

# Create test data
echo "2. Creating test data..."
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "CREATE (n:Person {name: '\''Alice'\'', age: 30})"}' $NEXUS_URL > /dev/null
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "CREATE (n:Person {name: '\''Bob'\'', age: 25})"}' $NEXUS_URL > /dev/null
curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "CREATE (n:Company {name: '\''Acme'\''})"}' $NEXUS_URL > /dev/null

sleep 1

# Verify data
echo "3. Verifying data..."
PERSON_COUNT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) RETURN count(n) AS cnt"}' $NEXUS_URL | jq -r '.rows[0][0]')
COMPANY_COUNT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Company) RETURN count(n) AS cnt"}' $NEXUS_URL | jq -r '.rows[0][0]')

echo "   Persons: $PERSON_COUNT (expected: 2)"
echo "   Companies: $COMPANY_COUNT (expected: 1)"

# Test 10.01 - UNION two queries
echo ""
echo "4. Test 10.01: UNION two queries"
echo "   Query: MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URL)
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 3)"
echo "$RESULT" | jq '.rows'

# Test 10.02 - UNION ALL
echo ""
echo "5. Test 10.02: UNION ALL"
echo "   Query: MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) RETURN n.name AS name UNION ALL MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URL)
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 3)"
echo "$RESULT" | jq '.rows'

# Test 10.05 - UNION with WHERE
echo ""
echo "6. Test 10.05: UNION with WHERE"
echo "   Query: MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) WHERE n.age > 30 RETURN n.name AS name UNION MATCH (n:Company) RETURN n.name AS name"}' $NEXUS_URL)
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 1 - only Alice has age > 30, but UNION deduplicates)"
echo "   Note: Alice age=30, which is NOT > 30, so expected is 1 (only Acme)"
echo "$RESULT" | jq '.rows'

# Test 10.06 - UNION with COUNT
echo ""
echo "7. Test 10.06: UNION with COUNT"
echo "   Query: MATCH (n:Person) RETURN count(n) AS cnt UNION MATCH (n:Company) RETURN count(n) AS cnt"
RESULT=$(curl -s -X POST -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n:Person) RETURN count(n) AS cnt UNION MATCH (n:Company) RETURN count(n) AS cnt"}' $NEXUS_URL)
COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Result: $COUNT rows (expected: 2 - counts of 2 and 1)"
echo "$RESULT" | jq '.rows'

echo ""
echo "=== Test Complete ==="

