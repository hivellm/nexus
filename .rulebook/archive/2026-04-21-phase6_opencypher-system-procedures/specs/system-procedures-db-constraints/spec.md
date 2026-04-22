# `db.constraints` Procedure Spec

## ADDED Requirements

### Requirement: `db.constraints()` Lists All Constraints

The system SHALL expose `db.constraints()` returning one row per
constraint, with columns: `id:INTEGER, name:STRING, type:STRING,
entityType:STRING, labelsOrTypes:LIST<STRING>, properties:LIST<STRING>,
ownedIndex:STRING`.

#### Scenario: Uniqueness constraint listed
Given a constraint `CREATE CONSTRAINT email_unique FOR (p:Person) REQUIRE p.email IS UNIQUE`
When `CALL db.constraints()` is executed
Then the returned row SHALL have `type = "UNIQUENESS"`
And `entityType` SHALL equal `"NODE"`
And `labelsOrTypes` SHALL equal `["Person"]`
And `properties` SHALL equal `["email"]`
And `ownedIndex` SHALL be the name of the backing uniqueness index

### Requirement: Constraint Types Match Neo4j Canonical Names

The `type` column SHALL use the canonical Neo4j 5.x identifiers:
`"UNIQUENESS"`, `"NODE_KEY"`, `"NODE_PROPERTY_EXISTENCE"`,
`"RELATIONSHIP_PROPERTY_EXISTENCE"`.

#### Scenario: Node-key constraint
Given a NODE KEY constraint on `(:Person {id, tenantId})`
When `CALL db.constraints()` is executed
Then the row SHALL have `type = "NODE_KEY"`
And `properties` SHALL equal `["id", "tenantId"]`

### Requirement: No Rows When No Constraints Exist

When the current database has no constraints defined, `db.constraints()`
SHALL return zero rows (not NULL, not an error).

#### Scenario: Empty database
Given a fresh database with no constraints
When `CALL db.constraints()` is executed
Then the result set SHALL be empty
