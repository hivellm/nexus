# Cypher Dynamic Property Access Spec

## ADDED Requirements

### Requirement: Read Property by Dynamic Key

The parser SHALL accept `expr '[' keyExpr ']'` as a property-access
form when the LHS resolves to a NODE or RELATIONSHIP at runtime. The
evaluator SHALL coerce `keyExpr` to STRING and return the named
property, or NULL if the property is absent.

#### Scenario: Read property via parameter
Given a node `(a:Person {name: "Alice", age: 30})`
And the parameter `{"k": "name"}`
When the query `MATCH (n:Person) RETURN n[$k]` is executed
Then the result column 0 row 0 SHALL equal `"Alice"`

#### Scenario: Absent property returns NULL
Given a node `(a:Person {name: "Alice"})`
And the parameter `{"k": "email"}`
When the query `MATCH (n:Person) RETURN n[$k]` is executed
Then the result column 0 row 0 SHALL equal `null`

### Requirement: Write Property by Dynamic Key

The parser SHALL accept `lhs '[' keyExpr ']'` as the LHS of `SET`. The
writer SHALL insert or overwrite the named property, coercing
`keyExpr` to STRING.

#### Scenario: Set property via parameter
Given an empty database and parameter `{"k": "age", "v": 30}`
When the query `CREATE (a:Person {name: "Alice"}) SET a[$k] = $v RETURN a.age`
  is executed
Then the result SHALL be `30`

### Requirement: Invalid Key Raises Error

When `keyExpr` does not evaluate to STRING or NULL, the executor SHALL
raise `ERR_INVALID_KEY(got="<type>")`.

#### Scenario: Integer key rejected
Given the query `MATCH (n) RETURN n[42]`
When the query is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_KEY`

### Requirement: NULL Key Returns NULL (Three-Valued Logic)

When `keyExpr` evaluates to NULL, the access SHALL return NULL without
raising an error (read path) or become a no-op (write path).

#### Scenario: NULL key on read
Given a node `(a:Person {name: "Alice"})`
And the parameter `{"k": null}`
When the query `MATCH (n:Person) RETURN n[$k]` is executed
Then the result column 0 row 0 SHALL equal `null`

### Requirement: List Indexing Remains Unchanged

Dynamic property access SHALL NOT alter the semantics of `list[i]` list
indexing when the LHS is a LIST.

#### Scenario: List indexing still works
Given the query `RETURN [10, 20, 30][1]`
When the query is executed
Then the result SHALL be `20`
