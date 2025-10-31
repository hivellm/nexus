# Neo4j Cross-Compatibility Testing

This directory contains tools to test Nexus compatibility with Neo4j by executing identical queries on both databases and comparing results.

## Overview

The cross-compatibility test validates that Nexus produces the same results as Neo4j for supported Cypher queries. This ensures that applications can migrate from Neo4j to Nexus with confidence.

## Prerequisites

1. **Neo4j Instance** - Running on default port 7474 (HTTP) or 7687 (Bolt)
2. **Nexus Server** - Running on port 15474
3. **PowerShell** - For running the test script

## Quick Start

### 1. Start Neo4j (if not already running)

```bash
# Using Docker
docker run -d \
  --name neo4j-compat-test \
  -p 7474:7474 -p 7687:7687 \
  -e NEO4J_AUTH=neo4j/password \
  neo4j:latest
```

### 2. Start Nexus

```bash
cd nexus
cargo run --release
```

### 3. Run Compatibility Test

```powershell
cd tests/cross-compatibility
./test-compatibility.ps1
```

## Test Coverage

The compatibility test validates:

- ✅ `count(*)` and `count(variable)` queries
- ✅ Node queries with labels
- ✅ Relationship queries
- ✅ Property access
- ✅ `WHERE` clause filtering
- ✅ Aggregations (`avg`, `min`, `max`, `sum`)
- ✅ `ORDER BY` sorting
- ✅ `LIMIT` clause
- ✅ `UNION` queries
- ✅ Function calls: `labels()`, `keys()`, `id()`, `type()`
- ✅ Multiple label support
- ✅ Bidirectional relationships
- ✅ `DISTINCT` operations

## Custom Configuration

```powershell
# Custom Neo4j instance
./test-compatibility.ps1 `
  -Neo4jUri "http://localhost:7474" `
  -Neo4jUser "neo4j" `
  -Neo4jPassword "your_password" `
  -NexusUri "http://localhost:15474"
```

## Output

The script generates:
- Console output with pass/fail for each query
- `cross-compatibility-report.json` - Detailed test results
- Summary with pass rate percentage

## Example Output

```
🔍 Neo4j vs Nexus Compatibility Test
============================================================

🔧 Setup: Clearing databases...
📝 Creating test data in both databases...
✅ Test data created

🧪 Running Compatibility Tests...
============================================================

📊 Testing: Count all nodes
Query: MATCH (n) RETURN count(*) AS count
Neo4j rows: 1 | Nexus rows: 1
Neo4j count: 8 | Nexus count: 8
✅ PASS - Results match!

...

============================================================
📊 COMPATIBILITY TEST SUMMARY
============================================================
Total Tests: 17
✅ Passed: 15
❌ Failed: 1
⚠️  Skipped: 1

🎯 Pass Rate: 88.24%
```

## Interpreting Results

- **✅ PASS** - Query produces identical results in both databases
- **❌ FAIL** - Results differ (may indicate missing feature or bug)
- **⚠️ SKIPPED** - Query failed in Neo4j (connection issue or syntax)

## Known Limitations

Some Neo4j features are intentionally not supported in Nexus MVP:
- `UNWIND` clause (planned for future)
- `LIMIT` after `UNION` (planned for future)
- `ORDER BY` after `UNION` (planned for future)
- Complex pattern matching with multiple relationships

See `docs/neo4j-compatibility-report.md` for full compatibility matrix.

## Troubleshooting

### Neo4j Connection Failed
```powershell
# Check if Neo4j is running
curl http://localhost:7474

# Check credentials
# Default: neo4j/neo4j (first login requires password change)
```

### Nexus Connection Failed
```powershell
# Check if Nexus is running
curl http://localhost:15474/health

# Start Nexus
cargo run --release
```

### Authentication Errors
Make sure to update the password in the script if you changed Neo4j's default password.

## Contributing

When adding new Cypher features to Nexus:
1. Add corresponding test query to `test-compatibility.ps1`
2. Run the test to validate Neo4j compatibility
3. Update `docs/neo4j-compatibility-report.md` with results

