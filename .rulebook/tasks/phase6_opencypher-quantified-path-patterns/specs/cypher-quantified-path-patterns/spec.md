# Quantified Path Patterns Spec

## ADDED Requirements

### Requirement: Parenthesised Quantified Fragment

The parser SHALL accept `'(' pathFragment ')' quantifier` as a valid
pattern element wherever a relationship pattern is accepted. The
quantifier forms MUST include `{m,n}`, `{m,}`, `{,n}`, `{n}`, `+`,
`*`, `?`.

#### Scenario: Bounded quantifier
Given the query `MATCH (a)( (x)-[:R]->(y) ){1,3}(b) RETURN a, b`
When the query is parsed
Then parsing SHALL succeed
And the resulting AST SHALL contain a `PatternPart::Quantified`
  with `lower = 1` and `upper = Some(3)`

#### Scenario: Shorthand quantifiers desugar correctly
Given the queries using `+`, `*`, and `?`
When parsed
Then `+` SHALL yield `{1, None}`
And `*` SHALL yield `{0, None}`
And `?` SHALL yield `{0, Some(1)}`

### Requirement: Inner Variables Promote to LIST

Every variable declared inside a quantified fragment SHALL be promoted
to a LIST type in the enclosing scope, ordered by iteration index.

#### Scenario: List promotion of inner nodes
Given a graph with three nodes A→B→C→D
When `MATCH (a) ( (x)-[:R]->(y) ){1,3} (end) RETURN y` is executed
  with `a = A`
Then one row SHALL have `y = [B, C, D]` when the quantifier matched 3 times
And rows for shorter matches SHALL have shorter lists

### Requirement: Quantifier Bounds Are Enforced

A match with iteration count `k < lower` or `k > upper` SHALL NOT be
emitted. For unbounded upper, the engine SHALL cap at
`max_qpp_depth` (default 64) and raise a warning when the cap is hit.

#### Scenario: Lower bound rejected
Given a graph with only A→B
When `MATCH (a) ( (x)-[:R]->(y) ){2,5} (end)` is executed with `a = A`
Then the result SHALL be empty

#### Scenario: Upper bound respected
Given a chain A→B→C→D→E→F
When `MATCH (a) ( (x)-[:R]->(y) ){1,2} (end)` is executed with `a = A`
Then only rows with `end ∈ {B, C}` SHALL be emitted

### Requirement: Inner Predicates Push Down

Predicates inside a quantified fragment SHALL apply at every iteration
and SHALL be pushed into the inner execution plan.

#### Scenario: Inner WHERE filter
Given nodes A→B{active:true}→C{active:false}→D{active:true}
When `MATCH (a) ( (x)-[:R]->(y) WHERE y.active ){1,3} (end)` is
  executed with `a = A`
Then the result SHALL be empty for quantifier value 3 (C fails filter)
And valid for quantifier value 1 with `end = B`

### Requirement: Nesting Limit

Quantified path patterns SHALL be nestable up to **one level deep**
in v1. Deeper nesting SHALL raise `ERR_QPP_NESTING_TOO_DEEP` at
parse time.

#### Scenario: Two-level nesting rejected
Given the query `MATCH ( ( ( (x)-[:R]->(y) ){1,2} )+ )` with 3 levels
When the query is parsed
Then parsing SHALL fail
And the error code SHALL be `ERR_QPP_NESTING_TOO_DEEP`

### Requirement: Invalid Quantifier Range Rejected

Quantifiers with `lower > upper` or negative bounds SHALL raise
`ERR_QPP_INVALID_QUANTIFIER` at parse time.

#### Scenario: Lower greater than upper
Given the query `MATCH (a)(...){5,2}(b)`
When parsed
Then the error code SHALL be `ERR_QPP_INVALID_QUANTIFIER`

### Requirement: Legacy Variable-Length Paths Continue to Work

Queries using the Cypher 9 form `-[:TYPE*m..n]->` SHALL continue to
parse and execute with identical semantics to prior releases. The
parser MAY rewrite them internally to QPP form, but user-visible
behaviour (variable bindings, types, row output) MUST NOT change.

#### Scenario: Legacy variable-length still returns relationship list
Given a chain A→B→C
When `MATCH p = (a)-[r:R*1..2]->(end) RETURN r` is executed with `a = A`
Then `r` SHALL be a LIST<RELATIONSHIP> as in prior Nexus releases

### Requirement: `shortestPath` Accepts Quantified Patterns

`shortestPath((a) ( inner ){m,n} (b))` and `allShortestPaths(...)`
SHALL accept QPP arguments. BFS SHALL use iteration count as the
cost metric.

#### Scenario: Shortest QPP match
Given a graph where A reaches D via either A→B→C→D (length 3) or
  A→E→D (length 2)
When `MATCH p = shortestPath((a) ( (x)-[:R]->(y) ){1,5} (b))` is
  executed with `a = A, b = D`
Then exactly one row SHALL be returned
And the matched path SHALL be the length-2 path A→E→D

### Requirement: Cycle Policy

Inside `MATCH` the engine SHALL permit nodes to repeat across
iterations (`NODES_CAN_REPEAT`). Inside `shortestPath` the engine
SHALL enforce `ALL_DIFFERENT` nodes.

#### Scenario: MATCH permits cycle
Given a 2-cycle A→B→A
When `MATCH (a) ( (x)-[:R]->(y) ){2,2} (end)` is executed with `a = A`
Then one row SHALL be emitted with `end = A` (after two hops)

#### Scenario: shortestPath rejects cycle
Given the same 2-cycle
When `MATCH p = shortestPath((a) ( (x)-[:R]->(y) ){1,5} (end))` is
  executed with `a = A, end = A`
Then no row SHALL be emitted (a self-path via a cycle is disallowed)
