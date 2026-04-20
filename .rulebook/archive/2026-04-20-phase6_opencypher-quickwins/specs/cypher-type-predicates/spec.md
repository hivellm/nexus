# Cypher Type-Checking Predicates Spec

## ADDED Requirements

### Requirement: Type-Check Predicate Registry

The system SHALL expose nine scalar predicate functions that return
BOOLEAN indicating the runtime type of their single argument:
`isInteger`, `isFloat`, `isString`, `isBoolean`, `isList`, `isMap`,
`isNode`, `isRelationship`, `isPath`.

#### Scenario: Integer value is INTEGER
Given the query `RETURN isInteger(42)`
When the query is executed
Then the result SHALL be `true`

#### Scenario: Float is not INTEGER
Given the query `RETURN isInteger(3.14)`
When the query is executed
Then the result SHALL be `false`

#### Scenario: NULL propagates as NULL
Given the query `RETURN isInteger(null)`
When the query is executed
Then the result SHALL be `null` (three-valued logic)

#### Scenario: Node argument is NODE
Given an existing node `(a:Person {name: "Alice"})`
When the query `MATCH (n) RETURN isNode(n)` is executed
Then the result SHALL be `true` for every matched node

### Requirement: Predicates Must Be Registered Under Exact openCypher Names

All nine predicates SHALL be callable with their canonical openCypher
names (case-sensitive exact match is not required; Cypher is
case-insensitive for function names).

#### Scenario: Case-insensitive call
Given the query `RETURN ISINTEGER(1), isinteger(1), isInteger(1)`
When the query is executed
Then all three columns SHALL return `true`

### Requirement: isList Detects Lists Independent of Element Type

`isList` SHALL return `true` for any LIST value, including the empty
list, regardless of element type.

#### Scenario: Empty list
Given the query `RETURN isList([])`
When the query is executed
Then the result SHALL be `true`

#### Scenario: Mixed-type list
Given the query `RETURN isList([1, "a", null])`
When the query is executed
Then the result SHALL be `true`

### Requirement: Graph Type Predicates Reject Scalars

`isNode`, `isRelationship`, `isPath` SHALL return `false` when applied
to any non-graph value (including NULL → NULL per three-valued logic).

#### Scenario: String is not a node
Given the query `RETURN isNode("hello")`
When the query is executed
Then the result SHALL be `false`
