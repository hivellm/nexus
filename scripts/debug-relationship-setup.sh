#!/bin/bash
# Debug script to investigate relationship setup issues

NEXUS_URI="http://localhost:15474"

echo "=== DEBUG: Investigating relationship setup ==="
echo ""

# Function to execute Nexus query and format output
query_nexus() {
    local query="$1"
    echo "Query: $query"
    local result=$(curl -s -X POST \
        -H "Content-Type: application/json" \
        -d "{\"query\": \"$query\"}" \
        "${NEXUS_URI}/cypher")
    echo "Result:"
    echo "$result" | jq '.'
    echo "---"
}

# Clear database
echo "1. Clearing database..."
query_nexus "MATCH (n) DETACH DELETE n"
echo ""

# Check database is empty
echo "2. Verifying database is empty..."
query_nexus "MATCH (n) RETURN count(n) AS total"
echo ""

# Create nodes
echo "3. Creating nodes..."
query_nexus "CREATE (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}), (c2:Company {name: 'TechCorp'})"
echo ""

# Count nodes
echo "4. Counting nodes after creation..."
query_nexus "MATCH (n:Person) RETURN count(n) AS person_count"
query_nexus "MATCH (n:Company) RETURN count(n) AS company_count"
echo ""

# List all nodes
echo "5. Listing all nodes..."
query_nexus "MATCH (n) RETURN labels(n) AS labels, n.name AS name"
echo ""

# Create first WORKS_AT relationship
echo "6. Creating Alice -> Acme (WORKS_AT 2020)..."
query_nexus "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) RETURN p1._nexus_id AS alice_id, c1._nexus_id AS acme_id"
query_nexus "MATCH (p1:Person {name: 'Alice'}), (c1:Company {name: 'Acme'}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1)"
echo ""

# Count relationships
echo "7. Counting relationships after first CREATE..."
query_nexus "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS works_at_count"
echo ""

# Create second WORKS_AT relationship
echo "8. Creating Alice -> TechCorp (WORKS_AT 2021)..."
query_nexus "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) RETURN p1._nexus_id AS alice_id, c2._nexus_id AS techcorp_id"
query_nexus "MATCH (p1:Person {name: 'Alice'}), (c2:Company {name: 'TechCorp'}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2)"
echo ""

# Count relationships
echo "9. Counting relationships after second CREATE..."
query_nexus "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS works_at_count"
echo ""

# Create third WORKS_AT relationship
echo "10. Creating Bob -> Acme (WORKS_AT 2019)..."
query_nexus "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) RETURN p2._nexus_id AS bob_id, c1._nexus_id AS acme_id"
query_nexus "MATCH (p2:Person {name: 'Bob'}), (c1:Company {name: 'Acme'}) CREATE (p2)-[:WORKS_AT {since: 2019}]->(c1)"
echo ""

# Count relationships
echo "11. Counting relationships after third CREATE..."
query_nexus "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS works_at_count"
echo ""

# Create KNOWS relationship
echo "12. Creating Alice -> Bob (KNOWS)..."
query_nexus "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1._nexus_id AS alice_id, p2._nexus_id AS bob_id"
query_nexus "MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) CREATE (p1)-[:KNOWS]->(p2)"
echo ""

# Final counts
echo "13. Final relationship counts..."
query_nexus "MATCH ()-[r:WORKS_AT]->() RETURN count(r) AS works_at_count"
query_nexus "MATCH ()-[r:KNOWS]->() RETURN count(r) AS knows_count"
query_nexus "MATCH ()-[r]->() RETURN type(r) AS type, count(r) AS count ORDER BY type"
echo ""

# List all relationships
echo "14. Listing all relationships..."
query_nexus "MATCH (a)-[r]->(b) RETURN labels(a) AS src_labels, a.name AS src, type(r) AS rel_type, labels(b) AS dst_labels, b.name AS dst, properties(r) AS rel_props"
echo ""

echo "=== DEBUG COMPLETE ==="

