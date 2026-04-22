# R-tree Index Spec

## ADDED Requirements

### Requirement: `CREATE INDEX ... USING RTREE`

The parser SHALL accept
`CREATE INDEX [name] FOR (n:Label) ON (n.prop) USING RTREE
 [OPTIONS {dimensions: INT, crs: STRING}]`
and register a new R-tree index covering all nodes of the given label
that carry the given property.

#### Scenario: Create index succeeds
Given an empty database
When `CREATE INDEX place_loc FOR (p:Place) ON (p.loc) USING RTREE` is executed
Then the index SHALL be created
And `db.indexes()` SHALL list it with `type = "RTREE"`

#### Scenario: Create rejects non-point values
Given a label `Place` where some nodes have `.loc` as STRING
When `CREATE INDEX ... USING RTREE` is executed
Then the server SHALL respond with HTTP 400
And the error code SHALL be `ERR_RTREE_BUILD`

### Requirement: Bulk Load Is Deterministic

Bulk-loading the same set of points SHALL produce the same R-tree
structure (same page layout, same root page id) across runs, so that
cluster replicas converge bit-for-bit.

#### Scenario: Idempotent bulk load
Given 10,000 points inserted in identical order on two replicas
When both replicas build their R-tree indexes
Then the resulting page files SHALL be byte-identical

### Requirement: Incremental Insert and Delete

After the initial bulk load the index SHALL accept incremental
insert and delete operations with O(log_b n) expected cost.

#### Scenario: Insert then query finds the new node
Given an R-tree index with 1,000 points
When a node with `loc = point({x: 10, y: 10})` is inserted
Then `MATCH (p:Place) WHERE point.withinDistance(p.loc, point({x:10, y:10}), 1) RETURN p`
  SHALL return the new node

#### Scenario: Delete removes from index
Given a node indexed by the R-tree
When the node is deleted
Then subsequent `point.withinBBox` queries SHALL NOT include it

### Requirement: MVCC Snapshot Reads

Readers at snapshot epoch `E` SHALL only see R-tree entries whose
owning node was committed at or before `E` and not yet tombstoned at `E`.

#### Scenario: Concurrent insert not visible to earlier snapshot
Given a reader at epoch `E0`
When a writer inserts a node and commits at epoch `E1 > E0`
Then the reader SHALL NOT see the new node in R-tree seek results

### Requirement: Crash Recovery

After an unclean shutdown the R-tree SHALL be fully rebuilt from WAL
replay, producing a structure equivalent to one that would have
resulted from applying the operations in log order.

#### Scenario: Crash during bulk load
Given a bulk-load that has journalled 5,000 inserts but not yet
  `OP_RTREE_BULKLOAD_DONE`
When the server restarts
Then the R-tree SHALL be rebuilt from scratch (bulk-load restarted)
And all 5,000 inserts SHALL be present in the final structure

### Requirement: Page Size and Fanout

Each R-tree page SHALL be 8 KB. Fanout SHALL be at least 64 children
per page and at most 127.

#### Scenario: Page utilisation after bulk-load
Given 1,000,000 points bulk-loaded
When the R-tree structure is inspected
Then no page other than the rightmost sibling of each level SHALL
  contain fewer than 64 children
