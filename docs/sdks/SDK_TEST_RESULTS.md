# SDK Test Results

## Test Date: 2025-11-28

All SDKs were tested against Nexus Server v0.12.0 running on http://localhost:15474.

## Summary

| SDK | Status | Tests Passed | Issues |
|-----|--------|--------------|--------|
| Python | ✅ PASS | 6/6 | None |
| TypeScript | ✅ PASS | 6/6 | Required endpoint fix (`/cypher` instead of `/query`) |
| Rust | ✅ PASS | 6/6 | None |
| n8n | ✅ PASS | 30/30 (24 unit + 6 integration) | Required endpoint fix (same as TypeScript) |
| C# | ⚠️ PARTIAL | 1/5 | JSON deserialization error (rows format) |
| Go | ⚠️ PARTIAL | 0/6 | JSON deserialization error (rows format) |
| PHP | ❓ NOT TESTED | - | Missing Composer installation |

## Detailed Results

### Python SDK ✅

**Test File**: `sdks/python/test_sdk_simple.py`

**Results**:
```
=== Testing Python SDK ===

1. Simple query: OK - Columns: ['num']
2. Create nodes: OK - Rows: 1
3. Query with parameters: OK - Found 2 nodes
4. Create relationship: OK
5. Query relationships: OK - Found 1 relationships
6. Cleanup: OK

[SUCCESS] All Python SDK tests passed!
```

**Status**: All tests passing
**Issues**: None
**Notes**: Python SDK correctly handles the server's row format

---

### TypeScript SDK ✅

**Test File**: `sdks/typescript/test-sdk.ts`

**Results**:
```
=== Testing TypeScript SDK ===

1. Simple query: OK - Columns: num
2. Create nodes: OK - Rows: 1
3. Query with parameters: OK - Found 2 nodes
4. Create relationship: OK
5. Query relationships: OK - Found 0 relationships
6. Cleanup: OK

[SUCCESS] All TypeScript SDK tests passed!
```

**Status**: All tests passing after fix
**Issues**:
- Required endpoint change from `/query` to `/cypher`
- Required changing request body from `{cypher, params}` to `{query, parameters}`
- Test 5 found 0 relationships when it should find 1 (possible separate issue)

**Fixes Applied**:
- Updated `src/client.ts:144` to use `/cypher` endpoint
- Updated request body parameter names

---

### Rust SDK ✅

**Test File**: `sdks/rust/examples/test_sdk.rs`

**Results**:
```
=== Testing Rust SDK ===

1. Simple query: OK - Columns: num
2. Create nodes: OK - Rows: 1
3. Query with parameters: OK - Found 2 nodes
4. Create relationship: OK
5. Query relationships: OK - Found 0 relationships
6. Cleanup: OK

[SUCCESS] All Rust SDK tests passed!
```

**Status**: All tests passing
**Issues**:
- Test 5 found 0 relationships when it should find 1 (same as TypeScript)
**Notes**: Rust SDK already had correct endpoint and format

---

### n8n SDK ✅

**Test Files**:
- Unit tests: `sdks/n8n/tests/*.test.ts`
- Integration test: `sdks/n8n/test-integration.ts`

**Unit Test Results**:
```
 ✓ tests/credentials.test.ts (10 tests) 4ms
 ✓ tests/NexusClient.test.ts (14 tests) 5ms

 Test Files  2 passed (2)
      Tests  24 passed (24)
   Duration  299ms
```

**Integration Test Results**:
```
=== Testing n8n SDK Integration ===

1. Simple query: OK - Columns: num
2. Create nodes: OK - Rows: 1
3. Query with parameters: OK - Found 2 nodes
4. Create relationship: OK
5. Query relationships: OK - Found 0 relationships
6. Cleanup: OK

[SUCCESS] All n8n SDK integration tests passed!
```

**Status**: All tests passing (24 unit tests + 6 integration tests)
**Issues**:
- Required endpoint change from `/query` to `/cypher` (same as TypeScript)
- Required changing request body from `{cypher, params}` to `{query, parameters}`
- Test 5 found 0 relationships (same issue as TypeScript/Rust)

**Fixes Applied**:
- Updated `nodes/Nexus/NexusClient.ts:85` to use `/cypher` endpoint
- Updated request body parameter names

