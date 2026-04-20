# Sharding Spec

## ADDED Requirements

### Requirement: Hash-Based Shard Assignment

The system SHALL assign every node to exactly one shard using a deterministic
hash function of its `node_id: u64`.

#### Scenario: Deterministic assignment
Given a cluster configured with `num_shards = 4`
When a node with `node_id = 12345` is inserted
Then the shard assignment SHALL be `xxh3(12345) mod 4`
And the same `node_id` SHALL always map to the same shard across restarts

#### Scenario: Balanced distribution under uniform IDs
Given a cluster with `num_shards = 8`
When 10,000 nodes are inserted with sequential IDs
Then each shard SHALL contain between 1,150 and 1,350 nodes (±15% of the mean)

### Requirement: Relationship Co-Location

The system SHALL store every relationship record on the shard that owns the
relationship's source node.

#### Scenario: Local-source edges
Given node `A` on shard 0 and node `B` on shard 1
When `CREATE (A)-[:KNOWS]->(B)` is committed
Then the relationship record SHALL live on shard 0
And shard 1 SHALL hold a remote-stub entry so reverse-direction expands succeed

### Requirement: Shard Metadata Storage

The system SHALL persist cluster metadata (shard count, replica assignments,
generation number) in a dedicated Raft group identified as the **metadata
group**.

#### Scenario: Metadata consistency after leader change
Given a 3-node cluster with the metadata leader on node A
When node A crashes and a new metadata leader is elected
Then the new leader's metadata state SHALL be byte-identical to the last
  committed metadata log entry

### Requirement: Generation-Based Staleness Detection

Every scatter RPC from the coordinator SHALL include the current `generation`
number. Shards SHALL reject RPCs with stale generations with
`ERR_STALE_GEN(current_gen)`.

#### Scenario: Coordinator with stale metadata
Given cluster generation 5 and a coordinator that cached generation 3
When the coordinator issues a scatter RPC to a shard at generation 5
Then the shard SHALL reject the RPC with `ERR_STALE_GEN(5)`
And the coordinator SHALL re-read metadata and retry

### Requirement: Cluster Bootstrap

The system SHALL bootstrap a new cluster from a static `cluster.toml` on first
start and MUST refuse to accept any write until the metadata group has elected
a leader.

#### Scenario: Fresh cluster
Given three nodes configured with matching `cluster.toml`
When all three start simultaneously
Then exactly one node SHALL become the metadata leader within 5 seconds
And the other two SHALL become metadata followers

### Requirement: Shard Health Monitoring

The system SHALL emit a `ShardHealth` snapshot per shard containing
`{leader, healthy_replicas, lag_per_replica, last_commit_offset}`.

#### Scenario: Replica falls behind
Given a shard with replicas R1 (leader), R2, R3
When R3 lags by more than 10,000 log entries
Then `GET /cluster/status` SHALL report `R3.healthy = false` with `lag = 10001+`
