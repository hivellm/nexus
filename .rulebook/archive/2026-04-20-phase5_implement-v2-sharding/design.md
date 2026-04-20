# V2 Sharding & Distribution — Technical Design

## Scope

Horizontal scaling beyond single-node limits via hash-based sharding, per-shard
Raft consensus, and a distributed query coordinator with scatter/gather
execution. Non-goals for V2: cross-shard transactions, geo-replication,
re-sharding online while serving writes.

## Deployment shapes

Nexus already supports three shapes (**Standalone**, **Cluster-mode multi-tenant**,
**Replication master/replica**). V2 adds a fourth:

```
Standalone              (1 node, 1 tenant)               — default
Cluster-mode            (1 node, N tenants, quotas)      — existing
Replication             (master + N replicas, async WAL) — existing
Sharded cluster         (N shards × R replicas, Raft)    — NEW
```

Shapes compose: a sharded cluster can also run cluster-mode multi-tenant on
each shard. Replication is **replaced** by Raft inside each shard (Raft is
a superset — it gives synchronous replication with stronger guarantees).

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

- **N shards** (fixed at cluster init, configurable; default 1 for single-node).
- Each shard is a **Raft group** of **R replicas** (`R ≥ 1`, default 3).
- Every node may host **0..N** shard replicas. No dedicated "coordinator node" —
  every node can answer coordinator requests and forward to the right shard
  leader.
- **Shard assignment**: `shard_id = xxh3(node_id_u64) mod num_shards`. Relationships
  live with their **source node**'s shard. Cross-shard edges are represented by
  a local *remote-stub* record on both ends (see §Cross-shard traversal).

## Shard metadata

Cluster metadata (shard count, node→shard-replica assignments, generation
number) lives in a dedicated Raft group called **Shard 0 / metadata group**.
This is bootstrapped from a static `cluster.toml` on first start, then
mutated through Raft proposals (add-node, remove-node, rebalance).

Shape:

```rust
struct ClusterMeta {
    cluster_id: Uuid,
    generation: u64,            // monotonic; bumps on any change
    num_shards: u32,
    shards: Vec<ShardGroup>,    // indexed by shard_id
    nodes: HashMap<NodeId, NodeInfo>,
}

struct ShardGroup {
    shard_id: u32,
    members: Vec<NodeId>,       // Raft group members
    leader: Option<NodeId>,     // cached; source of truth is the Raft group
    state: ShardState,          // Active, Migrating(src, dst), Offline
}
```

Generation number is included in every scatter RPC — a stale coordinator gets
`ERR_STALE_GEN` and re-reads metadata before retry.

## Raft consensus (per shard)

