# Fix Data API Endpoints - Corm Change Proposal

**Change ID**: `fix-data-api-endpoints`  
**Priority**: 🔴 **CRITICAL**  
**Status**: 📋 **READY TO START**  
**Created**: 2025-10-29

## Summary

Fix critical bugs in REST API data endpoints that prevent basic node CRUD operations from working correctly. Currently, GET/PUT/DELETE `/data/nodes` endpoints either fail with Cypher parser errors or return "not implemented" messages.

orphans of this fix:
- Users can properly retrieve individual nodes via REST API
- Users can update and delete nodes directly via REST without Cypher
- Full CRUD operations are available through REST API
- API consistency with other working endpoints (POST /data/nodes)

## Problem Statement

During comprehensive REST API testing, three critical issues were identified:

1. **GET /data/nodes?id=X** fails with Cypher syntax error when trying to retrieve nodes
2. **PUT /data/nodes** returns "not implemented" message instead of updating nodes
3. **DELETE /data/nodes** returns "not implemented" message instead of deleting nodes

These issues block basic data management operations via REST API, forcing users to use Cypher queries for simple operations.

## Proposed Solution

Refactor all three endpoints to use `Engine` directly, following the same pattern as the working `POST /data/nodes` endpoint:

1. **GET /data/nodes**: Use `Engine.get_node(id)` directly instead of Cypher query
2. **PUT /data/nodes**: Implement using `Engine.update_node()` directly
3. **DELETE /data/nodes**: Implement using `Engine.delete_node()` directly

## Technical Details

### Current Implementation Issues

**GET /data/nodes** (`nexus-server/src/api/data.rs:611`):
- ❌ Tries to use Cypher: `MATCH (n) WHERE id(n) = {}`
- ❌ Cypher parser doesn't support `id(n)` function
- ❌ Returns syntax error

**PUT /data/nodes** (`nexus-server/src/api/data.rs:463`):
- ❌ Just returns error message
- ❌ Doesn't actually update nodeд

**DELETE /data/nodes** (`nexus-server/src/api/data.rs:528`):
- ❌ Just returns error message
- ❌ Doesn't actually delete node

### Proposed Implementation

All endpoints should follow the pattern used in `create_node()`:
- Get Engine instance via `ENGINE.get()`
- Acquire write lock: `engine.write().await`
- Call Engine method directly
- Return properly formatted JSON response

### Engine API Reference

From `nexus-core/src/lib.rs`:

```rust
// GET - Read node
pub fn get_node(&mut self, id: u64) -> Result<Option<storage::NodeRecord>>

// PUT - Update node  
pub fn update_node(
    &mut self,
    id: u64,
    labels: Vec<String>,
    properties: serde_json::Value,
) -> Result<()>

// DELETE - Delete node
pub fn delete_node(&mut self, id: u64) -> Result<()>
```

## Implementation Plan

See detailed tasks in `tasks.md`.

## Testing Strategy

1. Unit tests for each endpoint function
2. Integration tests via REST API
3. Re-run complete test suite (`test-all-routes.ps1`)
4. Verify all CRUD operations work end-to-end
5. Test error cases (invalid IDs, not found, etc.)

## Success Criteria

- ✅ GET /data/nodes?id=X returns node data correctly
- ✅ PUT /data/nodes updates node properties and labels
- ✅ DELETE /data/nodes removes node from storage
- ✅ All endpoints return proper JSON responses
- ✅ All REST tests pass (100% success rate)
- ✅ Error handling works for edge cases

## Dependencies

- ✅ Engine already initialized in server
- ✅ `create_node()` working as reference implementation
- ✅ Engine methods (`get_node`, `update_node`, `delete_node`) exist and working

## Risks

- **Low Risk**: Changes are straightforward refactoring following existing patterns
- Engine methods already exist and are tested
- No breaking changes to API contracts

## Timeline

**Estimated**: 3.5 - 4.5 hours
- Task 1 (GET): 1-2 hours
- Task 2 (PUT): 1-2 hours  
- Task 3 (DELETE): 1 hour
- Task 4 (Testing): 30 minutes

## Related Changes

- v0.9.1: Engine-Executor data synchronization fixes
- v0.9.0: CREATE persistence fixes
- This fixes the remaining CRUD operation issues

---

**Next Steps**: Start with Task 1 (GET endpoint) as it's the most critical blocker.

