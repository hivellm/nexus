# Cluster Management API Spec

## ADDED Requirements

### Requirement: GET /cluster/status

The system SHALL expose `GET /cluster/status` returning a JSON snapshot of
cluster metadata + per-shard health.

Response body:
```json
{
  "cluster_id": "uuid",
  "generation": 42,
  "num_shards": 3,
  "shards": [
    {
      "shard_id": 0,
      "state": "Active",
      "leader": "node-a",
      "replicas": [
        {"node_id": "node-a", "healthy": true, "commit_offset": 12345, "lag": 0},
        {"node_id": "node-b", "healthy": true, "commit_offset": 12344, "lag": 1}
      ]
    }
  ],
  "nodes": { "node-a": {"addr": "10.0.0.1:15480"}, ... }
}
```

#### Scenario: Healthy 3-node cluster
Given a 3-node cluster with 3 shards, replica factor 3, all healthy
When `GET /cluster/status` is called on any node
Then the response SHALL be HTTP 200
And `shards[i].replicas.len()` SHALL equal 3 for every shard
And every replica SHALL have `healthy: true`

### Requirement: POST /cluster/add_node

The system SHALL expose `POST /cluster/add_node` that accepts
`{ node_id: String, addr: SocketAddr }` and commits the membership change
through the metadata Raft group.

#### Scenario: Add a 4th node to a 3-node cluster
Given a 3-node cluster at generation 10
When `POST /cluster/add_node {"node_id": "node-d", "addr": "10.0.0.4:15480"}`
  is sent to any node
Then the request SHALL be forwarded to the metadata leader if necessary
And generation SHALL advance to 11
And `node-d` SHALL appear in `GET /cluster/status` within 3 seconds

### Requirement: POST /cluster/remove_node

The system SHALL expose `POST /cluster/remove_node` with body
`{ node_id: String, drain: bool }`. When `drain = true`, the API SHALL wait
for all shards to reach `commit_offset` parity on other replicas before
returning 200.

#### Scenario: Graceful removal
Given a 4-node cluster where node-d hosts replicas of shards 0 and 2
When `POST /cluster/remove_node {"node_id": "node-d", "drain": true}` is called
Then shards 0 and 2 SHALL each receive a Raft membership change to exclude
  node-d
And the API SHALL wait until the remaining replicas of shards 0 and 2 report
  `lag = 0` before returning HTTP 200

### Requirement: POST /cluster/rebalance

The system SHALL expose `POST /cluster/rebalance` that triggers the
rebalancer: if any node hosts > `ceil(num_shards * replica_factor / num_nodes)`
replicas, shards SHALL be moved away from it.

#### Scenario: Move a replica
Given a 3-node cluster where node-a hosts 5 replicas and node-b hosts 3
When `POST /cluster/rebalance` is called
Then the rebalancer SHALL propose membership changes to move one replica
  from node-a to node-b
And generation SHALL bump exactly once per replica moved

### Requirement: Leader Redirect

All mutating `/cluster/*` endpoints SHALL respond with `307 Temporary
Redirect` to the metadata leader when called on a follower.

#### Scenario: Write on follower
Given a 3-node cluster with metadata leader L
When `POST /cluster/add_node` is sent to a follower F
Then F SHALL respond `307 Temporary Redirect` with `Location` pointing
  at L's `/cluster/add_node`

### Requirement: Admin Authorization

All `/cluster/*` endpoints SHALL require a caller with the `Admin`
permission (same model as RBAC on `/auth/users`).

#### Scenario: Unauthorized caller rejected
Given a user without the Admin permission
When they call `POST /cluster/add_node`
Then the response SHALL be HTTP 403 Forbidden
