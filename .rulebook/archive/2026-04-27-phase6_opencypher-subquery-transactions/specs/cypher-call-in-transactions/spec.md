# `CALL {} IN TRANSACTIONS` Spec

## ADDED Requirements

### Requirement: Grammar

The parser SHALL accept
`CALL '{' inner '}' [IN [CONCURRENT] TRANSACTIONS] [OF INT ROWS]
 [REPORT STATUS AS ident] [ON ERROR (CONTINUE|BREAK|FAIL|RETRY INT)]`.

#### Scenario: Minimal form
Given a query `UNWIND range(1,100) AS x CALL { WITH x CREATE (n {x:x}) } IN TRANSACTIONS`
When parsed
Then parsing SHALL succeed
And the AST SHALL contain a `CallInTransactions` clause with
  `batch_size = 1000` (default) and `on_error = FAIL`

#### Scenario: Full form
Given a query using every clause option
When parsed
Then every field in the AST SHALL be set correctly

### Requirement: Batch Commit Semantics

Every `N` input rows consumed by the operator SHALL result in one
committed transaction. If the final buffer holds fewer than `N`
rows, the operator SHALL flush it in a final commit.

#### Scenario: Exact multiple
Given `UNWIND range(1, 10000) AS x CALL { ... } IN TRANSACTIONS OF 1000 ROWS`
When the query completes
Then exactly 10 batches SHALL be committed

#### Scenario: Remainder flushed
Given `UNWIND range(1, 1050) AS x CALL { ... } IN TRANSACTIONS OF 1000 ROWS`
When the query completes
Then exactly 2 batches SHALL be committed: one of 1000, one of 50

### Requirement: `ON ERROR FAIL` Aborts Outer Query

The default `ON ERROR FAIL` SHALL cause the outer query to fail when
any batch encounters an error, rolling back only the failing batch.

#### Scenario: Failure mid-ingest
Given an ingest that commits 3 batches successfully, then fails in
  batch 4
When the operator is running with default `ON ERROR FAIL`
Then the outer query SHALL fail
And the first 3 committed batches' data SHALL be queryable
And batch 4's data SHALL NOT be queryable

### Requirement: `ON ERROR CONTINUE` Records and Proceeds

With `ON ERROR CONTINUE`, a failing batch SHALL be rolled back and
the operator SHALL keep consuming input rows. If `REPORT STATUS` is
set, the failure SHALL appear as a row with `committed = false`.

#### Scenario: Continue after failure
Given an ingest with one bad batch in the middle
When the clause is `ON ERROR CONTINUE REPORT STATUS AS s`
Then every good batch SHALL be committed
And the status stream SHALL include one row with `committed = false`
  and `err` non-NULL

### Requirement: `ON ERROR RETRY n` Retries Up To `n` Times

With `ON ERROR RETRY n`, a failing batch SHALL be retried up to `n`
times before escalating. If the retry succeeds, the operator SHALL
continue with the next batch.

#### Scenario: Retry resolves a transient error
Given a transient error that succeeds on the second attempt
When the clause is `ON ERROR RETRY 3`
Then the batch SHALL be committed on the retry
And the metric `nexus_call_in_tx_retry_attempts_total` SHALL increase by 1

### Requirement: `IN CONCURRENT TRANSACTIONS` Spawns Workers

With `IN CONCURRENT TRANSACTIONS OF N ROWS`, the operator SHALL
spawn up to `nexus.cypher.concurrency` worker transactions, each
consuming batches from a shared channel.

#### Scenario: Four workers share batches
Given `nexus.cypher.concurrency = 4` and an input of 100k rows with
  batch size 1000
When the operator runs with `IN CONCURRENT TRANSACTIONS`
Then up to 4 simultaneous writer transactions SHALL exist at any
  point in time
And the total committed-rows count SHALL equal 100000
