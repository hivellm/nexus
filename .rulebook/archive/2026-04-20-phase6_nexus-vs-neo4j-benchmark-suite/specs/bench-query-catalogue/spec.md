# Benchmark Query Catalogue Spec

## ADDED Requirements

### Requirement: Coverage of Every Cypher Function

The catalogue SHALL contain at least one scenario per documented
Cypher scalar function shipped by Nexus (as listed in
`docs/compatibility/NEO4J_COMPATIBILITY_REPORT.md`).

#### Scenario: Every function benchmarked
Given the current function registry has K functions
When the bench suite runs
Then the report SHALL contain at least K scenarios under the
  `scalar_fns` category

### Requirement: Coverage by Category

The catalogue SHALL contain scenarios in each of the 12 declared
categories: `scalar_fns`, `aggregations`, `point_reads`,
`label_scans`, `traversals`, `writes`, `indexes`, `constraints`,
`subqueries`, `procedures`, `temporal_spatial`, `hybrid`.

#### Scenario: No empty category
When the bench suite is enumerated
Then each category SHALL contain at least 5 scenarios

### Requirement: Parameters Are Versioned With Query

Each scenario SHALL declare its Cypher query and parameter map as
Rust constants. Changing either SHALL require changing the scenario
code and SHALL bump the scenario's internal version identifier.

#### Scenario: Version bump on query change
Given a scenario at version `v1`
When the query is edited
Then the scenario's `version` field SHALL be incremented to `v2`
And the CI step `check-version-bump` SHALL fail if the bump is missing

### Requirement: Scenario IDs Are Stable and Unique

Scenario IDs SHALL follow `<category>.<snake_case_name>` and SHALL
be unique across the entire catalogue. IDs MUST NOT be renamed
without a migration entry in `bench/migrations/`.

#### Scenario: Duplicate ID rejected at compile time
Given two scenarios declared with the same `id`
When the crate is compiled
Then compilation SHALL fail with a duplicate-registration error

### Requirement: Aggregation Decomposition Scenarios

The suite SHALL include scenarios where partial aggregation across
nodes would matter (sum, avg, percentileDisc), so that Neo4j's and
Nexus's optimisers are exercised on the same input.

#### Scenario: Percentile at scale
Given the `social` dataset
When `MATCH (n:Person) RETURN percentileDisc(n.age, 0.95)` is executed
Then both engines SHALL emit the same scalar result
And the ratio Nexus/Neo4j SHALL be recorded

### Requirement: RAG-Style Hybrid Scenarios

The `hybrid` category SHALL exercise queries that combine vector
search, full-text search, and graph traversal in a single Cypher
statement.

#### Scenario: Vector + graph combined
Given the `vector` dataset with an HNSW index
When a query performs `CALL db.index.fulltext.queryNodes(...)`
  followed by `MATCH (node)-[:SIMILAR_TO]->(b)`
Then both engines SHALL return the same top-k candidates
And the ratio SHALL be recorded
