# NOT NULL Constraint Spec

## ADDED Requirements

### Requirement: `REQUIRE n.p IS NOT NULL`

The parser SHALL accept
`CREATE CONSTRAINT [name] FOR (n:L) REQUIRE n.p IS NOT NULL`.

#### Scenario: Create constraint on empty database
Given an empty database
When `CREATE CONSTRAINT person_email FOR (p:Person) REQUIRE p.email IS NOT NULL`
  is executed
Then the constraint SHALL be created
And `db.constraints()` SHALL include a row with
  `type = "NODE_PROPERTY_EXISTENCE"`

#### Scenario: Create constraint on dirty data fails atomically
Given a database containing `(:Person)` nodes that lack `email`
When `CREATE CONSTRAINT ... REQUIRE p.email IS NOT NULL` is executed
Then the procedure SHALL fail with `ERR_CONSTRAINT_VIOLATED`
And the error payload SHALL include up to 100 offending node IDs
And no constraint SHALL be registered

### Requirement: Enforcement on CREATE

Once the constraint is active, any `CREATE (p:Person)` missing `email`
or setting `email = null` SHALL fail with `ERR_CONSTRAINT_VIOLATED`.

#### Scenario: CREATE missing property rejected
Given an active `NOT NULL` constraint on `(:Person).email`
When `CREATE (p:Person {name: "Alice"})` is executed
Then the transaction SHALL fail with HTTP 400
And the response SHALL reference the constraint name

#### Scenario: CREATE with NULL property rejected
Given the same constraint
When `CREATE (p:Person {name: "Alice", email: null})` is executed
Then the transaction SHALL fail with HTTP 400

### Requirement: Enforcement on SET / REMOVE

`SET n.p = NULL` and `REMOVE n.p` SHALL fail on a NOT NULL property.

#### Scenario: SET to NULL rejected
Given a node `(a:Person {email: "alice@ex.com"})` and an active NOT NULL constraint
When `MATCH (p:Person) SET p.email = null` is executed
Then the transaction SHALL fail with HTTP 400

#### Scenario: REMOVE rejected
Given the same node
When `MATCH (p:Person) REMOVE p.email` is executed
Then the transaction SHALL fail with HTTP 400

### Requirement: Enforcement on Adding Label

Adding a label whose constraint is violated by the existing node
SHALL fail.

#### Scenario: SET label without required property
Given a node `(n:Thing {name: "foo"})` with no `email` property
And an active `NOT NULL` constraint on `(:Person).email`
When `MATCH (n:Thing) SET n:Person` is executed
Then the transaction SHALL fail with `ERR_CONSTRAINT_VIOLATED`
