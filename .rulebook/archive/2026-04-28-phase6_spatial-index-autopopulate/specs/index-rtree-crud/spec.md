# Spatial Index Auto-populate Spec

## ADDED Requirements

### Requirement: CREATE auto-populates every matching index

Every `create_node` path in `crate::engine::crud` SHALL invoke
`Engine::spatial_autopopulate_node(node_id, label_ids,
properties)`. The hook SHALL walk every index registered in
`IndexManager::rtree` and, for each index whose `(label,
property)` pair matches the node, insert the node's Point into
the R-tree and emit a matching `WalEntry::RTreeInsert`.

#### Scenario: CREATE triggers the auto-populate hook
Given an R-tree index on `:Place(loc)`
When `CREATE (p:Place {loc: point({x: 1, y: 2})})` is run
Then `CALL spatial.nearest(point({x:0,y:0}), 'Place', 1)`
  SHALL return that node without any prior
  `spatial.addPoint` call

### Requirement: SET / REMOVE auto-refresh the index

`Engine::persist_node_state` SHALL invoke
`Engine::spatial_refresh_node(node_id, old_props, new_props)`.
The hook SHALL delete the stale entry from every index the
node belonged to, then re-insert when the new property value
is still a valid Point. When `REMOVE n.<prop>` clears the last
indexed property, the node SHALL stay evicted.

#### Scenario: SET moves the indexed position
Given a Place node at `point({x:1,y:1})` indexed by `:Place(loc)`
When `SET p.loc = point({x:10,y:10})` is executed
Then `CALL spatial.nearest(point({x:10,y:10}), 'Place', 1)`
  SHALL return the node with distance ~ 0

#### Scenario: REMOVE evicts without phantom re-add
Given a Place node at `point({x:1,y:1})` indexed by `:Place(loc)`
When `REMOVE p.loc` is executed
Then `spatial.nearest(point({x:1,y:1}), 'Place', 1)` SHALL
  NOT return the node

### Requirement: DELETE evicts from every index

`Engine::delete_node` SHALL invoke
`Engine::spatial_evict_node(node_id)`. The hook SHALL iterate
every index the node was a member of and emit
`WalEntry::RTreeDelete`.

#### Scenario: DELETE clears the index
Given a Place node indexed by `:Place(loc)`
When `MATCH (p:Place) WHERE id(p) = $id DELETE p` is executed
Then `spatial.nearest` SHALL NOT return the node

### Requirement: `CREATE SPATIAL INDEX` validates property type

When `CREATE SPATIAL INDEX ON :Label(prop)` runs, the
executor SHALL sample up to 1 000 existing `Label` nodes and
verify each carries `prop` as a Point. On the first non-Point
sample the server SHALL respond with HTTP 400 and error code
`ERR_RTREE_BUILD`, naming the offending `node_id`.

#### Scenario: Rejection on STRING-valued property
Given a Place node whose `loc` property is the STRING
  "not a point"
When `CREATE SPATIAL INDEX ON :Place(loc)` is executed
Then the response SHALL carry HTTP 400
And the error code SHALL be `ERR_RTREE_BUILD`
And the error message SHALL include the offending node id

### Requirement: Crash-recovery visibility

After an unclean shutdown every `RTreeInsert` /
`RTreeDelete` that was fsynced to the WAL SHALL surface in
`spatial.nearest` results after replay. Writes that never
reached the WAL SHALL remain absent.

#### Scenario: WAL-committed inserts survive kill-9
Given a running engine with 20 `CREATE (:Place {loc: ...})`
  statements committed to the WAL
When the engine is dropped without a clean shutdown and
  reopened
Then all 20 nodes SHALL surface in `spatial.nearest` results
