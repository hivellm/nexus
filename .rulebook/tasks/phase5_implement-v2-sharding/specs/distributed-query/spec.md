# Distributed Query Coordinator Spec

## ADDED Requirements

### Requirement: Query Classification

The coordinator SHALL classify every parsed Cypher query as one of:
`SingleShard(shard_id)`, `Targeted(shards)`, or `Broadcast`.

#### Scenario: Single-shard by shard key
Given a cluster with `num_shards = 4`
When executing `MATCH (n:Person {id: $x}) RETURN n` with `$x = 42`
Then the coordinator SHALL classify as `SingleShard(xxh3(42) mod 4)`
And SHALL scatter the query to exactly one shard

#### Scenario: Broadcast without shard key
Given any cluster
When executing `MATCH (n:Person) RETURN count(n)` with no predicate on the
  sharding key
Then the coordinator SHALL classify as `Broadcast`
And SHALL scatter to every shard

### Requirement: Plan Pushdown

The coordinator SHALL push filters, projections, and `LIMIT` clauses into
shard-local subplans. The coordinator plan MUST only contain merge,
re-sort, re-limit, and final projection operators.

#### Scenario: Filter pushdown
Given `MATCH (n:Person) WHERE n.age > 30 RETURN n.name`
When the coordinator decomposes the plan
Then each shard-local subplan SHALL contain the `WHERE n.age > 30` filter
And the coordinator plan SHALL only concatenate per-shard rows

#### Scenario: Top-K with ORDER BY + LIMIT
Given `MATCH (n:Person) RETURN n ORDER BY n.age DESC LIMIT 10`
When the coordinator decomposes the plan
Then each shard-local subplan SHALL emit its local top-10 ordered by age desc
And the coordinator SHALL merge-sort and truncate to 10

### Requirement: Aggregation Decomposition

The coordinator SHALL decompose `COUNT`, `SUM`, `MIN`, `MAX`, `AVG`, and
`COLLECT` into partial aggregations at each shard + a final merge at the
coordinator.

#### Scenario: AVG decomposed correctly
Given `MATCH (n:Person) RETURN avg(n.age)` across 3 shards with
  partial sums (100, 200, 300) over (5, 10, 15) rows
When the coordinator merges results
Then the final result SHALL be `(100+200+300) / (5+10+15) = 20.0`

### Requirement: Scatter/Gather Timeout

The coordinator SHALL time out a scatter/gather cycle after
`query_timeout_ms` (default 30000). On timeout the query SHALL fail
atomically with `ERR_QUERY_TIMEOUT`; no partial rows SHALL be returned.

#### Scenario: Slow shard cancelled
Given a 3-shard cluster with shards 0 and 1 responding in 100ms and
  shard 2 hung
When `query_timeout_ms = 5000`
Then after 5 seconds the coordinator SHALL cancel the RPC to shard 2
And SHALL return `ERR_QUERY_TIMEOUT` to the client
And SHALL NOT emit rows from shards 0 or 1

### Requirement: Shard-Failure Atomicity

If any shard returns an error during scatter/gather, the coordinator SHALL
fail the entire query with `ERR_SHARD_FAILURE(shard_id, reason)` and MUST
NOT return partial results.

#### Scenario: One shard down
Given a 3-shard cluster
When shard 1's leader returns `ERR_NOT_LEADER` and has no follower to retry
  against
Then the coordinator SHALL return `ERR_SHARD_FAILURE(1, "no leader")`
And the client SHALL observe no rows from shards 0 or 2

### Requirement: Leader-Hint Retry

When a scatter RPC returns `ERR_NOT_LEADER(leader_hint)`, the coordinator
SHALL retry the RPC against `leader_hint` up to 3 times total before
surfacing a failure.

#### Scenario: Leader change mid-query
Given a query targeting shard 2 with cached leader L_old
When L_old responds `ERR_NOT_LEADER(L_new)`
Then the coordinator SHALL retry against L_new
And on success SHALL update its cached leader for shard 2
