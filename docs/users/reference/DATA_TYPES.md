---
title: Data Types
module: reference
id: data-types
order: 3
description: Supported data types
tags: [data-types, reference, types]
---

# Data Types

Complete reference for all supported data types in Nexus.

## Primitive Types

### Integer

```cypher
// Integer literals
42
-10
0
```

### Float

```cypher
// Float literals
3.14
-0.5
1.0
```

### String

```cypher
// String literals
"hello"
'world'
```

### Boolean

```cypher
// Boolean literals
true
false
```

### Null

```cypher
// Null literal
null
```

## Collection Types

### List

```cypher
// List literals
[1, 2, 3]
["a", "b", "c"]
[1, "mixed", true]
```

### Map

```cypher
// Map literals
{name: "Alice", age: 30}
{key: "value"}
```

## Temporal Types

### Date

```cypher
// Date functions
date()
date("2025-01-01")
```

### DateTime

```cypher
// DateTime functions
datetime()
datetime("2025-01-01T10:00:00Z")
```

### Time

```cypher
// Time functions
time()
time("10:00:00")
```

### Duration

```cypher
// Duration literals
duration({days: 1, hours: 2})
duration({months: 3})
```

## Geospatial Types

### Point

```cypher
// Point literals
point({x: 1, y: 2, crs: 'cartesian'})
point({x: -122.4194, y: 37.7749, crs: 'wgs-84'})
```

## Vector Types

### Vector (List of Floats)

```cypher
// Vector literals
[0.1, 0.2, 0.3, 0.4]
[0.5, 0.6, 0.7, 0.8]
```

## Type Coercion

### Automatic Coercion

```cypher
// Integer to Float
RETURN 1 + 1.5 AS result  // 2.5

// String concatenation
RETURN "hello" + " " + "world" AS greeting
```

## Type Checking

### IS NULL / IS NOT NULL

```cypher
MATCH (n:Person)
WHERE n.email IS NOT NULL
RETURN n
```

## Related Topics

- [Cypher Guide](../cypher/CYPHER.md) - Cypher query language
- [Functions Reference](./FUNCTIONS.md) - Type functions
- [Error Reference](./ERRORS.md) - Type errors

