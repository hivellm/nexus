## 1. Raft TCP Transport

- [ ] 1.1 Implement `TcpRaftTransport` implementing `RaftTransport` on `tokio::net::TcpStream`
- [ ] 1.2 Reuse `replication::protocol` framing with CRC32 + bincode payload
- [ ] 1.3 Connection pool + reconnect-on-drop per peer
- [ ] 1.4 Partition-safe backpressure (bounded channel per peer)
- [ ] 1.5 Unit tests with a two-node loopback harness

## 2. Coordinator TCP Client

- [ ] 2.1 Implement `TcpShardClient` implementing `ShardClient`
- [ ] 2.2 Leader-hint cache shared across scatter cycles
- [ ] 2.3 Deadline-aware `tokio::select!` on response vs timeout
- [ ] 2.4 Unit tests against a stub server using `tokio_test`

## 3. Server Bootstrap

- [ ] 3.1 Parse `[cluster.sharding]` into `ShardingConfig`
- [ ] 3.2 On `Bootstrap`: spin the metadata `RaftNode`, form group
- [ ] 3.3 On `Join`: dial seeds, receive metadata snapshot, sync
- [ ] 3.4 Install the `ClusterController` onto `NexusServer`
- [ ] 3.5 Wire coordinator `ShardClient` = `TcpShardClient` instance

## 4. Benchmarks

- [ ] 4.1 Wall-clock failover benchmark (3-node, localhost Docker)
- [ ] 4.2 Scatter/gather throughput benchmark (1M queries, read-only)
- [ ] 4.3 Cross-shard traversal latency profile

## 5. Tail (mandatory — enforced by rulebook v5.3.0)

- [ ] 5.1 Update `docs/guides/DISTRIBUTED_DEPLOYMENT.md` with TCP transport operational notes
- [ ] 5.2 Write tests covering the new behavior (unit + 3-node Docker integration test)
- [ ] 5.3 Run tests and confirm they pass
