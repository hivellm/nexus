# Cross-Shard Traversal Spec

## ADDED Requirements

### Requirement: Remote Node Fetch

The system SHALL provide `coordinator.fetch_remote_node(shard_id, node_id) ->
RemoteNodeView` to fetch a node's labels, properties, and first-relationship
pointer from a remote shard.

#### Scenario: Cross-shard expand
Given node A on shard 0 with an edge `(A)-[:KNOWS]->(B)` where B is on shard 1
When `MATCH (A)-[r]->(b) WHERE A.id = $x RETURN b.name` executes
Then the coordinator SHALL fetch B's properties from shard 1
And SHALL project `b.name` correctly

### Requirement: Remote-Stub Records

When a relationship `(src)-[r]->(dst)` crosses shards, the destination
shard SHALL store a **remote stub** with `flags |= REMOTE_STUB`,
`label_bits = 0`, and a `first_rel_ptr` pointing at a local edge list
reachable when expanding from `dst`.

#### Scenario: Reverse-direction expand finds the edge
Given an edge `(A on shard 0)-[:KNOWS]->(B on shard 1)`
When executing `MATCH (b)<-[r]-(a) WHERE b.id = B.id RETURN a`
Then shard 1 SHALL locate the edge via B's remote-stub `first_rel_ptr`
And SHALL report `a` as a remote handle `(shard=0, node=A.id)` back to the
  coordinator

### Requirement: Cross-Shard Cache

The system SHALL cache remote-node views in an LRU with at least 10,000
entries (configurable). Cache keys MUST be `(shard_id, node_id, generation)`.

#### Scenario: Cache hit on repeat fetch
Given a warm cache with `(1, 42, 5)` present
When a second query fetches `(1, 42)` at generation 5
Then the coordinator SHALL NOT issue a network RPC
And SHALL serve the response from cache

#### Scenario: Generation change invalidates cache
Given a cached entry at generation 5
When a cluster rebalance bumps the generation to 6
Then the next fetch SHALL bypass the cache and refetch from the shard

### Requirement: Cache TTL Safety Net

Cached entries SHALL expire after at most 30 seconds regardless of
generation changes, so stale cache cannot persist under missed
invalidation notifications.

#### Scenario: TTL expiry
Given a cached entry inserted at t=0
When the cache is queried at t=31s
Then the cache SHALL miss
And the coordinator SHALL refetch from the owning shard

### Requirement: Bounded Network Hops

A single Cypher query SHALL issue at most
`max_cross_shard_rpcs_per_query` (default 1,000) remote-node fetch RPCs.
Queries exceeding the bound MUST fail with `ERR_TOO_MANY_REMOTE_FETCHES`.

#### Scenario: Runaway traversal rejected
Given `MATCH (a)-[*1..10]->(b) RETURN b` that would fetch 5,000 remote nodes
When the coordinator processes the plan
Then the coordinator SHALL abort at 1,000 remote fetches
And SHALL return `ERR_TOO_MANY_REMOTE_FETCHES` to the client
