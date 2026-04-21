# Transaction Savepoints Spec

## ADDED Requirements

### Requirement: SAVEPOINT Statement

The parser SHALL accept `SAVEPOINT <name>` as a statement inside an
explicit transaction. The MVCC engine SHALL push a marker storing
the current undo-log and staged-ops offsets.

#### Scenario: Basic savepoint
Given an active transaction that has created one node
When `SAVEPOINT s1` is executed
Then the transaction SHALL accept further writes normally
And the savepoint stack SHALL have depth 1

#### Scenario: Outside transaction rejected
Given no active explicit transaction
When `SAVEPOINT s1` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_SAVEPOINT_NO_TX`

### Requirement: ROLLBACK TO SAVEPOINT

`ROLLBACK TO SAVEPOINT <name>` SHALL undo every mutation since the
named savepoint while keeping the savepoint (and the transaction)
active for further work.

#### Scenario: Rollback preserves pre-savepoint work
Given a transaction that created node A, then `SAVEPOINT s1`, then
  created node B
When `ROLLBACK TO SAVEPOINT s1` is executed
Then node A SHALL remain in the transaction's pending state
And node B SHALL NOT be visible on subsequent reads within the tx
And the savepoint `s1` SHALL still be active

### Requirement: Unknown Savepoint Errors

Rolling back to or releasing an unknown savepoint SHALL raise
`ERR_SAVEPOINT_UNKNOWN(name)`.

#### Scenario: Rollback to missing savepoint
Given an active transaction with no savepoints
When `ROLLBACK TO SAVEPOINT ghost` is executed
Then the error code SHALL be `ERR_SAVEPOINT_UNKNOWN`

### Requirement: RELEASE SAVEPOINT

`RELEASE SAVEPOINT <name>` SHALL pop the named savepoint (and any
inner savepoints) from the stack WITHOUT undoing any mutations.

#### Scenario: Release preserves work
Given a tx with savepoints `s1` then `s2`, with writes after each
When `RELEASE SAVEPOINT s1` is executed
Then the savepoint stack SHALL be empty
And all writes from the tx SHALL remain in the pending state

### Requirement: Nested Savepoints Unwind Correctly

Rolling back to an outer savepoint SHALL discard all inner
savepoints and their work atomically.

#### Scenario: Unwind through nested savepoints
Given a tx with `SAVEPOINT s1`, write 1, `SAVEPOINT s2`, write 2,
  `SAVEPOINT s3`, write 3
When `ROLLBACK TO SAVEPOINT s1` is executed
Then writes 1, 2, 3 SHALL all be undone
And savepoints `s2` and `s3` SHALL be removed from the stack
And `s1` SHALL remain active

### Requirement: Savepoints Invisible to WAL

A transaction containing savepoints that commits SHALL produce a
WAL entry indistinguishable from one produced by an equivalent
transaction without savepoints.

#### Scenario: WAL equivalence
Given a transaction that uses a savepoint but rolls back no writes
  before commit
When the transaction commits
Then its WAL entry SHALL contain only the final committed mutations
And SHALL NOT contain savepoint markers
