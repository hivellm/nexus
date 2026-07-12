# HTTP write-path relationship parity

## ADDED Requirements

### Requirement: MERGE creates relationships over HTTP

The HTTP `/cypher` write path SHALL create a relationship when a `MERGE`
relationship pattern does not already match, persisting the pattern's
inline properties, and SHALL be idempotent on re-execution.

#### Scenario: MERGE creates a new relationship with inline properties
Given two nodes `(a:N {id:1})` and `(b:N {id:2})` exist
When the client executes `MERGE (a)-[r:LINK {w:5}]->(b)` over `/cypher`
Then a single `:LINK` relationship from `a` to `b` exists with `r.w = 5`
And re-executing the same `MERGE` leaves exactly one `:LINK` relationship with `r.w = 5`

#### Scenario: MERGE with a parameterized inline property
Given two nodes `(a:M {id:1})` and `(b:M {id:2})` exist
When the client executes `MERGE (a)-[r:REL {score:$s}]->(b)` with `{"s":42}`
Then a `:REL` relationship exists with `r.score = 42`

### Requirement: SET on a relationship variable persists over HTTP

The HTTP `/cypher` write path SHALL bind relationship variables from
`MATCH` and `MERGE` patterns and SHALL apply `SET` on those variables to
the stored relationship.

#### Scenario: SET a property on a matched relationship variable
Given a relationship `(a)-[r:LINK]->(b)` exists
When the client executes `MATCH (a)-[r:LINK]->(b) SET r.w2 = 9` over `/cypher`
Then reading the relationship back returns `r.w2 = 9`

#### Scenario: SET map-merge on a relationship variable
Given a relationship `(a)-[r:LINK {w:5}]->(b)` exists
When the client executes `MATCH (a)-[r:LINK]->(b) SET r += {w:7, tag:"x"}`
Then the relationship has `r.w = 7` and `r.tag = "x"`
And `SET r.w = null` removes the `w` key

### Requirement: RETURN projects relationship properties in the same statement

The HTTP `/cypher` write path SHALL project relationship-variable property
access in the `RETURN` clause of the same `CREATE` or `MERGE` statement.

#### Scenario: CREATE ... RETURN reads a relationship property
Given an empty graph
When the client executes `CREATE (a)-[r:E {w:5}]->(b) RETURN r.w`
Then the response row contains `5` (not `null`)
