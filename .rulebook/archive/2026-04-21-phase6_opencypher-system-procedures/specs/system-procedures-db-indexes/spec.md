# `db.indexes` / `db.indexDetails` Procedure Spec

## ADDED Requirements

### Requirement: `db.indexes()` Lists Every Index

The system SHALL expose `db.indexes()` returning one row per index
configured in the current database, with columns: `id:INTEGER,
name:STRING, state:STRING, populationPercent:FLOAT,
uniqueness:STRING, type:STRING, entityType:STRING,
labelsOrTypes:LIST<STRING>, properties:LIST<STRING>,
indexProvider:STRING`.

#### Scenario: Single B-tree index
Given a database with a B-tree index on `(:Person).name`
When `CALL db.indexes()` is executed
Then exactly one row SHALL be returned
And `type` SHALL equal `"BTREE"`
And `labelsOrTypes` SHALL equal `["Person"]`
And `properties` SHALL equal `["name"]`
And `uniqueness` SHALL equal `"NONUNIQUE"`

#### Scenario: KNN vector index
Given a database with a KNN index on `(:Item).embedding` of dim 384
When `CALL db.indexes()` is executed
Then the row for the KNN index SHALL have `type = "VECTOR"`
And `indexProvider` SHALL equal `"hnsw-1.0"`

### Requirement: Index State Reflects Population Progress

`state` SHALL be one of `"ONLINE"`, `"POPULATING"`, `"FAILED"`. When
`state = "POPULATING"`, `populationPercent` SHALL reflect the progress
in `[0.0, 100.0)`.

#### Scenario: Online index reports 100%
Given a fully populated index
When `CALL db.indexes()` is executed
Then `state` SHALL equal `"ONLINE"`
And `populationPercent` SHALL equal `100.0`

### Requirement: `db.indexDetails(name)` Returns Single Row

The procedure `db.indexDetails(name: STRING)` SHALL return the same row
schema as `db.indexes()` but filtered to the named index. If the index
does not exist, the call SHALL raise `ERR_INDEX_NOT_FOUND(name)`.

#### Scenario: Known index
Given an index named `"person_name_idx"`
When `CALL db.indexDetails("person_name_idx")` is executed
Then exactly one row SHALL be returned

#### Scenario: Unknown index
Given no index named `"missing"`
When `CALL db.indexDetails("missing")` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_INDEX_NOT_FOUND`

### Requirement: Multi-Database Scoping

`db.indexes()` SHALL only emit indexes for the caller's current session
database. Indexes in other databases MUST NOT be visible.

#### Scenario: Two databases isolated
Given database `A` with one index and database `B` with zero indexes
When `CALL db.indexes()` is executed in database `B`
Then the result SHALL be empty
