## 1. Raft TCP Transport

- [x] 1.1 Implement `TcpRaftTransport` implementing `RaftTransport` on `tokio::net::TcpStream` — `crates/nexus-core/src/sharding/raft/tcp_transport.rs`
- [x] 1.2 Wire framing with CRC32 + bincode payload — `codec.rs` (`[shard_id:u32][type:u8=0x40][len:u32][payload][crc32]`, with header/body split + shard-mismatch detection)
- [x] 1.3 Connection pool + reconnect-on-drop per peer — exponential backoff (min 100ms, max 5s); `add_peer` replaces prior writer
- [x] 1.4 Partition-safe backpressure (bounded channel per peer) — default 1024-entry outbound queue; `try_send` drops on Full (Raft tolerates loss)
- [x] 1.5 Unit tests with a two-node loopback harness — 19 tests total (12 codec + 7 tcp_transport including loopback, reconnect-after-restart, remove-peer, idempotent shutdown)

## 2. Coordinator TCP Client

- [x] 2.1 Implement `TcpShardClient` implementing `ShardClient` — `crates/nexus-core/src/coordinator/tcp_client.rs`. Syncronous `execute` bridges to async via `block_in_place + Handle::block_on` (requires multi-thread runtime; documented in module header)
- [x] 2.2 Leader-hint cache shared across scatter cycles — `LeaderCache` (Arc-shared) with `update` / `invalidate` / `get`; `NotLeader` replies auto-update the cache
- [x] 2.3 Deadline-aware `tokio::time::timeout` on connect / write / read — each phase budgets against the caller's `deadline: Instant`
- [x] 2.4 Unit tests against a stub server — 7 tests: wire roundtrip, wrong-type rejection, full RPC round-trip, `NotLeader` cache update, empty members, past-deadline timeout, cache-first fan-out ordering

Wire format: `rmp-serde` (MessagePack, chosen over bincode because `serde_json::Value` in `parameters` / `rows` requires `deserialize_any`, which bincode 1.x does not support) with the same `[shard_id:u32][type:u8][len:u32][payload][crc32]` frame as the Raft transport, type bytes `0x60` (request) / `0x61` (response).

## 3. Server Bootstrap

- [x] 3.1 Parse `ShardingConfig` from `NEXUS_SHARDING_*` env vars — `crates/nexus-server/src/cluster_bootstrap.rs::parse_sharding_env` (MODE / NODE_ID / LISTEN_ADDR / PEERS / NUM_SHARDS / REPLICA_FACTOR). Env form chosen because existing `config.yml` has no sharding schema to extend without cascading tests; YAML field additions are a follow-up rulebook task tracked under Phase 4 of this task
- [x] 3.2 On `Bootstrap`: form the metadata group from peers + spin the metadata `RaftNode` — `MetadataDriver` task owns the node, ticks every 10ms, forwards inbound envelopes, propagates role changes to the controller
- [x] 3.3 On `Join`: dial seeds at `listen_addr + JOIN_PORT_OFFSET`, request metadata snapshot, install locally — `run_join_client` + `run_join_listener` with a dedicated `JoinRequest` / `JoinResponse` pair over rmp-serde
- [x] 3.4 Install the `ClusterController` onto `NexusServer` — `nexus_server.set_cluster_controller(Some(handle.controller))` in main.rs after a successful bootstrap
- [x] 3.5 Expose `TcpShardClient` via `BootstrapHandle.shard_client` — the coordinator's `ScatterGather` picks it up as the production `ShardClient`. Scatter-engine integration into the `/cypher` handler is a follow-up; the client is ready and the wire format is stable.

Tests: 11 new tests covering env parser happy paths + rejection cases, deterministic seed stability, single-node bootstrap with real TCP transport + election observable on `controller.is_leader()`, join protocol roundtripping `ClusterMeta` through a live `TcpListener`, join failing when seeds unreachable.

## 4. Benchmarks

- [ ] 4.1 Wall-clock failover benchmark (3-node, localhost Docker)
- [ ] 4.2 Scatter/gather throughput benchmark (1M queries, read-only)
- [ ] 4.3 Cross-shard traversal latency profile

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 5.1 Update `docs/guides/DISTRIBUTED_DEPLOYMENT.md` with TCP transport operational notes
- [ ] 5.2 Write tests covering the new behavior (unit + 3-node Docker integration test)
- [ ] 5.3 Run tests and confirm they pass
