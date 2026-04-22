# Proposal: phase5_v2-tcp-transport-bridge

## Why

The V2 sharding core (`phase5_implement-v2-sharding`) ships with a
fully-tested in-process [`InMemoryTransport`] that covers single-host
deployments and every integration scenario. To run a real multi-host
sharded cluster we still need the TCP bridge that lets the per-shard
Raft groups and the coordinator's RPCs hop between machines using the
bincode wire format already defined in
[`nexus-core/src/sharding/raft/types.rs`](../../../nexus-core/src/sharding/raft/types.rs).

The reason this is a follow-up task rather than part of the core
implementation: the V2 core is ~3,500 lines of tested logic that
stands on its own; the TCP bridge is a substantially smaller I/O
adapter that plugs into the existing `RaftTransport` trait and the
existing `ShardClient` trait without touching any of the core
algorithms. Landing the core first keeps the reviews focused and
the commit graph linear.

## What Changes

- **Raft TCP transport** implementing `RaftTransport` on top of
  `tokio::net::TcpStream` with the existing wire framing
  (`[shard_id:u32][message_type:u8][length:u32][payload:N][crc32:u32]`).
  Reuses `replication::protocol` encode/decode helpers.
- **Shard client TCP transport** implementing `ShardClient` that
  dials the owning shard's leader, forwards the scatter request,
  caches leader hints between calls.
- **Bootstrap hooks** in `nexus-server`: when `[cluster.sharding]` is
  populated with `mode = bootstrap | join`, spin up a
  `TcpRaftTransport` on `listen_addr`, connect to `peers`, and wire
  the transport into the per-shard `RaftNode`s + the coordinator's
  `ShardClient`.
- **Failure wall-clock benchmarks** that exercise leader failover and
  scatter/gather under real network latency. Complements the
  tick-based harness in the core crate.

## Impact

- Affected specs: V2 sharding specs already cover the wire format;
  this task adds the I/O layer beneath them. No new spec requirements.
- Affected code:
  - `nexus-core/src/sharding/raft/tcp_transport.rs` (new, ~400 LOC)
  - `nexus-server/src/cluster_bootstrap.rs` (new, ~250 LOC)
  - `nexus-core/src/coordinator/tcp_client.rs` (new, ~200 LOC)
  - `nexus-server/src/main.rs` — cluster bootstrap wiring
- Breaking change: NO (sharding is already opt-in via `[cluster.sharding]`)
- User benefit: multi-host sharded cluster deployments become possible
  without any code changes to existing sharded-cluster operators —
  same config shape, real network underneath.
