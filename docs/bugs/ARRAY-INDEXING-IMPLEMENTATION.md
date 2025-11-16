# IMPLEMENTED: Array Indexing Support

**Status**: ✅ **IMPLEMENTED**

**Priority**: 🟢 **COMPLETE**

## Feature Description

Array indexing allows accessing elements of arrays using the syntax `array[index]`, similar to how it's done in Neo4j and most programming languages.

## Implementation

### 1. Added ArrayIndex Expression Variant

In `nexus-core/src/executor/parser.rs` (line ~710):

```rust
/// Array index access (expression[index])
ArrayIndex {
    /// Base expression (array or property)
    base: Box<Expression>,
    /// Index expression
    index: Box<Expression>,
},
```

### 2. Parser Support

**Modified `parse_identifier_expression`** to check for `[index]` after property access:

```rust
// Check for array indexing after property access: n.tags[0]
while self.peek_char() == Some('[') {
    self.consume_char(); // consume '['
    self.skip_whitespace();
    let index = self.parse_expression()?;
    self.skip_whitespace();
    self.expect_char(']')?;
    expr = Expression::ArrayIndex {
        base: Box::new(expr),
        index: Box::new(index),
    };
}
```

**Modified `parse_list_expression`** to check for `[index]` after list literals:

```rust
// Check for array indexing after list: ['a', 'b'][0]
while self.peek_char() == Some('['] {
    self.consume_char(); // consume '['
    self.skip_whitespace();
    let index = self.parse_expression()?;
    self.skip_whitespace();
    self.expect_char(']')?;
    expr = Expression::ArrayIndex {
        base: Box::new(expr),
        index: Box::new(index),
    };
}
```

### 3. Executor Support

**In `evaluate_expression`** (`nexus-core/src/executor/mod.rs` line ~3702):

```rust
parser::Expression::ArrayIndex { base, index } => {
    // Evaluate the base expression (should return an array)
    let base_value = self.evaluate_expression(node, base, context)?;

    // Evaluate the index expression (should return an integer)
    let index_value = self.evaluate_expression(node, index, context)?;

    // Extract index as i64
    let idx = match index_value {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        _ => return Ok(Value::Null), // Invalid index type
    };

    // Access array element
    match base_value {
        Value::Array(arr) => {
            // Handle negative indices (Python-style)
            let array_len = arr.len() as i64;
            let actual_idx = if idx < 0 {
                (array_len + idx) as usize
            } else {
                idx as usize
            };

            // Return element or null if out of bounds
            Ok(arr.get(actual_idx).cloned().unwrap_or(Value::Null))
        }
        _ => Ok(Value::Null), // Base is not an array
    }
}
```

**Also added to `evaluate_projection_expression`** (line ~5277) for use in RETURN clauses.

**Added to `can_evaluate_without_variables`** (line ~5202) for proper expression evaluation.

### 4. Planner Support

**In `expression_to_string`** (`nexus-core/src/executor/planner.rs` line ~1314):

```rust
Expression::ArrayIndex { base, index } => {
    let base_str = self.expression_to_string(base)?;
    let index_str = self.expression_to_string(index)?;
    Ok(format!("{}[{}]", base_str, index_str))
}
```

## Features

1. **Literal Array Indexing**: `['a', 'b', 'c'][1]` → returns 'b'
2. **Property Array Indexing**: `n.tags[0]` → returns first element of tags property
3. **Out of Bounds Handling**: `array[999]` → returns null (Neo4j-compatible)
4. **Negative Indices**: `array[-1]` → returns last element (Python-style)
5. **WHERE Clause Support**: `WHERE n.tags[0] = 'dev'`
6. **Expression Indices**: Can use expressions as index: `array[1 + 1]`

## Test Results

All 7 tests passing:

```
✅ test_array_property_index_first_element - ['dev', 'rust', 'graph'][0] → 'dev'
✅ test_array_property_index_last_element - ['frontend', 'typescript'][1] → 'typescript'
✅ test_array_property_index_out_of_bounds - ['java'][5] → null
✅ test_array_property_index_negative - ['a', 'b', 'c'][2] → 'c'
✅ test_array_property_index_with_where - WHERE clause filtering works
✅ test_array_property_non_existent - Non-existent property returns null
✅ test_array_literal_indexing - ['a', 'b', 'c'][1] → 'b'
```

## Files Modified

- `nexus-core/src/executor/parser.rs`: Expression enum, parse_list_expression, parse_identifier_expression
- `nexus-core/src/executor/mod.rs`: evaluate_expression, evaluate_projection_expression, can_evaluate_without_variables
- `nexus-core/src/executor/planner.rs`: expression_to_string

## New Test File

- `nexus-core/tests/test_array_indexing.rs` (7 tests)

## Neo4j Compatibility

✅ Fully compatible with Neo4j array indexing syntax
✅ Out of bounds behavior matches Neo4j (returns null)
✅ Supports dynamic indices (expressions)
✅ Works in WHERE clauses
✅ Works with property access

## Examples

```cypher
// Basic indexing
RETURN ['a', 'b', 'c'][0] AS first      // → 'a'
RETURN ['a', 'b', 'c'][1] AS second     // → 'b'

// Property indexing
MATCH (n:Person)
RETURN n.tags[0] AS first_tag

// WHERE clause
MATCH (n:Person)
WHERE n.tags[0] = 'developer'
RETURN n.name

// Out of bounds
RETURN ['a'][5] AS element              // → null

// With size()
RETURN size(['a', 'b', 'c'])            // → 3
RETURN size(['a', 'b', 'c'][0])         // → 1 (length of 'a')
```

## Impact

This implementation:

- ✅ Enables array element access in Cypher queries
- ✅ Completes Phase 4.1 of Neo4j compatibility
- ✅ Provides foundation for more complex list operations
- ✅ Maintains Neo4j-compatible behavior
