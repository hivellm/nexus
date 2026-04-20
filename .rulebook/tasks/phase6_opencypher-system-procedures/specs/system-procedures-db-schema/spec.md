# `db.schema.*` Procedure Spec

## ADDED Requirements

### Requirement: `db.schema.visualization()` Returns Schema Graph

The system SHALL expose `db.schema.visualization()` returning two
columns: `nodes:LIST<NODE>` and `relationships:LIST<RELATIONSHIP>`.
The nodes represent labels and the relationships represent observed
`(label)-[:TYPE]->(label)` patterns in the current database.

#### Scenario: Schema of populated graph
Given a database with nodes `(a:Person), (b:Movie)` and relationship
  `(a)-[:ACTED_IN]->(b)`
When `CALL db.schema.visualization()` is executed
Then the result SHALL contain two schema nodes (`Person`, `Movie`)
And the result SHALL contain one schema relationship labelled `ACTED_IN`
  pointing from `Person` to `Movie`

#### Scenario: Empty database
Given a fresh database with no data
When `CALL db.schema.visualization()` is executed
Then the result SHALL be one row with empty lists for both columns

### Requirement: `db.schema.nodeTypeProperties()` Describes Node Properties

The procedure SHALL enumerate every combination of node label + property
key observed in the sample, with columns:
`nodeType:STRING, nodeLabels:LIST<STRING>, propertyName:STRING,
propertyTypes:LIST<STRING>, mandatory:BOOLEAN`.

#### Scenario: Two labels with overlapping keys
Given nodes `(:Person {name: "Alice", age: 30})` and `(:Person {name: "Bob"})`
When `CALL db.schema.nodeTypeProperties()` is executed
Then two rows SHALL be returned for label `Person`
And the `name` row SHALL have `mandatory = true`
And the `age` row SHALL have `mandatory = false`

### Requirement: `db.schema.relTypeProperties()` Describes Relationship Properties

The procedure SHALL enumerate every relationship-type + property-key
combination observed in the sample, with columns:
`relType:STRING, propertyName:STRING, propertyTypes:LIST<STRING>,
mandatory:BOOLEAN`.

#### Scenario: Relationship with optional property
Given relationships `(:A)-[:R {w: 1.0}]->(:B)` and `(:A)-[:R]->(:B)`
When `CALL db.schema.relTypeProperties()` is executed
Then one row SHALL have `relType = ":`R`"`, `propertyName = "w"`,
  `mandatory = false`

### Requirement: Sampling Size is Configurable

The procedures `db.schema.nodeTypeProperties` and
`db.schema.relTypeProperties` SHALL accept an optional config map
`{sample: INTEGER}` where `0` means exhaustive and any positive
integer caps the sample size per label/relationship-type.

#### Scenario: Exhaustive sampling
Given 10,000 `Person` nodes with varying property sets
When `CALL db.schema.nodeTypeProperties({sample: 0})` is executed
Then every unique `(label, propertyName)` pair SHALL appear in the result
