# `apoc.periodic.*` Procedure Spec

## ADDED Requirements

### Requirement: `apoc.periodic.iterate(driveQuery, actionQuery, config)`

The procedure SHALL run `driveQuery` to produce a stream of rows and
execute `actionQuery` once per row in a batched-transaction loop
equivalent to `CALL { } IN TRANSACTIONS OF config.batchSize ROWS`.

#### Scenario: Typical ingest
Given an empty database
When `CALL apoc.periodic.iterate(
  "UNWIND range(1, 1000) AS i RETURN i",
  "CREATE (n:N {id: i})",
  {batchSize: 100}
)` is executed
Then 1000 nodes of label `N` SHALL be created
And the result SHALL report `batches = 10`, `total = 1000`,
  `committedOperations = 1000`, `failedOperations = 0`

#### Scenario: Parallel mode
Given the same setup
When the same call is issued with `{batchSize: 100, parallel: true, concurrency: 4}`
Then writes SHALL complete with no data loss
And up to 4 worker transactions SHALL be simultaneously active

### Requirement: Failure Accounting

When `actionQuery` raises on a row, the failure SHALL be counted in
`failedOperations` and the error message recorded in `errorMessages`.

#### Scenario: One failing row
Given a `driveQuery` producing 100 rows
And an `actionQuery` that fails on one specific row
When `apoc.periodic.iterate` is called with `{batchSize: 10}` and
  `config = {retries: 0}`
Then `failedOperations` SHALL equal 1
And `committedOperations` SHALL equal 99
And `errorMessages` SHALL contain a single non-empty entry

### Requirement: Retries

When `config.retries` is positive, failed batches SHALL be retried
up to the configured count before being counted as failed.

#### Scenario: Retry succeeds
Given a transient action error that succeeds on the second attempt
When called with `{retries: 3}`
Then `committedOperations` SHALL include every row
And the retry count SHALL be reflected in the response

### Requirement: `apoc.periodic.commit(cypher, params)`

The procedure SHALL repeatedly execute `cypher` (which is expected
to return a `count` column representing rows processed) until the
returned `count` is zero. Each execution SHALL run in its own
transaction.

#### Scenario: Delete in chunks
Given a label with 50,000 nodes
When `apoc.periodic.commit(
  "MATCH (n:Big) WITH n LIMIT 1000 DETACH DELETE n RETURN count(n)",
  {}
)` is executed
Then every `Big` node SHALL be deleted
And the call SHALL execute at least 50 transactions

### Requirement: `apoc.periodic.list`

The procedure SHALL return rows of currently-running background
`apoc.periodic.submit` tasks with columns `name, delay, rate,
submitted, failures`.

#### Scenario: Empty list
Given no background tasks are running
When `CALL apoc.periodic.list()` is executed
Then the result SHALL be empty