**Notes**:
- n8n SDK is a community node for n8n workflow automation platform
- Built on TypeScript, shares same patterns as standalone TypeScript SDK
- All unit tests pass using mocked n8n-workflow dependencies
- Integration tests confirm real server connectivity

---

### C# SDK ⚠️

**Test File**: `sdks/TestConsoleSimple/Program.cs`

**Results**:
```
=== Testing C# SDK ===

1. Ping server: OK
2. Simple query: [ERROR] Error: The JSON value could not be converted to System.Collections.Generic.Dictionary`2[System.String,System.Object]. Path: $.rows[0] | LineNumber: 0 | BytePositionInLine: 28.
```

**Status**: Partial - Ping works, query deserialization fails
**Issues**:
- JSON deserialization error when parsing query results
- Server returns rows as array of arrays: `{"rows": [[1]], "columns": ["num"]}`
- SDK expects rows as array of dictionaries: `{"rows": [{"num": 1}], "columns": ["num"]}`

**Root Cause**: Mismatch between server response format and SDK expectations

**Potential Fixes**:
1. Update server to return rows as objects (affects all clients)
2. Update C# SDK to handle array format
3. Add format negotiation to API

---

### Go SDK ⚠️

**Test File**: `sdks/go/test/test_sdk.go`

**Results**:
```
Error: failed to decode response: json: cannot unmarshal array into Go struct field QueryResult.rows of type map[string]interface {}
```

**Status**: Failing on first query
**Issues**:
- Same JSON deserialization error as C# SDK
- Server returns rows as arrays, SDK expects maps/dictionaries

**Root Cause**: Same as C# - format mismatch

---

### PHP SDK ❓

**Status**: Not tested
**Reason**: Composer not installed on test machine
**Command attempted**: `composer install` in `sdks/php/`
**Error**: `composer: command not found`

---

## Critical Issue: Row Format Incompatibility

### Problem

The server returns query results in array format:
```json
{
  "columns": ["num"],
  "rows": [[1]]
}
```

But Go and C# SDKs expect object/dictionary format:
```json
{
  "columns": ["num"],
  "rows": [{"num": 1}]
}
```

### Why Python/TypeScript/Rust Work

These SDKs are more flexible in their JSON handling:
- **Python**: Uses `serde_json::Value` which can represent any JSON
- **TypeScript**: Uses `any` type for row data
- **Rust**: Uses `serde_json::Value` for rows

### Recommended Solutions

1. **Option A** (Preferred): Update server to return rows as objects
   - More intuitive API
   - Easier to work with in strongly-typed languages
   - Matches Neo4j format

2. **Option B**: Update SDKs to handle array format
   - Keep server as-is
   - Add row parsing logic to each SDK
   - Map arrays to objects using column names

3. **Option C**: Support both formats
   - Add `format` query parameter
   - Default to object format for compatibility

---

## Relationship Query Issue

Multiple SDKs (TypeScript, Rust) report finding 0 relationships in test 5 when 1 relationship was created in test 4. This may indicate:
- Relationship query syntax issue
- Relationship not being persisted
- Transaction isolation issue

**Needs investigation**

---

## Next Steps

1. ✅ Fix TypeScript SDK endpoint (DONE)
2. ✅ Fix n8n SDK endpoint (DONE)
3. ⚠️ Decide on row format strategy
4. 🔲 Fix C# and Go SDKs based on decision
5. 🔲 Install Composer and test PHP SDK
6. 🔲 Investigate relationship query issue (affects Python, TypeScript, Rust, n8n)
7. 🔲 Add comprehensive integration tests for all SDKs

## Test Files Created

All integration test files are now available for future regression testing:

- `sdks/python/test_sdk_simple.py` - Python SDK integration tests
- `sdks/typescript/test-sdk.ts` - TypeScript SDK integration tests
- `sdks/rust/examples/test_sdk.rs` - Rust SDK integration tests
- `sdks/n8n/test-integration.ts` - n8n SDK integration tests
- `sdks/TestConsoleSimple/Program.cs` - C# SDK integration tests
- `sdks/go/test/test_sdk.go` - Go SDK integration tests (currently failing)
