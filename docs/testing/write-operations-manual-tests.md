# Nexus Write Operations - REST API Test Examples

## Prerequisites
Start the Nexus server:
```bash
cd f:\Node\hivellm\nexus
cargo run --bin nexus-server
```

Server should be running on: `http://localhost:15474`

---

## 1. MERGE Operations

### Test 1.1: Basic MERGE (Create if not exists)
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MERGE (n:Person {name: \"Alice\"}) RETURN n"
  }'
```

**Expected**: Should create node if doesn't exist, or return existing node

### Test 1.2: MERGE with ON CREATE
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MERGE (n:Person {name: \"Bob\"}) ON CREATE SET n.created = timestamp(), n.id = 1 RETURN n"
  }'
```

**Expected**: Create node and set created timestamp only on first creation

### Test 1.3: MERGE with ON MATCH
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MERGE (n:Person {name: \"Alice\"}) ON MATCH SET n.updated = timestamp(), n.count = n.count + 1 RETURN n"
  }'
```

**Expected**: Update node only if it already exists

### Test 1.4: MERGE with both ON CREATE and ON MATCH
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MERGE (n:Person {email: \"alice@example.com\"}) ON CREATE SET n.created = timestamp() ON MATCH SET n.updated = timestamp() RETURN n"
  }'
```

---

## 2. SET Operations

### Test 2.1: SET property
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n.age = 30 RETURN n"
  }'
```

**Expected**: Update age property of Alice

### Test 2.2: SET multiple properties
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n.age = 30, n.city = \"NYC\", n.active = true RETURN n"
  }'
```

**Expected**: Update multiple properties at once

### Test 2.3: SET label
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n:VIP RETURN n"
  }'
```

**Expected**: Add VIP label to Alice

### Test 2.4: SET with expression
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n.age = n.age + 1 RETURN n"
  }'
```

**Expected**: Increment age by 1

### Test 2.5: SET all properties (replacement)
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n = {name: \"Alice\", age: 25, city: \"LA\"} RETURN n"
  }'
```

**Expected**: Replace all properties with new map

### Test 2.6: SET properties addition
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) SET n += {email: \"alice@example.com\", phone: \"555-1234\"} RETURN n"
  }'
```

**Expected**: Add new properties without removing existing ones

---

## 3. DELETE Operations

### Test 3.1: DELETE node (will fail if has relationships)
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"TestUser\"}) DELETE n"
  }'
```

**Expected**: Delete node if it has no relationships

### Test 3.2: DETACH DELETE (delete node and all relationships)
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) DETACH DELETE n"
  }'
```

**Expected**: Delete node and all its relationships

### Test 3.3: DELETE multiple nodes
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:TempNode) DELETE n"
  }'
```

**Expected**: Delete all nodes with TempNode label

### Test 3.4: DELETE relationship
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (a:Person)-[r:KNOWS]->(b:Person) WHERE a.name = \"Alice\" AND b.name = \"Bob\" DELETE r"
  }'
```

**Expected**: Delete specific relationship between Alice and Bob

---

## 4. REMOVE Operations

### Test 4.1: REMOVE property
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) REMOVE n.age RETURN n"
  }'
```

**Expected**: Remove age property from Alice

### Test 4.2: REMOVE label
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person:VIP {name: \"Alice\"}) REMOVE n:VIP RETURN n"
  }'
```

**Expected**: Remove VIP label from Alice

### Test 4.3: REMOVE multiple items
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person {name: \"Alice\"}) REMOVE n.age, n:VIP, n.temp_field RETURN n"
  }'
```

**Expected**: Remove multiple properties and labels

---

## 5. Complex Scenarios

### Test 5.1: MERGE relationship
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (a:Person {name: \"Alice\"}), (b:Person {name: \"Bob\"}) MERGE (a)-[r:KNOWS]->(b) RETURN a, r, b"
  }'
```

**Expected**: Create KNOWS relationship if doesn't exist

### Test 5.2: Complex write query
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MERGE (n:Person {email: \"charlie@example.com\"}) ON CREATE SET n.name = \"Charlie\", n.created = timestamp() ON MATCH SET n.updated = timestamp() SET n.active = true RETURN n"
  }'
```

**Expected**: Complex MERGE with conditional SETs

### Test 5.3: Conditional update
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) WHERE n.age > 30 SET n:Senior REMOVE n:Junior RETURN count(n)"
  }'
```

**Expected**: Update labels based on condition

---

## 6. Error Cases (Should fail gracefully)

### Test 6.1: DELETE with relationships (should fail)
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) WHERE EXISTS((n)-[:KNOWS]->()) DELETE n"
  }'
```

**Expected**: Error - cannot delete node with relationships (use DETACH DELETE)

### Test 6.2: Invalid property expression
```bash
curl -X POST http://localhost:15474/cypher \
  -H "Content-Type: application/json" \
  -d '{
    "query": "MATCH (n:Person) SET n.invalid = nonexistent_var RETURN n"
  }'
```

**Expected**: Error - undefined variable

---

## Current Status

**Parser**: ✅ COMPLETE - All queries above parse correctly
**Executor**: ⏳ PENDING - Execution will return "not implemented" errors

To implement executor:
1. Add execution logic for MERGE (match or create)
2. Add execution logic for SET (update properties/labels)
3. Add execution logic for DELETE (remove nodes/relationships)
4. Add execution logic for REMOVE (remove properties/labels)

---

## Quick Test Script

Save this as `test_write_ops.ps1`:

```powershell
$baseUrl = "http://localhost:15474"

Write-Host "Testing MERGE..." -ForegroundColor Yellow
curl -X POST "$baseUrl/cypher" -H "Content-Type: application/json" -d '{"query":"MERGE (n:Person {name: \"Alice\"}) RETURN n"}' | jq

Write-Host "`nTesting SET..." -ForegroundColor Yellow
curl -X POST "$baseUrl/cypher" -H "Content-Type: application/json" -d '{"query":"MATCH (n:Person) SET n.age = 30 RETURN n"}' | jq

Write-Host "`nTesting DELETE..." -ForegroundColor Yellow
curl -X POST "$baseUrl/cypher" -H "Content-Type: application/json" -d '{"query":"MATCH (n:TestNode) DELETE n"}' | jq

Write-Host "`nTesting REMOVE..." -ForegroundColor Yellow
curl -X POST "$baseUrl/cypher" -H "Content-Type: application/json" -d '{"query":"MATCH (n:Person) REMOVE n.temp RETURN n"}' | jq
```

Run with: `.\test_write_ops.ps1`
