# Cypher Graph Scoping Spec

## ADDED Requirements

### Requirement: `GRAPH[name]` Preamble

The parser SHALL accept an optional `GRAPH[name]` clause at the
start of a query. When present, the query SHALL be executed against
the named database rather than the session's default database.

#### Scenario: Query runs in named graph
Given databases `default` (empty) and `analytics` (containing one
  node `(:X {v: 42})`)
And the session's current database is `default`
When `GRAPH[analytics] MATCH (n:X) RETURN n.v` is executed
Then the result SHALL contain one row with value `42`
And the session's current database SHALL remain `default`

### Requirement: Unknown Graph Rejected

If the named graph does not exist, the query SHALL fail with
`ERR_GRAPH_NOT_FOUND(name)`.

#### Scenario: Missing graph
Given no database named `ghost`
When `GRAPH[ghost] MATCH (n) RETURN n` is executed
Then the server SHALL respond with HTTP 404
And the error code SHALL be `ERR_GRAPH_NOT_FOUND`

### Requirement: Access Control

Resolving `GRAPH[name]` SHALL check that the caller has read access
to the named database. Lack of access SHALL raise
`ERR_GRAPH_ACCESS_DENIED(name)`.

#### Scenario: Unauthorised caller
Given a user without access to database `analytics`
When the user issues `GRAPH[analytics] MATCH (n) RETURN n`
Then the server SHALL respond with HTTP 403
And the error code SHALL be `ERR_GRAPH_ACCESS_DENIED`

### Requirement: Scope Does Not Leak Into Writes

`GRAPH[name]` SHALL scope both reads and writes. A `CREATE` inside
the scope SHALL materialise in the named database, not the session
database.

#### Scenario: CREATE goes to named graph
Given databases `default` and `analytics`
And the session's current database is `default`
When `GRAPH[analytics] CREATE (n:X {v: 1}) RETURN n` is executed
Then a new node SHALL exist in database `analytics`
And database `default` SHALL be unchanged

### Requirement: Out of Scope — Multi-Graph Queries

A single query SHALL contain at most one `GRAPH[...]` clause, which
MUST appear as the first syntactic element. Multiple clauses or
placement elsewhere SHALL be rejected at parse time.

#### Scenario: Second GRAPH rejected
Given the query `GRAPH[a] MATCH (n) GRAPH[b] RETURN n`
When parsed
Then parsing SHALL fail with a syntax error
