#!/bin/bash
set -e

# Check for running servers
echo "Checking for running servers..."
ps aux | grep nexus-server | grep -v grep || true

# Clear DB
echo "Clearing DB..."
rm -rf nexus-db
if [ -d "nexus-db" ]; then
    echo "ERROR: nexus-db still exists!"
    ls -R nexus-db
    exit 1
fi
echo "DB cleared."

# Start server in background
./target/release/nexus-server > server.log 2>&1 &
SERVER_PID=$!
echo "Server started with PID $SERVER_PID"

# Wait for server
sleep 5

# Create nodes
echo "Creating nodes..."
curl -X POST http://localhost:15474/cypher -H "Content-Type: application/json" -d '{"query": "CREATE (p1:Person {name: \"Alice\"}), (p2:Person {name: \"Bob\"}), (c1:Company {name: \"Acme\"}), (c2:Company {name: \"TechCorp\"})"}'
echo ""

# Create relationship 1
echo "Creating relationship 1..."
curl -X POST http://localhost:15474/cypher -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: \"Alice\"}), (c1:Company {name: \"Acme\"}) CREATE (p1)-[:WORKS_AT {since: 2020}]->(c1) RETURN count(*) AS cnt"}'
echo ""

# Debug Cartesian Product
echo "Debug Cartesian Product..."
curl -X POST http://localhost:15474/cypher -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: \"Alice\"}), (c2:Company {name: \"TechCorp\"}) RETURN p1.name, c2.name"}'
echo ""

# Create relationship 2
echo "Creating relationship 2..."
curl -X POST http://localhost:15474/cypher -H "Content-Type: application/json" -d '{"query": "MATCH (p1:Person {name: \"Alice\"}), (c2:Company {name: \"TechCorp\"}) CREATE (p1)-[:WORKS_AT {since: 2021}]->(c2) RETURN count(*) AS cnt"}'
echo ""

# Kill server
kill $SERVER_PID
