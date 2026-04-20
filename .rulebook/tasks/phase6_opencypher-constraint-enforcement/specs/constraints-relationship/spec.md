# Relationship Constraint Spec

## ADDED Requirements

### Requirement: Relationship NOT NULL Grammar

The parser SHALL accept
`CREATE CONSTRAINT [name] FOR ()-[r:T]-() REQUIRE r.p IS NOT NULL`.

#### Scenario: Parse succeeds
Given an empty database
When
  `CREATE CONSTRAINT rel_weight_required FOR ()-[r:CONNECTS]-()
   REQUIRE r.weight IS NOT NULL`
  is executed
Then the constraint SHALL be created
And `db.constraints()` SHALL list it with
  `type = "RELATIONSHIP_PROPERTY_EXISTENCE"`,
  `entityType = "RELATIONSHIP"`,
  `labelsOrTypes = ["CONNECTS"]`

### Requirement: Enforcement on CREATE

With an active relationship NOT NULL constraint, creating a
relationship of the constrained type without the property or with
NULL SHALL fail with `ERR_CONSTRAINT_VIOLATED`.

#### Scenario: Missing property rejected
Given an active constraint `REQUIRE r.weight IS NOT NULL` on type `CONNECTS`
When `CREATE (a)-[:CONNECTS]->(b)` is executed
Then the transaction SHALL fail with HTTP 400

#### Scenario: NULL property rejected
Given the same constraint
When `CREATE (a)-[:CONNECTS {weight: null}]->(b)` is executed
Then the transaction SHALL fail with HTTP 400

### Requirement: Enforcement on SET / REMOVE

Setting the constrained property to NULL or removing it SHALL fail.

#### Scenario: SET to NULL
Given a relationship `[r:CONNECTS {weight: 1.0}]` with an active constraint
When `MATCH ()-[r:CONNECTS]->() SET r.weight = null` is executed
Then the transaction SHALL fail with HTTP 400

#### Scenario: REMOVE property
Given the same setup
When `MATCH ()-[r:CONNECTS]->() REMOVE r.weight` is executed
Then the transaction SHALL fail with HTTP 400

### Requirement: Backfill Validation

Creating a relationship NOT NULL constraint on a database containing
relationships of the target type that lack the property SHALL fail
the CREATE and return up to 100 offending relationship IDs.

#### Scenario: Backfill rejects dirty data
Given `[r1:CONNECTS {weight: 1.0}]` and `[r2:CONNECTS]` already exist
When `CREATE CONSTRAINT ... FOR ()-[r:CONNECTS]-() REQUIRE r.weight IS NOT NULL`
  is executed
Then the CREATE SHALL fail
And the error payload SHALL include the ID of `r2`
