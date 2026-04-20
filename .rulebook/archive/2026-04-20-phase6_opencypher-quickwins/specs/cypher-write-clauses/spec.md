# Cypher Write Clauses — `SET +=` Map Merge Spec

## MODIFIED Requirements

### Requirement: `SET lhs += mapExpr` — Map Merge Semantics

The parser SHALL accept `SET lhs += mapExpr` as a distinct form from
`SET lhs = mapExpr`. The writer SHALL merge `mapExpr` into the target's
existing property bag:

- Keys present in `mapExpr` with non-NULL values SHALL overwrite the
  target's existing values.
- Keys present in `mapExpr` with NULL values SHALL be removed from the
  target.
- Keys absent from `mapExpr` but present on the target SHALL be
  preserved.

This contrasts with `SET lhs = mapExpr` which REPLACES the entire
property bag.

#### Scenario: Merge preserves existing keys
Given a node `(a:Person {name: "Alice", age: 30})`
When `MATCH (n:Person) SET n += {city: "Berlin"} RETURN n` is executed
Then `n.name` SHALL equal `"Alice"`
And `n.age` SHALL equal `30`
And `n.city` SHALL equal `"Berlin"`

#### Scenario: Merge overwrites matching keys
Given a node `(a:Person {name: "Alice", age: 30})`
When `MATCH (n:Person) SET n += {age: 31} RETURN n` is executed
Then `n.age` SHALL equal `31`
And `n.name` SHALL equal `"Alice"` (preserved)

#### Scenario: NULL value deletes the key
Given a node `(a:Person {name: "Alice", age: 30})`
When `MATCH (n:Person) SET n += {age: null} RETURN n` is executed
Then `n.age` SHALL be absent from the returned property bag
And `n.name` SHALL equal `"Alice"` (preserved)

#### Scenario: Contrast with SET = replace
Given a node `(a:Person {name: "Alice", age: 30})`
When `MATCH (n:Person) SET n = {city: "Berlin"} RETURN n` is executed
Then `n.name` SHALL be absent
And `n.age` SHALL be absent
And `n.city` SHALL equal `"Berlin"`

### Requirement: Non-Map RHS Raises Error

When the RHS of `SET lhs += rhs` evaluates to a value that is neither a
MAP nor NULL, the executor SHALL raise `ERR_SET_NON_MAP(got="<type>")`.

#### Scenario: List RHS rejected
Given a node `(a:Person)`
When `MATCH (n:Person) SET n += [1, 2, 3]` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_SET_NON_MAP`

### Requirement: NULL RHS Is a No-Op

When the RHS evaluates to NULL, `SET lhs += NULL` SHALL be a no-op
(leave the target's property bag unchanged).

#### Scenario: NULL RHS
Given a node `(a:Person {name: "Alice"})`
When `MATCH (n:Person) SET n += $missing RETURN n` is executed with
  parameters `{}`
Then `n.name` SHALL remain `"Alice"`
