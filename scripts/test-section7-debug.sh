#!/bin/bash
# Debug script to investigate the 3 failing tests in Section 7

NEXUS_URI="${NEXUS_URI:-http://localhost:15474}"

echo "=== Section 7 Debug Test ==="
echo ""

# Helper function to get value from row array by column index
get_value() {
    local result="$1"
    local row_idx="$2"
    local col_idx="$3"
    echo "$result" | jq -r ".rows[$row_idx][$col_idx] // empty"
}

# Helper function to get column index by name
get_col_index() {
    local result="$1"
    local col_name="$2"
    echo "$result" | jq -r ".columns | index(\"$col_name\") // empty"
}

# Clear database
echo "1. Clearing database..."
curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (n) DETACH DELETE n"}' > /dev/null
echo "OK"
echo ""

# Create nodes
echo "2. Creating nodes..."
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "CREATE (p1:Person {name: \"Alice\"}), (p2:Person {name: \"Bob\"}), (c1:Company {name: \"Acme\"}), (c2:Company {name: \"TechCorp\"}) RETURN count(*) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // 0'
echo "OK - Nodes created"
echo ""

# Create relationships one by one
echo "3. Creating relationships..."
echo "  3.1 Alice -> Acme (WORKS_AT, since: 2020)"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p1:Person {name: \"Alice\"}), (c1:Company {name: \"Acme\"}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1) RETURN count(*) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  3.2 Verifying relationships after first CREATE..."
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  3.3 Alice -> TechCorp (WORKS_AT, since: 2021)"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p1:Person {name: \"Alice\"}), (c2:Company {name: \"TechCorp\"}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2) RETURN count(*) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  3.4 Verifying relationships after second CREATE..."
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  3.5 Bob -> Acme (WORKS_AT, since: 2019)"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (p2:Person {name: \"Bob\"}), (c1:Company {name: \"Acme\"}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1) RETURN count(*) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  3.6 Verifying relationships after third CREATE..."
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

# Test 7.19
echo "4. Test 7.19: Relationship with aggregation"
echo "   Query: MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, count(r) AS jobs ORDER BY person"}')
echo "   Result:"
PERSON_IDX=$(get_col_index "$RESULT" "person")
JOBS_IDX=$(get_col_index "$RESULT" "jobs")
if [ -n "$PERSON_IDX" ] && [ -n "$JOBS_IDX" ]; then
    ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
    for i in $(seq 0 $((ROW_COUNT - 1))); do
        PERSON=$(get_value "$RESULT" "$i" "$PERSON_IDX")
        JOBS=$(get_value "$RESULT" "$i" "$JOBS_IDX")
        echo "     $PERSON -> $JOBS jobs"
    done
else
    echo "$RESULT" | jq '.'
fi
ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Row count: $ROW_COUNT (expected: 2)"
echo ""

# Test 7.25
echo "5. Test 7.25: MATCH all connected nodes"
echo "   Query: MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person)-[r]-(b) RETURN DISTINCT a.name AS name ORDER BY name"}')
echo "   Result:"
NAME_IDX=$(get_col_index "$RESULT" "name")
if [ -n "$NAME_IDX" ]; then
    ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
    for i in $(seq 0 $((ROW_COUNT - 1))); do
        NAME=$(get_value "$RESULT" "$i" "$NAME_IDX")
        echo "     $NAME"
    done
else
    echo "$RESULT" | jq '.'
fi
ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Row count: $ROW_COUNT (expected: 2)"
echo ""

# Test 7.30
echo "6. Test 7.30: Complex relationship query"
echo "   Query: MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person)-[r:WORKS_AT]->(c:Company) RETURN a.name AS person, c.name AS company, r.since AS year ORDER BY year"}')
echo "   Result:"
PERSON_IDX=$(get_col_index "$RESULT" "person")
COMPANY_IDX=$(get_col_index "$RESULT" "company")
YEAR_IDX=$(get_col_index "$RESULT" "year")
if [ -n "$PERSON_IDX" ] && [ -n "$COMPANY_IDX" ] && [ -n "$YEAR_IDX" ]; then
    ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
    for i in $(seq 0 $((ROW_COUNT - 1))); do
        PERSON=$(get_value "$RESULT" "$i" "$PERSON_IDX")
        COMPANY=$(get_value "$RESULT" "$i" "$COMPANY_IDX")
        YEAR=$(get_value "$RESULT" "$i" "$YEAR_IDX")
        echo "     $PERSON -> $COMPANY (since: $YEAR)"
    done
else
    echo "$RESULT" | jq '.'
fi
ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
echo "   Row count: $ROW_COUNT (expected: 3)"
echo ""

# Additional verification
echo "7. Additional verifications:"
echo "  7.1 Count all WORKS_AT relationships:"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  7.2 List all WORKS_AT relationships:"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person)-[r:WORKS_AT]->(b:Company) RETURN a.name AS person, b.name AS company, r.since AS year"}')
PERSON_IDX=$(get_col_index "$RESULT" "person")
COMPANY_IDX=$(get_col_index "$RESULT" "company")
YEAR_IDX=$(get_col_index "$RESULT" "year")
if [ -n "$PERSON_IDX" ] && [ -n "$COMPANY_IDX" ] && [ -n "$YEAR_IDX" ]; then
    ROW_COUNT=$(echo "$RESULT" | jq -r '.rows | length')
    for i in $(seq 0 $((ROW_COUNT - 1))); do
        PERSON=$(get_value "$RESULT" "$i" "$PERSON_IDX")
        COMPANY=$(get_value "$RESULT" "$i" "$COMPANY_IDX")
        YEAR=$(get_value "$RESULT" "$i" "$YEAR_IDX")
        echo "     $PERSON -> $COMPANY (since: $YEAR)"
    done
else
    echo "$RESULT" | jq '.'
fi
echo ""

echo "  7.3 Verify Alice relationships:"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person {name: \"Alice\"})-[r:WORKS_AT]->(b) RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "  7.4 Verify Bob relationships:"
RESULT=$(curl -s -X POST "${NEXUS_URI}/cypher" \
  -H "Content-Type: application/json" \
  -d '{"query": "MATCH (a:Person {name: \"Bob\"})-[r:WORKS_AT]->(b) RETURN count(r) AS cnt"}')
echo "$RESULT" | jq -r '.rows[0][0] // "null"'
echo ""

echo "=== End of test ==="

