# Tasks: Fix Critical DELETE, CREATE, and FILTER Bugs

**Status**: 🔴 Pending  
**Priority**: P0 - Blocking  
**Started**: 2025-10-31  
**Estimated**: 14 hours  

---

## Phase 1: Implement DELETE Operator (Priority 1)

### 1.1 Add DELETE Operator Infrastructure
- [ ] 1.1.1 Add `Operator::Delete { nodes: Vec<String> }` enum variant in `mod.rs`
- [ ] 1.1.2 Add `Operator::DetachDelete { nodes: Vec<String> }` enum variant
- [ ] 1.1.3 Update `plan_query` in planner to recognize Delete clauses
- [ ] 1.1.4 Implement `plan_delete` function in planner
- [ ] 1.1.5 Generate Delete operator from DeleteClause AST

### 1.2 Implement DELETE Execution
- [ ] 1.2.1 Add `execute_delete` function in executor
- [ ] 1.2.2 Extract node IDs from context variables
- [ ] 1.2.3 Call `RecordStore::delete_node` for each node ID
- [ ] 1.2.4 Ensure deleted nodes are marked (not physically removed)
- [ ] 1.2.5 Implement DETACH DELETE (delete relationships first)

### 1.3 Update Query Filtering
- [ ] 1.3.1 Modify `read_node` to skip deleted nodes
- [ ] 1.3.2 Modify `execute_node_by_label` to skip deleted nodes
- [ ] 1.3.3 Verify deleted nodes don't appear in MATCH results
- [ ] 1.3.4 Add `is_deleted` check in all node scan operations

### 1.4 Testing
- [ ] 1.4.1 Test: `MATCH (n) DELETE n` deletes all nodes
- [ ] 1.4.2 Test: `MATCH (n:Label) DELETE n` deletes labeled nodes
- [ ] 1.4.3 Test: `MATCH (n) DETACH DELETE n` deletes nodes + relationships
- [ ] 1.4.4 Test: Deleted nodes don't appear in subsequent queries
- [ ] 1.4.5 Run debug-match-create.ps1 to verify cleanup works

---

## Phase 2: Fix CREATE Duplication (Priority 2)

### 2.1 Investigation
- [ ] 2.1.1 Add debug counter to `create_node` function
- [ ] 2.1.2 Add logging to `execute_create_query` entry/exit
- [ ] 2.1.3 Add logging to track transaction lifecycle
- [ ] 2.1.4 Add logging to `refresh_executor` calls
- [ ] 2.1.5 Run tests to identify duplication source

### 2.2 Root Cause Fix
- [ ] 2.2.1 Verify `execute_create_query` called only once per CREATE
- [ ] 2.2.2 Verify `create_node` called only once per node
- [ ] 2.2.3 Check if `refresh_executor` triggers duplicates
- [ ] 2.2.4 Fix identified root cause
- [ ] 2.2.5 Remove debug logging after fix confirmed

### 2.3 Transaction Handling
- [ ] 2.3.1 Verify transaction created only once
- [ ] 2.3.2 Verify transaction committed only once
- [ ] 2.3.3 Ensure no transaction rollback/retry creating duplicates
- [ ] 2.3.4 Add transaction isolation if needed

