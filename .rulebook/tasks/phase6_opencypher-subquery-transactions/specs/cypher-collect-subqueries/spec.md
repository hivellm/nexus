# `COLLECT {}` Subquery Spec

## ADDED Requirements

### Requirement: Simple Projection Returns List of Scalars

The expression `COLLECT { query-returning-one-column }` SHALL evaluate
to a LIST of that column's values, one per emitted row, in emission
order.

#### Scenario: Collect names
Given nodes `(:Person {name: "Alice"})`, `(:Person {name: "Bob"})`
When
  `RETURN COLLECT { MATCH (p:Person) RETURN p.name ORDER BY p.name } AS names`
  is executed
Then `names` SHALL equal `["Alice", "Bob"]`

### Requirement: Multi-Column Projection Returns List of Maps

When the inner query returns multiple columns, the collect subquery
SHALL produce a LIST of MAPs keyed by the column names.

#### Scenario: Collect records
Given the same nodes
When
  `RETURN COLLECT { MATCH (p:Person) RETURN p.name AS n, 1 AS x } AS rows`
  is executed
Then `rows` SHALL equal `[{n: "Alice", x: 1}, {n: "Bob", x: 1}]`

### Requirement: Aggregating Inner Returns Single-Element List

When the inner query contains an aggregation and returns exactly one
row, the collect subquery SHALL produce a one-element list.

#### Scenario: Aggregate count
Given three Person nodes
When
  `RETURN COLLECT { MATCH (p:Person) RETURN count(p) AS c } AS counts`
  is executed
Then `counts` SHALL equal `[3]`

### Requirement: Empty Subquery Returns Empty List

When the inner query emits zero rows, the collect subquery SHALL
produce the empty list `[]`, NOT NULL.

#### Scenario: No matches
Given an empty database
When `RETURN COLLECT { MATCH (p:Person) RETURN p.name } AS names`
  is executed
Then `names` SHALL equal `[]`

### Requirement: Nested Collect Subqueries

A collect subquery MAY be nested inside another collect subquery. The
inner evaluator SHALL maintain its own row stream.

#### Scenario: Nested collect
Given nodes `(:Team)-[:HAS]->(:Member)` where two teams each have
  two members
When
  `MATCH (t:Team) RETURN COLLECT {
     MATCH (t)-[:HAS]->(m) RETURN COLLECT { RETURN m.name } AS members
   } AS all`
  is executed
Then the result SHALL be a LIST of LISTs matching the team/member structure
