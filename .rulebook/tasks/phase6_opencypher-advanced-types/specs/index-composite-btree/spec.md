# Composite B-tree Index Spec

## ADDED Requirements

### Requirement: Create Composite Index

The parser SHALL accept
`CREATE INDEX [name] FOR (n:L) ON (n.p1, n.p2, ..., n.pK)`. The
system SHALL build a B-tree index keyed by the tuple
`(label_bits, p1_value, p2_value, ..., pK_value)`.

#### Scenario: Two-column index
Given an empty database
When `CREATE INDEX person_id FOR (p:Person) ON (p.tenantId, p.id)` is executed
Then an index SHALL be created
And `db.indexes()` SHALL show a row with `type = "BTREE"`,
  `properties = ["tenantId", "id"]`

### Requirement: Equality Seek on Prefix

When a query predicates all index columns or a proper prefix, the
planner SHALL use a `CompositeSeek` operator.

#### Scenario: Full-prefix seek
Given a composite index on `(tenantId, id)`
When `EXPLAIN MATCH (p:Person) WHERE p.tenantId = $t AND p.id = $i RETURN p`
  is produced
Then the plan SHALL contain `CompositeSeek` with both columns bound

#### Scenario: Partial-prefix seek
Given the same index
When `EXPLAIN MATCH (p:Person) WHERE p.tenantId = $t RETURN p` is produced
Then the plan SHALL contain `CompositeSeek` with one column bound
And the seek SHALL scan the prefix range for that tenant

### Requirement: Range Seek on Last Component

When the first K-1 components are equality-bound and the Kth is a
range predicate, the planner SHALL seek the appropriate sub-range.

#### Scenario: Tenant + id range
Given the same index
When the predicate is `p.tenantId = $t AND p.id > 100 AND p.id < 200`
Then the plan SHALL seek the range `[(t, 100), (t, 200)]` exclusive at both ends

### Requirement: No Seek Without Prefix

Predicates that skip an index column SHALL NOT use the composite
index.

#### Scenario: Middle column only
Given a composite index on `(tenantId, id, version)`
When the predicate is `p.id = 7`
Then the plan SHALL NOT use `CompositeSeek`
And SHALL fall back to label scan + filter

### Requirement: Uniqueness Flag

A composite index SHALL support a `UNIQUE` flag. When set, duplicate
tuples SHALL be rejected with `ERR_CONSTRAINT_VIOLATED` on write.

#### Scenario: Unique composite
Given a UNIQUE composite index on `(:Person).(tenantId, id)`
And an existing node `(:Person {tenantId: "a", id: 1})`
When `CREATE (:Person {tenantId: "a", id: 1})` is executed
Then the transaction SHALL fail with HTTP 409
