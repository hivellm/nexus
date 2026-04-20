# Dynamic Labels on Writes Spec

## ADDED Requirements

### Requirement: `CREATE (n:$label)`

The parser SHALL accept parameter references in label positions of
`CREATE`. The writer SHALL resolve the parameter at runtime.

#### Scenario: Single dynamic label
Given the parameter `{"label": "Person"}`
When `CREATE (n:$label {name: "Alice"}) RETURN labels(n)` is executed
Then the returned labels SHALL equal `["Person"]`

#### Scenario: List of dynamic labels
Given the parameter `{"labels": ["Person", "User"]}`
When `CREATE (n:$labels {name: "Alice"}) RETURN labels(n)` is executed
Then the returned labels SHALL equal `["Person", "User"]`

### Requirement: `SET n:$label` and `REMOVE n:$label`

The parser SHALL accept `SET n:$label` and `REMOVE n:$label` with
the same parameter resolution semantics as CREATE.

#### Scenario: SET adds label
Given an existing node `(n:Person)` and parameter `{"label": "Admin"}`
When `MATCH (n:Person) SET n:$label RETURN labels(n)` is executed
Then the returned labels SHALL contain both `"Person"` and `"Admin"`

#### Scenario: REMOVE drops label
Given a node `(n:Person:Admin)` and parameter `{"label": "Admin"}`
When `MATCH (n) REMOVE n:$label RETURN labels(n)` is executed
Then the returned labels SHALL equal `["Person"]`

### Requirement: Invalid Label Values Rejected

The engine SHALL reject with `ERR_INVALID_LABEL` when the resolved
parameter is:
- NULL
- empty STRING
- empty LIST
- a LIST containing a non-STRING element
- a STRING containing characters outside the valid label set

#### Scenario: NULL parameter
Given the parameter `{"label": null}`
When `CREATE (n:$label)` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INVALID_LABEL`

#### Scenario: Non-string element
Given the parameter `{"labels": ["Person", 42]}`
When `CREATE (n:$labels)` is executed
Then the error code SHALL be `ERR_INVALID_LABEL`

### Requirement: Label Limit Enforced

Adding a label that would push the node over the 64-label bitmap
cap SHALL fail with `ERR_LABEL_LIMIT`.

#### Scenario: Limit reached
Given a node already carrying 64 distinct labels
When `SET n:$newlabel` is executed
Then the error code SHALL be `ERR_LABEL_LIMIT`