### 2.4 Testing
- [ ] 2.4.1 Test: `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- [ ] 2.4.2 Test: Multiple CREATE statements create exact count
- [ ] 2.4.3 Test: MATCH returns correct count after CREATE
- [ ] 2.4.4 Test: No garbage nodes created
- [ ] 2.4.5 Run debug-match-create.ps1 to verify node count = 2

---

## Phase 3: Fix Inline Property Filters (Priority 3)

### 3.1 Investigation
- [ ] 3.1.1 Add logging to `execute_filter` showing input/output row counts
- [ ] 3.1.2 Add logging to `evaluate_predicate_on_row` showing results
- [ ] 3.1.3 Add logging to show property values being compared
- [ ] 3.1.4 Test with single simple query: `MATCH (n:Person {name: 'Alice'})`
- [ ] 3.1.5 Identify which hypothesis is correct

### 3.2 Fix Row Materialization
- [ ] 3.2.1 Verify `materialize_rows_from_variables` returns correct data
- [ ] 3.2.2 Verify context variables contain all scanned nodes
- [ ] 3.2.3 Ensure filtered rows are properly materialized
- [ ] 3.2.4 Fix any issues in row materialization

### 3.3 Fix Predicate Evaluation
- [ ] 3.3.1 Verify `evaluate_projection_expression` extracts properties correctly
- [ ] 3.3.2 Verify property access returns correct values (not null)
- [ ] 3.3.3 Verify value comparison works for strings
- [ ] 3.3.4 Verify boolean result is correct (true/false)
- [ ] 3.3.5 Fix any issues in predicate evaluation

### 3.4 Fix Result Set Update
- [ ] 3.4.1 Verify `update_result_set_from_rows` applies filtered rows
- [ ] 3.4.2 Verify result set row count matches filtered count
- [ ] 3.4.3 Ensure filter reduces rows (not passes all through)
- [ ] 3.4.4 Fix any issues in result set update

### 3.5 Fix Filter Ordering
- [ ] 3.5.1 Verify filters apply AFTER NodeByLabel scan
- [ ] 3.5.2 Ensure filters apply BEFORE Cartesian product
- [ ] 3.5.3 Test with multiple patterns to verify correct order
- [ ] 3.5.4 Adjust operator ordering if needed

### 3.6 Testing
- [ ] 3.6.1 Test: `MATCH (n:Person {name: 'Alice'}) RETURN n` returns 1 row
- [ ] 3.6.2 Test: `MATCH (n {age: 30}) RETURN n` filters by property
- [ ] 3.6.3 Test: Multiple inline properties work together
- [ ] 3.6.4 Test: Cartesian product with filters: `MATCH (p1 {name: 'Alice'}), (p2 {name: 'Bob'})`
- [ ] 3.6.5 Run debug-filter.ps1 to verify all tests pass

---

## Phase 4: Integration Testing

### 4.1 Clean Database Testing
- [ ] 4.1.1 Clean database with DELETE
- [ ] 4.1.2 Create fresh test data
- [ ] 4.1.3 Verify exact node/relationship counts
- [ ] 4.1.4 Run all debug scripts successfully

### 4.2 Cross-Compatibility Testing
- [ ] 4.2.1 Run test-compatibility.ps1
- [ ] 4.2.2 Verify >80% compatibility (target: 90%)
- [ ] 4.2.3 Fix any remaining failures
- [ ] 4.2.4 Document any intentional differences

### 4.3 Regression Testing
- [ ] 4.3.1 Run cargo test --workspace
- [ ] 4.3.2 Verify all 1279 tests still pass
- [ ] 4.3.3 Fix any regressions introduced
- [ ] 4.3.4 Add new regression tests for these bugs

### 4.4 Performance Testing
- [ ] 4.4.1 Measure DELETE performance (1K, 10K, 100K nodes)
- [ ] 4.4.2 Measure CREATE performance (ensure no slowdown)
- [ ] 4.4.3 Measure FILTER performance (ensure efficient)
- [ ] 4.4.4 Document any performance impacts

---

## Phase 5: Documentation & Cleanup

### 5.1 Code Documentation
- [ ] 5.1.1 Document DELETE operator in executor
- [ ] 5.1.2 Add examples for DELETE in Cypher reference
- [ ] 5.1.3 Document inline filter behavior
- [ ] 5.1.4 Update architecture diagrams if needed

### 5.2 Update OpenSpec
- [ ] 5.2.1 Mark all tasks as completed
- [ ] 5.2.2 Update compatibility percentage
- [ ] 5.2.3 Update neo4j-cross-compatibility-fixes tasks
- [ ] 5.2.4 Close this OpenSpec change

### 5.3 Update Project Documentation
- [ ] 5.3.1 Update CHANGELOG.md with v0.9.9 or v0.10.0
- [ ] 5.3.2 Update README.md compatibility percentage
- [ ] 5.3.3 Update neo4j-compatibility-report.md
- [ ] 5.3.4 Add examples for DELETE usage

### 5.4 Cleanup
- [ ] 5.4.1 Remove debug logging added during investigation
- [ ] 5.4.2 Remove temporary debug scripts if not needed
- [ ] 5.4.3 Remove commented-out code
- [ ] 5.4.4 Run cargo fmt and cargo clippy

---

## Success Criteria

### Functional Requirements ✅
- [ ] `CREATE (p:Person {name: 'Alice'})` creates exactly 1 node
- [ ] `MATCH (n:Person {name: 'Alice'}) RETURN n` returns exactly 1 row
- [ ] `MATCH (n) DETACH DELETE n` removes all nodes (count = 0)
- [ ] `MATCH (p1:Person), (p2:Person) RETURN p1, p2` returns 4 rows (2×2)
- [ ] No duplicate nodes created
- [ ] No garbage data in database

### Compatibility Requirements ✅
- [ ] Neo4j compatibility >80% (17 tests)
- [ ] All cross-compatibility tests pass
- [ ] No regression in existing tests (1279 tests)

### Performance Requirements ✅
- [ ] DELETE completes in <100ms for 1K nodes
- [ ] CREATE performance unchanged (<10% slowdown)
- [ ] FILTER performance acceptable (<50ms for 1K nodes)

---

## Priority & Dependencies

### Must Complete (Blocking)
1. Phase 1: DELETE (enables testing)
2. Phase 2: CREATE (stops corruption)
3. Phase 3: FILTER (enables queries)

### Can Complete After
4. Phase 4: Integration Testing (after fixes)
5. Phase 5: Documentation (after all fixes confirmed)

### Blocks
- All Neo4j compatibility work
- MATCH ... CREATE testing
- Production deployment
- v1.0 release

---

## Notes

- **Run tests after EACH phase** to verify fixes incrementally
- **Don't proceed to next phase** if current phase fails
- **Keep debug scripts** - they're valuable for future testing
- **Update this document** as new insights discovered

