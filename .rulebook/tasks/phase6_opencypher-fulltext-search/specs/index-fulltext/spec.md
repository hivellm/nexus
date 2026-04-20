# Full-Text Index Spec

## ADDED Requirements

### Requirement: Full-Text Index Catalogue Entry

The system SHALL introduce a new index kind `FullText` with metadata
`{id, name, entity_type, labels_or_types, properties, analyzer,
refresh_ms}` persisted alongside existing index metadata in the LMDB
catalogue.

#### Scenario: Create full-text index registers metadata
Given an empty database
When `CALL db.index.fulltext.createNodeIndex("movies", ["Movie"], ["title", "overview"])`
  is executed
Then `db.indexes()` SHALL include a row with `name = "movies"`,
  `type = "FULLTEXT"`, `labelsOrTypes = ["Movie"]`,
  `properties = ["title", "overview"]`

#### Scenario: Duplicate name across kinds is rejected
Given a B-tree index named `"name_idx"` already exists
When `CALL db.index.fulltext.createNodeIndex("name_idx", ["X"], ["y"])` is
  executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_FTS_INDEX_EXISTS`

### Requirement: WAL-Backed Durability

Every add/delete that mutates a full-text index SHALL be journalled
to the WAL with entries `OP_FTS_ADD` / `OP_FTS_DEL`. On crash recovery
the system SHALL replay these entries into a fresh Tantivy directory.

#### Scenario: Crash mid-bulk-ingest recovers cleanly
Given an in-flight bulk ingest that has committed 10,000 nodes but
  has not yet refreshed the FTS writer
When the server is killed with SIGKILL and restarted
Then on restart the 10,000 nodes SHALL be searchable once recovery
  completes
And no WAL entry SHALL be lost or duplicated

### Requirement: Eventual Consistency Window

Reads SHALL reflect writes within `refresh_ms` of commit (default
1000 ms). The procedure
`db.index.fulltext.awaitEventuallyConsistentIndexRefresh()` SHALL
block until every FTS index has refreshed at least once since the
call.

#### Scenario: Synchronous refresh via await
Given a node is created and committed at time t
When the same transaction (or a later one) calls
  `db.index.fulltext.awaitEventuallyConsistentIndexRefresh()`
Then the node SHALL be searchable by the time the call returns

### Requirement: Single-Writer per Index

Each full-text index SHALL have exactly one writer task. Concurrent
writer creation for the same index SHALL fail with
`ERR_FTS_WRITER_UNAVAILABLE` rather than producing two Tantivy writers.

#### Scenario: Second writer rejected
Given a writer task is running for index `"movies"`
When another module attempts to construct a second writer for `"movies"`
Then the construction SHALL fail with `ERR_FTS_WRITER_UNAVAILABLE`

### Requirement: Drop Cleans Up Directory

`db.index.fulltext.drop(name)` SHALL:
1. Flush and stop the writer task.
2. Remove the Tantivy directory from disk.
3. Remove the catalogue entry.

After drop, `db.indexes()` MUST NOT list the index and the directory
MUST NOT exist.

#### Scenario: Drop removes disk state
Given an index `"movies"` exists
When `db.index.fulltext.drop("movies")` is executed
Then `<data_dir>/fulltext/movies/` SHALL no longer exist on disk
And `db.indexes()` SHALL NOT include `"movies"`
