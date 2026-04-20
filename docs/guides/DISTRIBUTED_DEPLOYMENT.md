# Distributed Deployment Guide (V2 Sharding)

> **Status**: core complete (2026-04-20). Production-readiness gates:
> auth, persistence layer, TCP transport wiring (deferred — in-process
> transport is production-usable for single-host testing; multi-host
> deployments need the TCP bridge).

Nexus V2 scales horizontally via **hash-based sharding + per-shard
Raft consensus + a distributed query coordinator**. This guide covers
the operator surface.

## Topology

```
           ┌───────────────────────────────┐
           │      Query Coordinator        │
           │ (routes, decomposes, gathers) │
           └──────┬────────┬────────┬──────┘
                  │        │        │
         ┌────────┘        │        └────────┐
         ▼                 ▼                 ▼
    ┌─────────┐       ┌─────────┐       ┌─────────┐
    │ Shard 0 │       │ Shard 1 │       │ Shard 2 │
    │  Raft   │       │  Raft   │       │  Raft   │
    │ L F F   │       │ L F F   │       │ L F F   │
    └─────────┘       └─────────┘       └─────────┘
```

* **N shards**, fixed at cluster bootstrap.
* Each shard is a **Raft group** of R replicas (default 3).
* Any node can accept queries; it forwards to the owning shard's leader.
* **Metadata group**: a dedicated Raft group storing the authoritative
  cluster layout (shard count, membership, generation). Identical in
  shape to the data groups; its members are typically every cluster
  node.

## Concepts

### Shard assignment

A node's home shard is `xxh3(node_id_le_bytes) mod num_shards`
([`nexus-core/src/sharding/assignment.rs`](../../nexus-core/src/sharding/assignment.rs)).
Relationships live on the **source** node's shard. Cross-shard edges
use a remote-anchor record on the destination shard so reverse
traversals work.

### Generation number

Every metadata change advances a monotonic `generation`. Coordinator
RPCs carry this value — shards reject stale ones with `ERR_STALE_GEN`.
This lets the coordinator detect a silent metadata drift and retry
with a fresh snapshot.

### Write path

1. Client hits any node's `/cypher`.
2. Coordinator classifies the query. Writes go to the owning shard.
3. Shard's Raft leader appends to its log, replicates to majority.
4. Leader applies to local storage. Followers apply from the log.

Single-writer invariant stays true per shard: **only the Raft apply
loop writes to the storage layer**. No path bypasses Raft.

## Configuration

`[cluster.sharding]` in the server TOML config:

```toml
[cluster.sharding]
mode = "bootstrap"                  # "disabled" | "bootstrap" | "join"
node_id = "node-a"
listen_addr = "0.0.0.0:15480"
peers = [
  { node_id = "node-a", addr = "10.0.0.1:15480" },
  { node_id = "node-b", addr = "10.0.0.2:15480" },
  { node_id = "node-c", addr = "10.0.0.3:15480" },
]
num_shards = 3
replica_factor = 3

election_timeout_min = "500ms"
election_timeout_max = "1000ms"
heartbeat = "100ms"
snapshot_log_size_threshold = 10_000

query_timeout = "30s"
max_cross_shard_rpcs_per_query = 1000
cross_shard_cache_size = 10_000
cross_shard_cache_ttl = "30s"
```

### Modes

* **`disabled`**: sharding off — classic single-node Nexus. The
  `/cluster/*` endpoints return `503 Service Unavailable`.
* **`bootstrap`**: bring up a new cluster from scratch. All listed
  peers must run with `mode = "bootstrap"` and identical `peers`,
  `num_shards`, `replica_factor`.
* **`join`**: add a new node to an existing cluster. The joining
  node reads the authoritative shard count from cluster metadata.

## Bootstrap walkthrough

On every node:

```bash
# 1. Place matching `nexus.toml` in /etc/nexus/
# 2. Start:
nexus-server
```

The metadata Raft group elects a leader within ~1s (election timeout
500–1000ms). From `GET /cluster/status`:

```json
{
  "cluster_id": "c1a5bf...",
  "generation": 1,
  "num_shards": 3,
  "shards": [
    { "shard_id": 0, "leader": "node-a", "replicas": [...] },
    { "shard_id": 1, "leader": "node-b", "replicas": [...] },
    { "shard_id": 2, "leader": "node-c", "replicas": [...] }
  ],
  "nodes": { "node-a": {...}, "node-b": {...}, "node-c": {...} },
  "metadata_leader": "node-a"
}
```

