# Analysis: Critical DELETE, CREATE, and FILTER Bugs

**Status**: 🔴 Critical  
**Created**: 2025-10-31  

---

## Executive Summary

Three critical bugs were discovered during Neo4j cross-compatibility testing that prevent basic Cypher operations from working. These bugs caused a **23.53% regression** in compatibility (70.59% → 47.06%) and make the system **not production ready**.

---

## Bug #1: DELETE Operations Not Working

### Symptoms
```cypher
-- Clean database
MATCH (n) DETACH DELETE n

-- Check if clean
MATCH (n) RETURN count(*) as count
-- Expected: 0
-- Actual: 31+ nodes remain
```

### Evidence
```powershell
# From debug tests
[CLEAN] Cleaning Nexus...
   [OK] Nexus cleaned

# But nodes persist
MATCH (n) RETURN count(*) AS count
   Rows: 1
   Count: 31  ❌ (should be 0)
```

### Root Cause

**DELETE operator is NOT IMPLEMENTED in the executor.**

**Evidence in code**:
```rust
// nexus-core/src/executor/mod.rs:273
pub fn execute(&mut self, query: &Query) -> Result<ResultSet> {
    for operator in operators {
        match operator {
            Operator::NodeByLabel { .. } => { /* implemented */ }
            Operator::Filter { .. } => { /* implemented */ }
            Operator::Project { .. } => { /* implemented */ }
            // Operator::Delete { .. } => MISSING! ❌
            _ => { /* other operators */ }
        }
    }
}
```

**Parser recognizes DELETE**:
```rust
// nexus-core/src/executor/parser.rs
pub enum Clause {
    Match(MatchClause),
    Create(CreateClause),
    Delete(DeleteClause),  // ✅ Parsed
    Merge(MergeClause),
    // ...
}
```

**But planner does NOT generate Delete operator**:
```rust
// nexus-core/src/executor/planner.rs
// No code to generate Operator::Delete
// No Operator::Delete enum variant exists
```

### Fix Required
1. Add `Operator::Delete` enum variant
2. Implement `plan_delete` in planner
3. Implement `execute_delete` in executor
4. Support `DETACH DELETE` (delete relationships first)

---

## Bug #2: CREATE Duplicating Nodes

### Symptoms
```cypher
CREATE (p:Person {name: 'Alice'})
CREATE (p:Person {name: 'Bob'})
-- Expected: 2 nodes
-- Actual: 24 nodes created
```

### Evidence
```powershell
# From debug-match-create.ps1
1. Creating Alice...
2. Creating Bob...

3. Counting nodes...
   Nodes: 24 (expected: 2)  ❌

8. Listing all nodes...
   - Labels: Person, Name: Alice      ✅ Intended
   - Labels: Person, Name: Bob        ✅ Intended
   - Labels: Person, Name: Charlie    ❌ Not created!
   - Labels: Person Employee, Name: David  ❌ Not created!
   - Labels: Company, Name: Acme Inc  ❌ Not created!
   - Labels: , Name:                  ❌ Garbage!
   ... (19 more duplicate/garbage nodes)
```

### Root Cause Analysis

**Pattern observed**:
- Creating 2 nodes → 24 nodes appear
- 24 nodes include:
  - 2 intended nodes (Alice, Bob)
  - 3 nodes from previous session (should have been deleted)
  - 19 garbage nodes (empty labels/names)

**Hypothesis 1: Multiple calls to `create_node`**
```rust
// nexus-core/src/lib.rs:443
fn execute_create_query(&mut self, ast: &CypherQuery) -> Result<()> {
    for clause in &ast.clauses {
        if let Clause::Create(create_clause) = clause {
            for element in create_clause.pattern.elements.iter() {
                match element {
                    PatternElement::Node(node) => {
                        let node_id = self.create_node(..)?;  // Called once per element
                    }
                }
            }
        }
    }
    Ok(())
}
```

**Hypothesis 2: `execute_cypher` called multiple times**
```rust
// nexus-core/src/lib.rs:622
pub fn execute_cypher(&mut self, query: &str) -> Result<ResultSet> {
    // Check if CREATE...
    if has_create {
        self.execute_create_query(&ast)?;  // Create here
        self.refresh_executor()?;
    }
    
    // Execute again!
    self.executor.execute(&query_obj)  // May create again? ⚠️
}
```

**Hypothesis 3: Transaction not committed properly**
- Multiple transactions created
- Each rollback/retry creates duplicates
- No transaction isolation

### Investigation Needed
- Add counter to `create_node` to track invocations
- Add logging to `execute_create_query` entry/exit
- Add logging to `refresh_executor`
- Verify transaction lifecycle

---

## Bug #3: Inline Property Filters Not Working

### Symptoms
```cypher
CREATE (p1:Person {name: 'Alice', age: 30})
CREATE (p2:Person {name: 'Bob', age: 25})

MATCH (p:Person {name: 'Alice'}) RETURN p
-- Expected: 1 row (Alice)
-- Actual: 7 rows (all Person nodes + duplicates)
```

### Evidence
```powershell
# From debug-filter.ps1
[TEST] MATCH (p:Person {name: 'Alice'}) RETURN p
   Rows: 7
Expected: 1 row, Actual: 7  ❌

[TEST] MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1.name, p2.name
   Rows: 7
Expected: 1 row (Alice x Bob), Actual: 7  ❌
```

