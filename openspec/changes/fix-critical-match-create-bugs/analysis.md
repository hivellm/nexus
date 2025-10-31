# Analysis: Critical MATCH and CREATE Bugs

**Status**: 🔴 Critical  
**Created**: 2025-10-31  

---

## Executive Summary

Three critical bugs discovered during Neo4j compatibility testing:

1. **Inline property filters don't work** - affects basic queries
2. **DELETE operations don't remove data** - breaks data cleanup
3. **CREATE duplicates nodes** - corrupts database

These bugs prevent basic Cypher functionality and dropped compatibility from 70% to 47%.

---

## Bug #1: Inline Property Filters Not Working

### Symptoms

```cypher
-- Expected: 1 row (Alice only)
MATCH (p:Person {name: 'Alice'}) RETURN p

-- Actual: 7 rows (all Person nodes + garbage)
```

### Evidence

```powershell
# Test results from debug-filter.ps1
[TEST] MATCH (p:Person {name: 'Alice'}) RETURN p
   Rows: 7
Expected: 1 row, Actual: 7  ❌

[TEST] MATCH (p1:Person {name: 'Alice'}), (p2:Person {name: 'Bob'}) RETURN p1.name, p2.name
   Rows: 7
Expected: 1 row (Alice x Bob), Actual: 7  ❌
```

### Root Cause Analysis

**Planner** (`nexus-core/src/executor/planner.rs:223-242`):
```rust
// Add filters for inline properties: MATCH (n {property: value})
if let Some(property_map) = &node.properties {
    for (prop_name, prop_value_expr) in &property_map.properties {
        let value_str = match prop_value_expr {
            Expression::Literal(lit) => match lit {
                Literal::String(s) => format!("\"{}\"", s),
                // ...
            },
            _ => self.expression_to_string(prop_value_expr)?,
        };
        let filter_expr = format!("{}.{} = {}", variable, prop_name, value_str);
        operators.push(Operator::Filter {
            predicate: filter_expr,  // ✅ Creates "p.name = \"Alice\""
        });
    }
}
```

**Executor** (`nexus-core/src/executor/mod.rs:748-804`):
```rust
fn execute_filter(&self, context: &mut ExecutionContext, predicate: &str) -> Result<()> {
    // Check for label check pattern: variable:Label
    if predicate.contains(':') && !predicate.contains("::") {
        // ... label filtering works ✅
    }

    // Regular predicate expression
    let mut parser = parser::CypherParser::new(predicate.to_string());
    let expr = parser.parse_expression()?;  // ✅ Parses "p.name = \"Alice\""

    let rows = self.materialize_rows_from_variables(context);
    let mut filtered_rows = Vec::new();

    for row in rows {
        if self.evaluate_predicate_on_row(&row, context, &expr)? {  // ⚠️ Should evaluate
            filtered_rows.push(row);
        }
    }

    self.update_variables_from_rows(context, &filtered_rows);
    self.update_result_set_from_rows(context, &filtered_rows);  // ⚠️ Should update
    Ok(())
}
```

**Evaluation** (`nexus-core/src/executor/mod.rs:2296-2333`):
```rust
parser::Expression::BinaryOp { left, op, right } => {
    let left_val = self.evaluate_projection_expression(row, context, left)?;
    let right_val = self.evaluate_projection_expression(row, context, right)?;
    match op {
        parser::BinaryOperator::Equal => Ok(Value::Bool(left_val == right_val)),  // ✅ Implemented
        // ...
    }
}
```

### Hypothesis

1. **Filter IS being created correctly** ✅
2. **Parser IS parsing the expression** ✅
3. **BinaryOp evaluation IS implemented** ✅
4. **Problem**: One of these is failing:
   - `materialize_rows_from_variables` returns wrong data
   - `evaluate_predicate_on_row` returns wrong boolean
   - `update_result_set_from_rows` doesn't actually filter

### Investigation Needed

- Add logging to `execute_filter` to see if it's called
- Add logging to `evaluate_predicate_on_row` to see what it returns
- Add logging to `update_result_set_from_rows` to verify row count changes

---

## Bug #2: DETACH DELETE Not Working

### Symptoms

```cypher
MATCH (n) DETACH DELETE n
-- Database still shows 31+ nodes after DELETE
```

### Evidence

```powershell
# After "clean" operation
[COUNT] After creating 2 nodes...
   Rows: 31  ❌ (should be 2)
```

### Root Cause Analysis

**Possible Causes**:

1. **DELETE operator not implemented**
   - Search for `Operator::Delete` in codebase: NOT FOUND ❌
   - `execute` function doesn't have a `Delete` case