## HTTP API

All mutating endpoints require the **Admin** permission and forward
`307 Temporary Redirect` to the metadata leader when called on a
follower. See [cluster-api spec](../../.rulebook/tasks/phase5_implement-v2-sharding/specs/cluster-api/spec.md).

### `GET /cluster/status`

Returns the JSON shape above.

### `POST /cluster/add_node`

```json
{ "node_id": "node-d", "addr": "10.0.0.4:15480", "zone": "" }
```

Commits membership change through the metadata Raft group. The new
node appears in `/cluster/status` within ~3s.

### `POST /cluster/remove_node`

```json
{ "node_id": "node-d", "drain": true }
```

With `drain = true`, the API refuses to remove a node still hosting
shard replicas, so the operator must first `rebalance` or manually
replace memberships. `drain = false` surfaces the raw conflict.

### `POST /cluster/rebalance`

Triggers the rebalancer. It's iterative and deterministic — repeat
calls until `moves_applied: 0`. Each move is a single Raft log entry
in the metadata group; the shard being moved gets its membership
updated atomically.

### `GET /cluster/shards/{id}`

Per-shard detail (leader, replicas, lag, state).

## Operations

### Adding capacity

1. Bring up `node-d` with `mode = "join"` and the existing cluster's
   peer list as seeds.
2. `POST /cluster/add_node` with d's node_id + addr.
3. `POST /cluster/rebalance` until noop.
4. Shards will drift replicas onto node-d, bumping `generation` once
   per move.

### Draining a node

1. `POST /cluster/remove_node { "node_id": "node-d", "drain": true }`
   — returns `409 Conflict` with `drain pending` if d still hosts
   replicas.
2. Move replicas off via `rebalance` calls.
3. Retry the remove.

### Failover

If a shard's Raft leader goes down, the shard's followers elect a new
leader within 3 × election timeout (default max 3s). Queries already
in flight to the old leader retry automatically up to 3 times (see
`ScatterGatherConfig::max_leader_retries`) against the leader hint
the failed RPC surfaces.

Majority-partition tolerance: a 3-replica shard keeps serving with 1
replica down; a 5-replica shard tolerates 2.

### Monitoring

* `/cluster/status` — authoritative layout + health.
* `/prometheus` — coordinator emits scatter latency, per-shard RPC
  counts, retry counts (integrate when wiring the TCP transport).
* `/replication/status` — per-shard Raft state per node (Phase 2+).

## Failure modes

| Situation | Observable behavior | Recovery |
|-----------|--------------------|----------|
| Shard leader crash | Queries to that shard retry, elect new leader ≤3× election timeout | Automatic |
| Shard majority down | Writes to the shard block, return `ERR_SHARD_FAILURE` | Restore replicas; they rejoin via snapshot install |
| Metadata leader crash | `/cluster/*` mutations return `307` to a follower-hint until a new leader is elected | Automatic |
| Network partition (minority side) | Minority-side replicas step down to followers; writes block until partition heals | Heal network |
| Cached coordinator metadata stale | Shard returns `ERR_STALE_GEN`; coordinator refreshes + retries once | Automatic |
| Runaway variable-length traversal | Coordinator aborts at `max_cross_shard_rpcs_per_query`, returns `ERR_TOO_MANY_REMOTE_FETCHES` | Tune the knob or rewrite the query |

## Storage format changes (**BREAKING**)

Every record-store file gains a 64-byte V2 header
(`NEXUSSHD\0 version:u32 cluster_id:uuid shard_id:u32 generation:u64
...`). Standalone deployments write `shard_id = 0, generation = 0`
and a deterministic `cluster_id` derived from the data directory. A
`nexus migrate --to v2` CLI command rewrites headers in place; data
files are unchanged.

## Out of scope for V2

* Online re-sharding (changing `num_shards` at runtime)
* Cross-shard ACID transactions (2PC)
* Geo-distribution / multi-region latency optimizations
* Read-replica shards with bounded staleness

These are tracked as V2.1 / V3 roadmap items.