### Root Cause Analysis

**Planner DOES create filters**:
```rust
// nexus-core/src/executor/planner.rs:223-242
// Add filters for inline properties: MATCH (n {property: value})
if let Some(property_map) = &node.properties {
    for (prop_name, prop_value_expr) in &property_map.properties {
        let value_str = match prop_value_expr {
            Expression::Literal(lit) => match lit {
                Literal::String(s) => format!("\"{}\"", s),  // ✅ Creates "Alice"
            },
        };
        let filter_expr = format!("{}.{} = {}", variable, prop_name, value_str);
        operators.push(Operator::Filter {
            predicate: filter_expr,  // ✅ Creates "p.name = \"Alice\""
        });
    }
}
```

**Executor DOES parse filters**:
```rust
// nexus-core/src/executor/mod.rs:748-804
fn execute_filter(&self, context: &mut ExecutionContext, predicate: &str) -> Result<()> {
    // Parse predicate
    let mut parser = parser::CypherParser::new(predicate.to_string());
    let expr = parser.parse_expression()?;  // ✅ Parses "p.name = \"Alice\""
    
    // Evaluate for each row
    for row in rows {
        if self.evaluate_predicate_on_row(&row, context, &expr)? {
            filtered_rows.push(row);
        }
    }
    
    // Update context
    self.update_variables_from_rows(context, &filtered_rows);
    self.update_result_set_from_rows(context, &filtered_rows);
}
```

**BinaryOp evaluation IS implemented**:
```rust
// nexus-core/src/executor/mod.rs:2296-2333
parser::Expression::BinaryOp { left, op, right } => {
    let left_val = self.evaluate_projection_expression(row, context, left)?;
    let right_val = self.evaluate_projection_expression(row, context, right)?;
    match op {
        parser::BinaryOperator::Equal => Ok(Value::Bool(left_val == right_val)),  // ✅
    }
}
```

### Hypotheses

**Hypothesis 1: `materialize_rows_from_variables` returns wrong data**
- Returns all nodes instead of filtered nodes
- Context variables not updated correctly

**Hypothesis 2: `evaluate_predicate_on_row` returns wrong boolean**
- Property access not working
- Value comparison failing
- Type mismatch (String vs Value::String)

**Hypothesis 3: `update_result_set_from_rows` doesn't filter**
- Filtered rows not applied to result set
- Result set keeps original rows

**Hypothesis 4: Filter applied AFTER NodeByLabel scan completes**
- NodeByLabel scans all Person nodes
- Filter should reduce, but doesn't
- Multiple NodeByLabel operators creating Cartesian product

### Investigation Needed
- Add logging: filter input row count vs output row count
- Add logging: `evaluate_predicate_on_row` result for each row
- Add logging: property values being compared
- Test with simple single-pattern query first

---

## Impact on Compatibility

### Before Bugs (v0.9.7)
- **Compatibility**: 70.59% (12/17 tests)
- **Features**: COUNT, UNION, aggregations working
- **Status**: Production-ready prototype

### After Bugs Discovered (current)
- **Compatibility**: 47.06% (8/17 tests)
- **Regression**: -23.53%
- **Status**: NOT PRODUCTION READY

### Failing Tests
1. Count all nodes - wrong count (duplicates)
2. Count nodes by label - wrong count (duplicates)
3. Get node properties - wrong row count (filter broken)
4. Count relationships - wrong count (duplicates)
5. Relationship properties - wrong row count (filter broken)
6. ORDER BY - wrong row count (filter broken)
7. UNION query - wrong row count (duplicates)
8. ID function - wrong row count (filter broken)
9. Type function - wrong row count (duplicates + filter)

---

## Testing Strategy

### Unit Tests Needed
```rust
#[test]
fn test_delete_single_node() {
    // Create 1 node, delete it, verify count = 0
}

#[test]
fn test_create_exact_count() {
    // Create 1 node, verify count = 1 (not 5-7)
}

#[test]
fn test_inline_filter_string() {
    // Create 2 nodes, filter by name, verify 1 row
}
```

### Integration Tests
```powershell
# Test DELETE
CREATE (p:Person {name: 'Test'})
MATCH (n) DETACH DELETE n
MATCH (n) RETURN count(*) as c
-- Expected: c = 0

# Test CREATE
MATCH (n) DETACH DELETE n
CREATE (p:Person {name: 'Alice'})
MATCH (n) RETURN count(*) as c
-- Expected: c = 1

# Test FILTER
MATCH (n) DETACH DELETE n
CREATE (p1:Person {name: 'Alice'})
CREATE (p2:Person {name: 'Bob'})
MATCH (p:Person {name: 'Alice'}) RETURN count(*) as c
-- Expected: c = 1
```

---

## Conclusion

All three bugs are **critical blockers** that must be fixed before any other development work. The bugs prevent basic Cypher operations and cause data corruption.

**Priority Order**:
1. **DELETE** - Highest priority (enables clean testing)
2. **CREATE** - High priority (stops data corruption)
3. **FILTER** - High priority (enables correct queries)

**Estimated Total Effort**: 14 hours (2 working days)

