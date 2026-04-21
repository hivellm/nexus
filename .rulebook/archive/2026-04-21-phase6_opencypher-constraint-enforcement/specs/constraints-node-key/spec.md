# NODE KEY Constraint Spec

## ADDED Requirements

### Requirement: `REQUIRE (n.p1, n.p2) IS NODE KEY`

The parser SHALL accept
`CREATE CONSTRAINT [name] FOR (n:L) REQUIRE (n.p1, n.p2) IS NODE KEY`.
The constraint SHALL enforce:
1. Each property in the tuple is NOT NULL.
2. The tuple of property values is globally unique across all nodes
   carrying label `L`.

#### Scenario: Composite uniqueness
Given an active NODE KEY `(id, tenantId)` on `(:Person)`
And a node `(:Person {id: 1, tenantId: "a"})`
When `CREATE (:Person {id: 1, tenantId: "a"})` is executed
Then the transaction SHALL fail with HTTP 409
And the error code SHALL be `ERR_CONSTRAINT_VIOLATED`

#### Scenario: Same first component, different tuple
Given an active NODE KEY `(id, tenantId)` on `(:Person)`
And a node `(:Person {id: 1, tenantId: "a"})`
When `CREATE (:Person {id: 1, tenantId: "b"})` is executed
Then the transaction SHALL succeed

### Requirement: Each Component Implicitly NOT NULL

Creating a node of the constrained label with NULL or absent value in
any tuple component SHALL fail with `ERR_CONSTRAINT_VIOLATED`.

#### Scenario: Missing tuple component
Given an active NODE KEY `(id, tenantId)` on `(:Person)`
When `CREATE (:Person {id: 1})` is executed
Then the transaction SHALL fail with HTTP 400

### Requirement: Backfill Rejects Existing Violations

Creating a NODE KEY constraint on a non-empty database SHALL scan the
label's nodes. If any tuple is non-unique or any component is NULL,
the CREATE SHALL fail with up to 100 offending examples.

#### Scenario: Duplicate tuple in existing data
Given two `(:Person {id: 1, tenantId: "a"})` nodes already exist
When `CREATE CONSTRAINT ... REQUIRE (p.id, p.tenantId) IS NODE KEY`
  is executed
Then the procedure SHALL fail
And the error SHALL list the two offending node IDs

### Requirement: Composite Index is Created

Successful creation of a NODE KEY constraint SHALL create (or reuse)
a composite-key B-tree index over `(p1, p2, ...)`. The index SHALL be
reported through `db.indexes()`.

#### Scenario: Index visible
Given a newly created NODE KEY `(id, tenantId)` on `(:Person)`
When `CALL db.indexes()` is executed
Then a row SHALL exist with `type = "BTREE"`, `uniqueness = "UNIQUE"`,
  `properties = ["id", "tenantId"]`
