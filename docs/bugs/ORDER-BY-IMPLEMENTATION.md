# FIXED: ORDER BY Implementation

**Status**: ✅ **FIXED**

**Priority**: 🟢 **RESOLVED**

## Problem Description

ORDER BY was not implemented at all:
- Sort operator was being added in wrong order (before Project instead of after)
- Column names weren't being resolved to aliases
- execute_sort was rebuilding rows, breaking column order

## Root Causes

### Problem 1: Operator Execution Order

The planner was adding the Sort operator during the clause loop, which meant it was added BEFORE the MATCH and Project operators were added.

**Order was**:
1. Sort (added during clause loop)
2. NodeByLabel (added in plan_execution_strategy)
3. Project (added in plan_execution_strategy)

**Should be**:
1. NodeByLabel
2. Project
3. Sort

### Problem 2: Column Name Resolution

ORDER BY expressions like `n.age` weren't being resolved to their RETURN aliases like `age`.

```
RETURN n.name AS name, n.age AS age ORDER BY n.age DESC
```

The Sort operator was looking for column `"n.age"` but the result_set had column `"age"`.

### Problem 3: Row Rebuild After Sort

The `execute_sort` function was calling:
```rust
let row_maps = self.result_set_as_rows(context);
self.update_variables_from_rows(context, &row_maps);
self.update_result_set_from_rows(context, &row_maps);
```

This was rebuilding the rows after sorting, which inverted the column order!

## Solution

### Fix 1: Collect ORDER BY, Add After Projection

In `nexus-core/src/executor/planner.rs`:

**Line 104**: Added variable to collect ORDER BY clause:
```rust
let mut order_by_clause: Option<(Vec<String>, Vec<bool>)> = None;
```

**Lines 175-193**: Collect ORDER BY instead of adding immediately:
```rust
Clause::OrderBy(order_by_clause_parsed) => {
    // Collect ORDER BY clause to add after projection
    let mut columns = Vec::new();
    let mut ascending = Vec::new();
    
    for item in &order_by_clause_parsed.items {
        let column = self.expression_to_string(&item.expression)?;
        columns.push(column);
        
        let is_asc = item.direction == SortDirection::Ascending;
        ascending.push(is_asc);
    }
    
    order_by_clause = Some((columns, ascending));
}
```

**Lines 536-568**: Add Sort AFTER Project, BEFORE Limit with alias resolution:
```rust
if let Some((columns, ascending)) = order_by_clause {
    // Build expression -> alias map
    let mut expression_to_alias = std::collections::HashMap::new();
    for item in &return_items {
        let expr_str = self.expression_to_string(&item.expression).unwrap_or_default();
        let alias = item.alias.clone().unwrap_or_else(|| expr_str.clone());
        expression_to_alias.insert(expr_str, alias);
    }
    
    // Resolve ORDER BY expressions to aliases
    let resolved_columns: Vec<String> = columns.iter().map(|col| {
        expression_to_alias.get(col).cloned().unwrap_or_else(|| col.clone())
    }).collect();
    
    // Insert Sort before Limit if exists, otherwise at end
    let limit_pos = operators.iter().position(|op| matches!(op, Operator::Limit { .. }));
    let sort_op = Operator::Sort { columns: resolved_columns, ascending };
    
    if let Some(pos) = limit_pos {
        operators.insert(pos, sort_op);
    } else {
        operators.push(sort_op);
    }
}
```

### Fix 2: Remove Row Rebuild

In `nexus-core/src/executor/mod.rs` (lines 1524-1560):

**Removed** the row rebuild after sort:
```rust
// Don't rebuild rows after sort - it breaks the column order!
// The rows are already sorted in place.
Ok(())
```

### Fix 3: Add SortDirection Import

In `nexus-core/src/executor/planner.rs` (line 1):

```rust
use super::parser::{
    BinaryOperator, Clause, CypherQuery, Expression, Literal, Pattern, PatternElement, QueryHint,
    RelationshipDirection, ReturnItem, SortDirection, UnaryOperator,
};
```

## Files Modified

- `nexus-core/src/executor/planner.rs`:
  - Line 1: Added `SortDirection` import
  - Line 104: Added `order_by_clause` variable
  - Lines 175-193: ORDER BY collection logic
  - Lines 536-568: ORDER BY resolution and insertion

- `nexus-core/src/executor/mod.rs`:
  - Lines 1524-1560: Removed row rebuild from `execute_sort`

## Test Results

All ORDER BY scenarios now work correctly:

```
✅ ORDER BY DESC: MATCH (n:P) RETURN n.name, n.age ORDER BY n.age DESC
   → Returns: Charlie(35), Alice(30), Bob(25)

✅ ORDER BY Multiple Columns: ORDER BY n.age, n.name
   → Returns: Charlie(25), Alice(30), Bob(30) (sorted by age, then name)

✅ ORDER BY with WHERE: WHERE n.age > 25 ORDER BY n.age DESC
   → Returns: Charlie(35), Alice(30) (filtered, then sorted)

✅ ORDER BY with Aggregation: RETURN n.city, count(n) AS count ORDER BY count DESC
   → Works correctly with alias resolution
```

## Impact

This fix:
- ✅ Enables proper ORDER BY functionality
- ✅ Supports DESC and ASC ordering
- ✅ Supports multiple column ordering
- ✅ Works with WHERE clauses
- ✅ Works with aggregations
- ✅ Completes Phase 3 of Neo4j compatibility

## Related Issues

- Phase 1 (Aggregation) completed
- Phase 2 (WHERE IN) completed
- Phase 3 (ORDER BY) now completed ✅