Integrates [`openraft`](https://docs.rs/openraft) 0.9 LTS. Each shard owns:

- **Log**: append-only file at `shard-{id}/raft.log`.
- **StateMachine**: the shard's full Nexus storage (record stores + indexes).
  Raft apply = calling the existing `Engine::execute_cypher_with_context` on
  the write path.
- **Snapshots**: reuse `replication::Snapshot` module (already exists, zstd
  + tar, compression level 3).

Transport: custom TCP on a configurable port (default **15480 + shard_id**,
avoids clash with the replication port 15475). Frames share the existing
replication wire format:

```
[message_type:1][length:4][payload:N][crc32:4]
```

…with a `shard_id:u32` header prefixed before the Raft message.

### Leader election

Defaults: election timeout 500–1000ms randomized, heartbeat 100ms. Matches
the existing replication defaults well enough that an operator does not need
to tune two knobs.

### Single-writer preservation

Nexus storage is **single-writer per partition**. Inside a shard this stays
true: only the Raft leader applies writes; followers apply from the log in
order. Crucial invariant: the Raft apply loop is the *only* writer to the
storage layer.

## Distributed query coordinator

Lives in `nexus-core/src/coordinator/`. Pipeline:

1. **Parse** — reuse the existing Cypher parser (`executor::parser`).
2. **Classify** — walk the AST; mark each pattern with the set of shards it
   touches.
   - `MATCH (n:Label {prop: $x})` where `prop` is the sharding key → **single-shard**
   - `MATCH (n:Label)` with no predicates → **broadcast**
   - `MATCH (a)-[r]->(b)` where `a` is on shard S → **route to S** (relationships
     live on the source shard). `b` may be remote → plan a cross-shard
     fetch after the local expand.
3. **Decompose** — split the logical plan into **shard-local subplans** and a
   **coordinator plan**. The coordinator plan is always one of:
   - `SingleShard(S, subplan)` — just forward.
   - `Broadcast(subplan, merge)` — scatter to every shard, apply a merge op.
   - `Targeted(shards, subplan, merge)` — scatter to a subset.
4. **Pushdown** — filters, projections, and `LIMIT` are always pushed into
   the subplan (never into the coordinator). `ORDER BY` with `LIMIT k` uses a
   **top-k merge**: every shard returns its local top-k, the coordinator
   merges and truncates. `COUNT`, `SUM`, `AVG` are decomposed into partial
   aggregations (`COUNT` → sum of partial counts; `AVG` → sum/count pair
   merged as sum/sum).
5. **Execute** — scatter via coordinator RPCs; gather; merge; return.

Scatter/gather timeout: 30s default, configurable. Partial results are
**not** returned — if any shard fails, the whole query fails with
`ERR_SHARD_FAILURE(shard_id, reason)`. V2.1 may add lenient modes.

## Cross-shard traversal

Edge goes from shard A to shard B. Two representations:

**A's store** — full relationship record, `dst_id` points to a node that
lives on shard B. The expand operator calls `coordinator.fetch_remote_node(dst_id)`
when it needs properties or labels of `dst`.

**B's store** — optional *remote stub* node record with `flags |= REMOTE_STUB`,
`label_bits = 0`, `first_rel_ptr` lists edges whose `dst` landed on B but
whose `src` lives on A. Lets reverse-direction expands (`MATCH (b)<-[r]-(a)`)
find the edge starting from B.

Cross-shard cache: LRU keyed by `(shard_id, node_id)`, 10K entries default,
value = `(label_bits, first_rel_ptr, prop_map)`. Invalidated by:

- Raft commits on the owning shard (via a notification channel).
- TTL 30s (safety net).

## Cluster management API

Under `/cluster/*`:

| Method | Path | Description |
|--------|------|-------------|
| GET    | `/cluster/status` | Metadata snapshot: shards, leaders, replicas, lag. |
| POST   | `/cluster/add_node` | `{ node_id, addr }`. Bootstraps the node, assigns it to shards. |
| POST   | `/cluster/remove_node` | `{ node_id, drain: bool }`. Removes from all shards; if `drain`, waits for replicas to catch up. |
| POST   | `/cluster/rebalance` | Triggers a rebalance pass: moves shards off overloaded nodes. |
| GET    | `/cluster/shards/{id}` | Detailed shard state + Raft log offset per replica. |

All mutations go through the metadata Raft group — only the metadata leader
accepts writes, others return `307 Temporary Redirect` to the leader.

## Storage format changes

**Breaking**: every record store file gains a 64-byte header:

```
magic:      8 bytes = "NEXUSSHD"
version:    u32     = 2
cluster_id: 16 bytes (uuid)
shard_id:   u32
generation: u64
reserved:   28 bytes zero
```

Standalone deployments write `shard_id=0, generation=0` and a deterministic
`cluster_id` derived from the data-dir path. Upgrade path: an explicit
`nexus migrate --to v2` CLI command rewrites the headers; data is unchanged.

## Configuration

New section `[cluster.sharding]` in the server config:

```toml
[cluster.sharding]
enabled = true
node_id = "node-a"
listen_addr = "0.0.0.0:15480"
peers = ["node-b=10.0.0.2:15480", "node-c=10.0.0.3:15480"]
num_shards = 3
replica_factor = 3
election_timeout_min_ms = 500
election_timeout_max_ms = 1000
heartbeat_ms = 100
snapshot_log_size_threshold = 10_000
```

## Dependencies

- `openraft = "0.9"` (LTS)
- `xxhash-rust` (already in; used for shard hashing)
- `bincode` (already in; Raft wire format)
- `tokio::net::TcpStream` (already in)

## Rollout / failure modes

- **Shard leader crash**: Raft elects a new leader in ~1s. Scatter RPCs
  in-flight to the old leader retry automatically (up to 3 times) with
  leader hint from the stale-leader error.
- **Network partition on minority side**: minority shard replicas step down
  to followers; writes block until the partition heals. No split-brain
  (Raft majority).
- **Metadata group unavailable**: read queries still work (metadata is
  cached on every node); writes to metadata (add/remove node) block.

## Out of scope for V2

- Online re-sharding (changing `num_shards` at runtime).
- Cross-shard ACID transactions (2PC).
- Read-replica shards with bounded staleness.
- Geo-distribution / multi-region latency optimizations.

These are tracked as V2.1 / V3 roadmap items.
