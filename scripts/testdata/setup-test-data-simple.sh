#!/bin/bash

# Simple script to setup test data for UNION/DISTINCT tests

NEXUS_URL="http://localhost:15474/api/v1/query"

echo "Setting up test data..."

# Clean first
curl -s -X POST "$NEXUS_URL" -H "Content-Type: application/json" -d '{"query":"MATCH (n) DETACH DELETE n"}' > /dev/null

# Create Person nodes
curl -s -X POST "$NEXUS_URL" -H "Content-Type: application/json" -d '{"query":"CREATE (a:Person {name: \"Alice\", age: 30, city: \"NYC\"}), (b:Person {name: \"Bob\", age: 25, city: \"LA\"}), (c:Person {name: \"Charlie\", age: 35, city: \"NYC\"}), (d:Person {name: \"David\", age: 28, city: \"LA\"})"}' > /dev/null

# Create Company nodes  
curl -s -X POST "$NEXUS_URL" -H "Content-Type: application/json" -d '{"query":"CREATE (c1:Company {name: \"Acme\"}), (c2:Company {name: \"TechCorp\"})"}' > /dev/null

echo "Test data created!"
echo "Verifying data..."
curl -s -X POST "$NEXUS_URL" -H "Content-Type: application/json" -d '{"query":"MATCH (n) RETURN count(n) AS cnt"}' | jq -r '.rows[0].cnt'