2. **Parser doesn't recognize DELETE**
   - Search for `Clause::Delete`: EXISTS in parser ✅
   - But planner doesn't generate `Delete` operator

3. **DELETE implementation missing**
   - No `execute_delete` function found

### Hypothesis

`DELETE` and `DETACH DELETE` clauses are **not implemented** in the executor.

### Investigation Needed

- Verify `DELETE` clause is parsed correctly
- Implement `Operator::Delete` in planner
- Implement `execute_delete` in executor
- Ensure `RecordStore::delete_node` marks records as deleted

---

## Bug #3: CREATE Duplicating Nodes

### Symptoms

```cypher
CREATE (p:Person {name: 'Alice'})
CREATE (p:Person {name: 'Bob'})
-- Creates 5-7 nodes instead of 2
```

### Evidence

```powershell
1. Creating Alice...
2. Creating Bob...

3. Counting nodes...
   Nodes: 24 (expected: 2)  ❌

8. Listing all nodes...
   - Labels: Person, Name: Alice
   - Labels: Person, Name: Bob
   - Labels: Person, Name: Charlie  ❌ (not created!)
   - Labels: Person Employee, Name: David  ❌ (not created!)
   - Labels: Company, Name: Acme Inc  ❌ (not created!)
   - Labels: , Name:   ❌ (garbage!)
   ... (19 more duplicate/garbage nodes)
```

### Root Cause Analysis

**Observation**: Creating 2 nodes results in 24 nodes, including:
- The 2 intended nodes (Alice, Bob)
- 3 nodes from previous session (Charlie, David, Acme Inc) - **DELETE bug**
- 19 garbage nodes with no labels/names

**execute_create_query** (`nexus-core/src/lib.rs:443-618`):
```rust
fn execute_create_query(&mut self, ast: &executor::parser::CypherQuery) -> Result<()> {
    let mut created_nodes: HashMap<String, u64> = HashMap::new();

    for clause in &ast.clauses {
        if let executor::parser::Clause::Create(create_clause) = clause {
            for (i, element) in create_clause.pattern.elements.iter().enumerate() {
                match element {
                    executor::parser::PatternElement::Node(node) => {
                        // Create node using Engine API
                        let node_id = self.create_node(node.labels.clone(), properties)?;  // ✅ Should create 1
                        // ...
                    }
                    // ...
                }
            }
        }
    }
    Ok(())
}
```

### Hypothesis

1. **execute_create_query is being called multiple times** for a single CREATE
2. **create_node** itself is creating duplicates
3. **refresh_executor** is triggering additional creates
4. **Garbage nodes** suggest memory corruption or transaction rollback issues

### Investigation Needed

- Add counter/logging to `execute_create_query` calls
- Add logging to `create_node` to track each creation
- Verify transaction is committed only once
- Check if `refresh_executor` causes side effects

---

## Impact Assessment

### Compatibility

| Before | After | Change |
|--------|-------|--------|
| 70.59% (12/17) | 47.06% (8/17) | -23.53% ❌ |

### Failing Tests

1. Count all nodes (10 vs 22)
2. Count nodes by label (8 vs 22)
3. Get node properties (4 vs 5 rows)
4. Count relationships (2 vs 12)
5. Relationship properties (2 vs 5 rows)
6. ORDER BY (4 vs 5 rows)
7. UNION query (5 vs 6 rows)
8. ID function (4 vs 5 rows)
9. Type function (3 vs 5 rows)

### Data Integrity

- Database cannot be cleaned (DELETE broken)
- Each CREATE multiplies garbage (CREATE duplication)
- Queries return wrong results (filter broken)

**Conclusion**: System is **NOT PRODUCTION READY** until these bugs are fixed.

---

## Recommended Fix Order

1. **DELETE first** - enables clean testing environment
2. **CREATE duplication** - stops data corruption
3. **Inline filters** - enables correct queries

---

## Testing Strategy

### Unit Tests

- `test_execute_filter_inline_property`
- `test_delete_single_node`
- `test_delete_all_nodes`
- `test_create_single_node_no_duplicates`

### Integration Tests

- `test_match_with_inline_filters`
- `test_detach_delete_cleans_database`
- `test_match_create_correct_count`

### Compatibility Tests

- Re-run `test-compatibility.ps1` after each fix
- Target: >80% compatibility

---

## Conclusion

These are **critical blocking bugs** that must be fixed before any other work. The system is currently unusable for basic Cypher operations.

**Estimated effort**: 14 hours (2 days)  
**Risk**: High (core functionality)  
**Priority**: Urgent

