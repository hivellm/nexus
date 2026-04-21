# Constraint Engine Spec

## ADDED Requirements

### Requirement: Pre-Commit Constraint Check

Every mutating transaction SHALL pass through
`ConstraintEngine::check_pre_commit(tx)` before the transaction's
WAL entry is produced. If any constraint is violated, the
transaction SHALL be aborted atomically and no data SHALL be
written.

#### Scenario: Abort is atomic
Given a constraint `REQUIRE n.email IS UNIQUE`
When a transaction inserts two nodes with the same email
Then the transaction SHALL fail with `ERR_CONSTRAINT_VIOLATED`
And neither node SHALL be readable by subsequent queries

### Requirement: Structured Violation Payload

A violation SHALL return a payload containing: constraint name,
kind, entity type, labels/types, properties, and up to 100 violating
values with IDs.

#### Scenario: Error shape
Given a constraint violation
When the server surfaces the error
Then the response body SHALL contain the keys `error`, `constraint`,
  `violating_values`, `violating_node_id` or `violating_relationship_id`
And `constraint.kind` SHALL be one of `UNIQUENESS`, `NODE_PROPERTY_EXISTENCE`,
  `RELATIONSHIP_PROPERTY_EXISTENCE`, `NODE_KEY`, `PROPERTY_TYPE`

### Requirement: HTTP Status Mapping

- `UNIQUENESS` and `NODE_KEY` violations SHALL map to HTTP 409.
- `NODE_PROPERTY_EXISTENCE`, `RELATIONSHIP_PROPERTY_EXISTENCE`,
  `PROPERTY_TYPE` violations SHALL map to HTTP 400.

#### Scenario: Uniqueness is conflict
Given a unique constraint violation
When the REST endpoint surfaces the error
Then the status code SHALL be 409

#### Scenario: Not-null is bad request
Given a NOT NULL violation
When the REST endpoint surfaces the error
Then the status code SHALL be 400

### Requirement: Per-Database Scoping

Constraints SHALL belong to exactly one database. Constraints in
one database MUST NOT enforce writes in another.

#### Scenario: Isolation
Given database `A` with constraint `person_email_unique` on `(:Person).email`
And database `B` without that constraint
When a node with duplicate email is inserted in database `B`
Then no violation SHALL be raised

### Requirement: Relaxed Enforcement Flag

When `relaxed_constraint_enforcement = true`, the engine SHALL log
violations at `WARN` level but SHALL NOT propagate them. The server
SHALL emit a prominent startup warning while the flag is set.

#### Scenario: Relaxed mode allows invalid write
Given the server is started with `relaxed_constraint_enforcement = true`
And a constraint `REQUIRE n.email IS NOT NULL`
When a node is created without an email
Then the write SHALL succeed
And a warning SHALL be logged citing the constraint name
