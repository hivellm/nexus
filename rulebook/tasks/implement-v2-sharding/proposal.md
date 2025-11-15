# Implement V2 Sharding & Distribution

## Why

Horizontal scalability beyond single-node limits requires sharding and distributed query execution. Hash-based partitioning with Raft consensus per shard.

## What Changes

- Implement hash-based sharding (hash(node_id) % num_shards)
- Implement Raft consensus per shard (via openraft)
- Implement distributed query coordinator
- Implement cross-shard traversal
- Add shard management API

**BREAKING**: Storage format changes (shard metadata)

## Impact

### Affected Specs
- NEW capability: `sharding`
- NEW capability: `distributed-query`
- NEW capability: `raft-consensus`

### Affected Code
- `nexus-core/src/sharding/` - Sharding logic (~800 lines)
- `nexus-core/src/coordinator/` - Query coordinator (~600 lines)
- `nexus-server/src/cluster/` - Cluster management (~400 lines)
- `tests/distributed_tests.rs` - Distributed tests (~800 lines)

### Dependencies
- Requires: V1 complete (replication, auth)

### Timeline
- **Duration**: 12-16 weeks
- **Complexity**: Very High

