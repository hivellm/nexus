# Cypher spec: reserved `_id` property

## ADDED Requirements

### Requirement: Reserved `_id` on CREATE
The Cypher executor SHALL treat `_id` as a reserved property on `CREATE` patterns that sets the external id of the new node.

#### Scenario: Create with hash external id
Given a database with no node carrying `external_id = sha256:abc…`
When the query `CREATE (n:File {_id: 'sha256:abc…', name: 'a.txt'}) RETURN n._id` is executed
Then a node is created with the supplied external id
And the returned column `n._id` equals `'sha256:abc…'`

#### Scenario: Create with conflict policy MATCH
Given a node already exists with `external_id = uuid:1111-…`
When `CREATE (n:Doc {_id: 'uuid:1111-…', name: 'x'}) ON CONFLICT MATCH RETURN n._id` is executed
Then no new node is created
And the existing node is returned

### Requirement: Index seek on `_id` predicates
The query planner SHALL plan `MATCH (n {_id: $x})` and `MATCH (n) WHERE n._id = $x` as an external-id index seek, not a label scan.

#### Scenario: Plan uses external-id index
Given a database with 1,000,000 nodes, one of which has `external_id = sha256:abc…`
When `EXPLAIN MATCH (n {_id: 'sha256:abc…'}) RETURN n` is executed
Then the plan contains an `ExternalIdSeek` operator
And no `LabelScan` or `AllNodesScan` appears in the plan

### Requirement: MERGE fast-path on `_id`
The Cypher executor SHALL use the external-id index when `MERGE` is constrained only by `_id`.

#### Scenario: MERGE creates when absent
Given no node has `external_id = blake3:abc…`
When `MERGE (n:File {_id: 'blake3:abc…'}) ON CREATE SET n.imported_at = timestamp() RETURN n._id` is executed
Then a new node is created with the external id
And `n.imported_at` is set

#### Scenario: MERGE matches when present
Given a node with `external_id = blake3:abc…` already exists
When the same `MERGE` query is executed
Then no new node is created
And the existing node is returned without modification

### Requirement: Projection of `_id`
The system SHALL return the original prefixed string form of the external id when a query projects `n._id`, or `null` when the node has no external id.

#### Scenario: Project absent external id
Given a node was created without an external id
When `RETURN n._id` is projected
Then the value is `null`
