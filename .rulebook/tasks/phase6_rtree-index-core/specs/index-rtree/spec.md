# R-tree Index Spec (packed Hilbert variant)

## ADDED Requirements

### Requirement: Packed Hilbert R-tree replaces the grid backend

The system SHALL replace the grid-backed `RTreeIndex` at
`crates/nexus-core/src/geospatial/rtree.rs` with a packed
Hilbert R-tree under `crates/nexus-core/src/index/rtree/`. The
new backend SHALL power every existing `CREATE SPATIAL INDEX`,
`spatial.nearest`, `spatial.addPoint`, `point.withinBBox`, and
`point.withinDistance` call without Cypher-surface changes.

#### Scenario: Slice A queries keep working
Given a database created under slice A with populated
`Store.loc` spatial data
When the database is reopened against the new R-tree backend
Then every `CALL spatial.nearest(point({x:0,y:0}), 'Store', 3)`
  SHALL return the same 3 rows, ordered by distance ascending,
  ties broken by `node_id` ascending
And the query SHALL complete in O(log N + k) page reads

### Requirement: 8 KB page layout with fanout 64-127

Each R-tree page SHALL occupy 8 KB on disk. Fanout SHALL be at
least 64 children per page and at most 127 after bulk-load.
Pages SHALL be read through `crate::page_cache::PageCache` so
the Clock / 2Q / TinyLFU eviction policy the B-tree already
uses applies to the R-tree.

#### Scenario: Page utilisation after bulk-load
Given 1 000 000 points bulk-loaded
When the R-tree structure is inspected
Then no page other than the rightmost sibling of each level
  SHALL contain fewer than 64 children
And no page SHALL contain more than 127 children

### Requirement: Deterministic bulk-load

Bulk-loading the same `(node_id, point)` set SHALL produce a
byte-identical page file across two independent runs on the
same host architecture. Ties in Hilbert index SHALL break on
`node_id` ascending so the sort is stable.

#### Scenario: Byte-identical output across replicas
Given 10 000 points inserted in the same order on two replicas
When both replicas build their R-tree indexes via bulk-load
Then the resulting `.rtree` page files SHALL hash to the same
  SHA-256 digest

### Requirement: Incremental insert / delete after bulk-load

After the initial bulk-load the index SHALL accept incremental
insert and delete operations with O(log_b N) expected cost
(`b = 127`). Insert overflow SHALL split via the quadratic
split heuristic; leaf-level delete underflow SHALL re-insert
orphaned entries rather than merging.

#### Scenario: Insert after bulk-load finds the new node
Given an R-tree index with 1 000 points
When a node with `loc = point({x: 10, y: 10})` is inserted
Then `MATCH (p:Place) WHERE point.withinDistance(p.loc,
  point({x: 10, y: 10}), 1) RETURN p` SHALL return the new
  node

#### Scenario: Delete removes from subsequent queries
Given a node indexed by the R-tree
When the node is deleted through the engine's CRUD path
Then subsequent `point.withinBBox` queries SHALL NOT include
  it

### Requirement: NN priority queue SLO

`spatial.nearest(p, label, k)` SHALL return the k nodes with
indexed points closest to `p`, ordered ascending by distance
with ties broken by `node_id` ascending, using a min-heap
ordered on bbox-to-point distance so the walk terminates after
`k` leaves are popped.

#### Scenario: k=10 over 1 M points meets p95 < 2 ms
Given 1 000 000 randomly distributed points indexed for
  `:Place(loc)`
When `CALL spatial.nearest(point({x:500000,y:500000}), 'Place',
  10)` is benchmarked 10 000 times
Then the p95 wall-clock latency SHALL be below 2 ms on the
  reference hardware (Linux, SSD, single CPU)

### Requirement: WAL + crash recovery

R-tree mutations SHALL be journalled in the WAL with three new
op-codes: `RTreeInsert`, `RTreeDelete`, `RTreeBulkLoadDone`.
After an unclean shutdown the R-tree SHALL be fully rebuilt
from WAL replay, producing a structure equivalent to applying
the operations in log order.

#### Scenario: Crash mid bulk-load rebuilds cleanly
Given a bulk-load that has journalled 5 000 inserts but not
  yet emitted `RTreeBulkLoadDone`
When the server restarts
Then the R-tree SHALL be rebuilt from scratch (bulk-load
  restarted)
And all 5 000 inserts SHALL be present in the final structure

### Requirement: MVCC snapshot reads

Readers at snapshot epoch `E` SHALL only see R-tree entries
whose owning node was committed at or before `E` and not yet
tombstoned at `E`. The visibility filter SHALL be applied
after the page walk, before emitting rows, so an entry whose
node is invisible does NOT count against the `k` limit of
`spatial.nearest`.

#### Scenario: Concurrent insert hidden from earlier snapshot
Given a reader at epoch E0
When a writer inserts a node and commits at epoch E1 > E0
Then the reader SHALL NOT see the new node in R-tree seek
  results

### Requirement: Atomic rebuild under concurrent writes

During a full bulk rebuild the old R-tree SHALL stay queryable
until the new tree is WAL-synced, then a single atomic swap
SHALL promote the new root so no reader observes a half-built
tree.

#### Scenario: Readers do not block during rebuild
Given a running R-tree rebuild
When a reader issues `point.withinDistance` against the same
  index
Then the reader SHALL return results from the pre-rebuild
  tree without waiting for the rebuild to finish

### Requirement: `USING RTREE` grammar alias

The parser SHALL accept `CREATE INDEX [name] FOR (n:Label) ON
(n.prop) USING RTREE` as an alias for the existing `CREATE
SPATIAL INDEX ON :Label(prop)` shape. Both forms SHALL
register the same index on `IndexManager::rtree`.

#### Scenario: `USING RTREE` parses identically
Given an empty database
When `CREATE INDEX place_loc FOR (p:Place) ON (p.loc) USING
  RTREE` is executed
Then `db.indexes()` SHALL list a single index named
  `place_loc` with `type = "RTREE"`
