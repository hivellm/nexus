# `db.labels` / `db.relationshipTypes` / `db.propertyKeys` Spec

## ADDED Requirements

### Requirement: `db.labels()` Returns All Labels

The system SHALL expose `db.labels()` returning one row per distinct
label present in the current database, with column `label:STRING`.

#### Scenario: Populated database
Given nodes with labels `Person`, `Movie`, and `Director`
When `CALL db.labels()` is executed
Then the result SHALL contain exactly three rows
And the set of label values SHALL be `{"Person", "Movie", "Director"}`

#### Scenario: Empty database
Given a fresh database
When `CALL db.labels()` is executed
Then the result SHALL be empty

### Requirement: `db.relationshipTypes()` Returns All Relationship Types

The procedure SHALL return one row per distinct relationship type with
column `relationshipType:STRING`.

#### Scenario: Multiple relationship types
Given relationships of types `ACTED_IN`, `DIRECTED`, `PRODUCED`
When `CALL db.relationshipTypes()` is executed
Then the result SHALL contain three rows with those exact values

### Requirement: `db.propertyKeys()` Returns All Property Keys

The procedure SHALL return one row per distinct property key used on
any node or relationship, with column `propertyKey:STRING`.

#### Scenario: Mixed properties across nodes and relationships
Given a node `(:Person {name: "Alice"})` and a relationship
  `-[:KNOWS {since: 2020}]->`
When `CALL db.propertyKeys()` is executed
Then the result SHALL contain two rows: `"name"` and `"since"`

### Requirement: Output Is Sorted Deterministically

All three procedures SHALL return rows sorted by the returned string
column in ascending lexicographic order, so clients can diff output
across runs.

#### Scenario: Deterministic order
Given labels inserted in order `Z`, `A`, `M`
When `CALL db.labels()` is executed
Then the result rows SHALL be `"A"`, `"M"`, `"Z"` in that order
