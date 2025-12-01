---
title: Advanced Cypher
module: cypher
id: advanced-cypher
order: 2
description: Advanced queries and patterns
tags: [cypher, advanced, patterns, queries]
---

# Advanced Cypher

Advanced Cypher queries and patterns.

## Variable-Length Paths

### Path Quantifiers

```cypher
// Path of length 2-3
MATCH (a:Person)-[:KNOWS*2..3]->(b:Person) 
RETURN a.name, b.name

// Fixed length path
MATCH (a:Person)-[:KNOWS*5]->(b:Person) 
RETURN a.name, b.name

// Unbounded path
MATCH (a:Person)-[:KNOWS*]->(b:Person) 
RETURN a.name, b.name

// One or more
MATCH (a:Person)-[:KNOWS+]->(b:Person) 
RETURN a.name, b.name
```

## OPTIONAL MATCH

### Left Outer Join

```cypher
// Left outer join semantics
MATCH (a:Person)
OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
RETURN a.name, b.name
```

### Multiple Optional Matches

```cypher
MATCH (a:Person)
OPTIONAL MATCH (a)-[:KNOWS]->(b:Person)
OPTIONAL MATCH (a)-[:WORKS_AT]->(c:Company)
RETURN a.name, b.name, c.name
```

## Complex Patterns

### Multiple Patterns

```cypher
MATCH (a:Person)-[:KNOWS]->(b:Person),
      (b:Person)-[:KNOWS]->(c:Person),
      (c:Person)-[:KNOWS]->(a:Person)
RETURN a.name, b.name, c.name
```

### Pattern Composition

```cypher
MATCH (a:Person)-[:KNOWS]->(b:Person)-[:WORKS_AT]->(c:Company),
      (a:Person)-[:LIVES_IN]->(city:City)
RETURN a.name, b.name, c.name, city.name
```

## Subqueries

### EXISTS Subqueries

```cypher
MATCH (n:Person)
WHERE EXISTS {
  MATCH (n)-[:KNOWS]->(m:Person)
  WHERE m.age > 30
}
RETURN n.name
```

### CALL Subqueries

```cypher
CALL {
  MATCH (n:Person) RETURN n LIMIT 10
}
RETURN n
```

## List and Pattern Comprehensions

### List Comprehensions

```cypher
MATCH (n:Person)
RETURN [x IN n.skills WHERE x STARTS WITH 'R' | upper(x)] AS rust_skills
```

### Pattern Comprehensions

```cypher
MATCH (n:Person)
RETURN [(n)-[:KNOWS]->(m) | m.name] AS friends
```

## Map Projections

### Basic Map Projection

```cypher
MATCH (n:Person)
RETURN n {.name, .age, .email} AS person_info
```

### Nested Map Projection

```cypher
MATCH (n:Person)-[:KNOWS]->(m:Person)
RETURN n {
  .name,
  .age,
  friends: [(n)-[:KNOWS]->(f) | f {.name, .age}]
} AS person_with_friends
```

## WITH Clause

### Chain Query Parts

```cypher
MATCH (n:Person)
WITH n, n.age * 2 AS double_age
WHERE double_age > 50
RETURN n.name, double_age
```

### Aggregation with WITH

```cypher
MATCH (n:Person)
WITH n.city AS city, COUNT(n) AS count
WHERE count > 10
RETURN city, count
ORDER BY count DESC
```

## UNWIND

### Expand Lists

```cypher
UNWIND [1, 2, 3] AS number
RETURN number
```

### Process List Elements

```cypher
MATCH (n:Person)
UNWIND n.skills AS skill
RETURN skill, COUNT(n) AS people_with_skill
ORDER BY people_with_skill DESC
```

## UNION

### Combine Results

```cypher
MATCH (n:Person) RETURN n.name AS name
UNION
MATCH (n:Company) RETURN n.name AS name
```

## FOREACH

### Iterate and Modify

```cypher
MATCH (n:Person)
FOREACH (x IN [1, 2, 3] |
  CREATE (n)-[:HAS_TAG]->(:Tag {id: x})
)
```

## CASE Expression

### Conditional Expressions

```cypher
MATCH (n:Person)
RETURN 
  n.name,
  CASE n.age
    WHEN 30 THEN 'Thirty'
    WHEN 40 THEN 'Forty'
    ELSE 'Other'
  END AS age_label
```

### Searched CASE

```cypher
MATCH (n:Person)
RETURN 
  n.name,
  CASE
    WHEN n.age < 18 THEN 'Minor'
    WHEN n.age < 65 THEN 'Adult'
    ELSE 'Senior'
  END AS age_category
```

## Related Topics

- [Cypher Basics](./BASIC.md) - Basic syntax
- [Functions Reference](./FUNCTIONS.md) - All functions
- [Complete Cypher Guide](./CYPHER.md) - Comprehensive reference

