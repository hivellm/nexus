# `apoc.path.*` Procedure Spec

## ADDED Requirements

### Requirement: `apoc.path.expand(start, relFilter, labelFilter, minDepth, maxDepth)`

The procedure SHALL perform a configurable BFS/DFS expansion from
`start` returning a stream of matching PATH values.

#### Scenario: Expand two hops along typed relationship
Given a graph `(a:User)-[:FOLLOWS]->(b:User)-[:FOLLOWS]->(c:User)`
When `CALL apoc.path.expand(a, "FOLLOWS>", "User", 1, 2)` is executed
Then exactly two paths SHALL be returned
And the paths SHALL include `a→b` (depth 1) and `a→b→c` (depth 2)

### Requirement: Relationship Filter Grammar

`relFilter` SHALL accept direction suffix:
- `TYPE>` — outgoing
- `<TYPE` — incoming
- `TYPE`  — any direction
- Multiple types joined with `|`

#### Scenario: Multi-type filter
Given relationships of types `A`, `B`, `C`
When `apoc.path.expand(start, "A|B>", "", 1, 3)` is executed
Then the expansion SHALL traverse outgoing `A` or `B` edges only

### Requirement: Label Filter Grammar

`labelFilter` SHALL accept tokens prefixed with:
- `+Label` — include paths whose terminal node matches
- `-Label` — exclude paths whose terminal node matches
- `/Label` — terminate expansion at matching nodes (inclusive)
- `>Label` — only emit paths whose terminal matches

#### Scenario: Terminator label
Given a graph A→B→C→D with B labelled `End`
When `apoc.path.expand(A, "REL>", "/End", 1, 10)` is executed
Then paths A→B SHALL be emitted
And no path passing through B and continuing further SHALL be emitted

### Requirement: Uniqueness Policy

`apoc.path.expandConfig(start, config)` SHALL accept `uniqueness` as
one of `NODE_GLOBAL`, `NODE_PATH`, `RELATIONSHIP_GLOBAL`,
`RELATIONSHIP_PATH`, matching Neo4j's path-finding policies.

#### Scenario: NODE_PATH allows revisits across paths but not within
Given a graph A→B→A (cycle)
When `apoc.path.expandConfig(A, {uniqueness: "NODE_PATH", maxLevel: 3})`
  is executed
Then no path SHALL visit A more than once
And B→A SHALL not be emitted as a 2-hop path from A

### Requirement: `apoc.path.subgraphAll(start, config)`

The procedure SHALL return a MAP `{nodes: LIST<NODE>,
relationships: LIST<RELATIONSHIP>}` containing every node reachable
from `start` within the configured bounds and every relationship
between those nodes.

#### Scenario: Subgraph is closed
Given a graph A→B→C with A→C as a direct edge
When `CALL apoc.path.subgraphAll(A, {maxLevel: 2})` is executed
Then the result SHALL contain nodes `{A, B, C}`
And the result SHALL contain all three relationships

### Requirement: `apoc.path.spanningTree(start, config)`

The procedure SHALL return a tree (no cycles) rooted at `start`
covering every reachable node within the configured bounds.

#### Scenario: Spanning tree drops cycle edges
Given a triangle A→B→C→A
When `CALL apoc.path.spanningTree(A, {maxLevel: 10})` is executed
Then exactly two of the three edges SHALL be included
